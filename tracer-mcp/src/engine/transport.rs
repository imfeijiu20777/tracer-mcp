use anyhow::Result;
use common::framed::{FramedBiStream, WriteHalf, recv_str, send_str};
use futures_lite::stream::StreamExt;
use iroh::{
    EndpointId,
    endpoint::{Connection, PathEvent},
    protocol::{AcceptError, ProtocolHandler},
};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::{Mutex, RwLock, broadcast};
use tracing::{debug, info};
/// 即个人信息等信息包裹在了这个地方
/// 实际的文件发送等也是交给他进行
#[derive(Debug)]
pub struct UserBox {
    // 连接端的名称
    name: Option<String>,
    // 写端
    write: WriteHalf,
    // 对应的地址状态
    _endpoint_id: EndpointId,
    // 连接模式
    _line_model: LineMode,
    // 即当前发送消息的消息下标
    _message_index: u64,
}
impl UserBox {
    /// 初始化
    fn new(write: WriteHalf, endpoint_id: EndpointId) -> Self {
        Self {
            name: None,
            write,
            _endpoint_id:endpoint_id,
            _line_model: LineMode::Init,
            _message_index: 0,
        }
    }
    fn set_line_model(&mut self, line_mode: LineMode) {
        self._line_model = line_mode;
    }
    fn set_index(&mut self, index: u64) {
        self._message_index = index;
    }
    pub fn set_name(&mut self, name: String) {
        self.name = Some(name);
    }
    pub async fn send_message(&mut self, message: String) -> Result<()> {
        send_str(&mut self.write, &message).await
    }
}
/// ip 为直连
/// relay 为中继
/// Closed 为关闭
/// Init 为正在初始化中
#[derive(Debug)]
pub enum LineMode {
    Ip,
    Relay,
    Closed,
    Init,
}
#[derive(Debug,Clone)]
pub struct ZTransports {
    /// 即整个进行广播的东西，主要还是用来做对外的一个广播
    event_tx: broadcast::Sender<ZTransportEvent>,
    // 整个连接器
    connections: Arc<RwLock<HashMap<u64, Arc<Mutex<UserBox>>>>>,
    // 下一个的编码id
    next_id: Arc<std::sync::atomic::AtomicU64>
}
impl ZTransports {
    /// 创建
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(30);
        Self {
            event_tx,
            connections: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }
    /// 订阅返回
    pub fn subscribe(&self)->broadcast::Receiver<ZTransportEvent>{
        self.event_tx.subscribe()
    }

    pub async fn get_userbox(&self, id: u64) -> Option<Arc<Mutex<UserBox>>> {
        self.connections.read().await.get(&id).cloned()
    }

}

impl ProtocolHandler for ZTransports {
    /// 连接过来之后进行实现处理
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let u_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let bi = match connection.accept_bi().await {
            Ok(bi) => bi,
            Err(e) => {
                // 对端不再开新流（正常收尾）：退出即可，不是错误
                // 注意：不能 eprintln，stderr 会打花上层 TUI 界面
                return Err(e.into());
            }
        };

        let mut framed = FramedBiStream::new(bi);
        let event_clone = self.event_tx.clone();
        let mut path_clone = connection.path_events();
        let user_box = UserBox::new(framed.write, connection.remote_id());
        self.connections.write().await.insert(u_id, Arc::new(Mutex::new(user_box)));
        let connection_clone = self.connections.clone();
        let _ = self.event_tx.send(ZTransportEvent::Init(u_id));

        tokio::spawn(async move {
            info!(connection_id = u_id, "connection task started, processing messages");
            loop {
                tokio::select! {
                    // 分支返回的 result 本身不能成句，这里不再影响逻辑
                    biased; // 多个等待的情况下 从上到下依次执行，而不是随机选择一个分支

                    // 1. 正常消息接收
                    result = recv_str(&mut framed.read) => {
                        match result {
                            Ok(msg) => {
                                info!(
                                    connection_id = u_id,
                                    msg_len = msg.len(),
                                    "message received from client"
                                );
                                if let Err(e) = event_clone
                                    .send(ZTransportEvent::MessageReceived { id: u_id, msg })
                                {
                                    info!(
                                        connection_id = u_id,
                                        error = %e,
                                        "upstream receiver closed, stopping connection task"
                                    );
                                    break;
                                }
                            }
                            Err(e) => {
                                info!(
                                    connection_id = u_id,
                                    error = %e,
                                    "message receive failed, disconnecting"
                                );
                                if let Err(e) = event_clone.send(ZTransportEvent::DisConnected(u_id)) {
                                    debug!(connection_id = u_id, error = %e, "failed to send disconnect event, upstream may have exited");
                                }
                                connection_clone.write().await.remove(&u_id);
                                info!(connection_id = u_id, "removed disconnected connection from table");
                                break;
                            }
                        }
                    }

                    // 2. 路径事件：任一路径事件都从连接当前状态重算连接类型
                    Some(event) = path_clone.next() => {
                        match event {
                            PathEvent::Selected { id, remote_addr, local_addr, .. } => {
                                info!(
                                    %id,
                                    %remote_addr,
                                    ?local_addr,
                                    connection_id = u_id,
                                    "path selected, data transfer switched to this path"
                                );
                            }
                            PathEvent::Opened { id, remote_addr, local_addr, .. } => {
                                info!(
                                    %id,
                                    %remote_addr,
                                    ?local_addr,
                                    connection_id = u_id,
                                    "new path opened"
                                );
                            }
                            PathEvent::Closed { id, remote_addr, last_stats, .. } => {
                                info!(
                                    %id,
                                    %remote_addr,
                                    rtt = ?last_stats.rtt,
                                    connection_id = u_id,
                                    "path closed"
                                );
                            }
                            PathEvent::Lagged { missed, .. } => {
                                info!(
                                    missed,
                                    connection_id = u_id,
                                    "path event consumer lagged, some events were missed"
                                );
                            }
                            // PathEvent 是 #[non_exhaustive]，兜底分支必须保留
                            other => {
                                info!(
                                    connection_id = u_id,
                                    event = ?other,
                                    "received unhandled path event variant"
                                );
                            }
                        }
                    }

                    // 3. 整条 QUIC 连接关闭：这才是可靠的断开信号
                    err = connection.closed() => {
                        info!(
                            connection_id = u_id,
                            error = ?err,
                            "QUIC connection closed"
                        );
                        if let Err(e) = event_clone.send(ZTransportEvent::DisConnected(u_id)) {
                            info!(connection_id = u_id, error = %e, "failed to send disconnect event, upstream may have exited");
                        }
                        connection_clone.write().await.remove(&u_id);
                        info!(connection_id = u_id, "removed disconnected connection from table");
                        break;
                    }
                }
            }

            info!(connection_id = u_id, "connection task exited");
        });
        Ok(())
    }
}
/// 从iroh 中接收到的放在这个地方
#[derive(Debug, Clone)]
pub enum ZTransportEvent {
    // 枚举变体
    MessageReceived { id: u64, msg: String },
    Init(u64),
    DisConnected(u64),
}
