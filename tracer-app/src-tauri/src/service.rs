use anyhow::{Context, Result};
use futures_lite::stream::StreamExt;
use iroh_blobs::{
    api::{
        Store,
        blobs::{ExportMode, ExportOptions},
        downloader::{DownloadProgressItem, Downloader},
    },
    store::fs::FsStore,
    ticket::BlobTicket,
};
use std::{
    collections::HashSet,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{
    RwLock,
    mpsc::{self, Receiver, Sender},
};
use tracing::{debug, info, warn};

/// 下载停滞保护：超过该时长没有任何进度事件即判定失败，避免无限转圈
const DOWNLOAD_STALL_TIMEOUT: Duration = Duration::from_secs(30);

use common::{
    MESSAGE_ALPN,
    framed::{FramedBiStream, WriteHalf, recv_str, send_str},
    payload::Payload,
};
use iroh::{
    Endpoint,
    endpoint::{PathEvent, presets},
};
use iroh_tickets::{Ticket, endpoint::EndpointTicket};
#[derive(Debug)]
/// 连接后的记录器
pub struct RecordService {
    store: Store,
    server_ticket: EndpointTicket,
    // 连接端的名称
    name: Option<String>,
    downloader: Downloader,
    // 写端
    write: WriteHalf,
    // 对应的地址状态
    endpoint: Endpoint,
    // 解析好后的消息放在这个地方
    record_vec: Arc<RwLock<Vec<Payload>>>,
    // 发给前端监听循环的事件写端，下载进度复用同一个通道
    events: Sender<RecordEvent>,
    // 已受理过的下载任务 id，保证同一个 id 只会被使用一次
    seen_download_task_ids: Arc<RwLock<HashSet<String>>>,
}

impl RecordService {
    /// 即开始创建连接
    pub async fn new(
        ticket: String,
        data_path: impl AsRef<Path>,
    ) -> Result<(Self, Receiver<RecordEvent>)> {
        info!("get ticket {}", ticket);
        let (event_tx, event_rx) = mpsc::channel::<RecordEvent>(64);
        let service_event_tx = event_tx.clone();
        let end_ticket = EndpointTicket::decode_string(&ticket)?;
        let store = FsStore::load(data_path).await?;
        let endpoint = Endpoint::bind(presets::N0).await?;
        let downloader: Downloader = Downloader::new(store.as_ref(), &endpoint);
        endpoint.online().await;
        // 注意：这里只是本地 endpoint上线，不是与对端建立连接
        info!(local_id = %endpoint.id(), "local endpoint online, connecting to peer");
        let connection = match endpoint.connect(end_ticket.clone(), MESSAGE_ALPN).await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "endpoint.connect failed, closing local endpoint");
                endpoint.close().await;
                return Err(anyhow::anyhow!("connect failed: {e}"));
            }
        };
        info!(remote = %connection.remote_id(), "QUIC connection established, opening bi stream");

        let bi = match connection.open_bi().await {
            Ok(bi) => bi,
            Err(e) => {
                warn!(error = %e, "open_bi failed, closing endpoint");
                endpoint.close().await;
                return Err(anyhow::anyhow!("open_bi failed: {e}"));
            }
        };
        info!("open_bi success");
        let mut framed = FramedBiStream::new(bi);
        let record_vec = Arc::new(RwLock::new(Vec::new()));
        let record_vec_clone = record_vec.clone();
        let mut path_clone = connection.path_events();
        let endpoint_in_task = endpoint.clone();
        // 连接建立后，按当前选中的路径计算初始连接类型（中继 / P2P）并通知前端
        let initial_conn_type = connection
            .paths()
            .iter()
            .find(|p| p.is_selected())
            .map(|p| {
                if p.is_relay() {
                    ConnType::Relay
                } else {
                    ConnType::P2p
                }
            })
            .unwrap_or(ConnType::P2p);
        let _ = service_event_tx
            .send(RecordEvent::Connection {
                conn_type: initial_conn_type,
            })
            .await;
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // 分支返回的 result 本身不能成句，这里不再影响逻辑
                    biased; // 多个等待的情况下 从上到下依次执行，而不是随机选择一个分支
                    // 1. 正常消息接收
                    result = recv_str(&mut framed.read) => {
                        match result {
                            Ok(msg) => {
                                let _=event_tx.send(RecordEvent::MessageUpdate()).await;
                                let pbody=Payload::parse(&msg);
                                info!("message {}",pbody);
                                record_vec_clone.write().await.push(pbody);
                                info!(
                                    msg_len = msg.len(),
                                    "message received from server"
                                );

                            }
                            Err(e) => {
                                let _=event_tx.send(RecordEvent::DisConnection()).await;
                                warn!(
                                    error = %e,
                                    "message receive failed, disconnecting"
                                );
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
                                    "path selected, data transfer switched to this path"
                                );
                            }
                            PathEvent::Opened { id, remote_addr, local_addr, .. } => {
                                info!(
                                    %id,
                                    %remote_addr,
                                    ?local_addr,
                                    "new path opened"
                                );
                            }
                            PathEvent::Closed { id, remote_addr, last_stats, .. } => {
                                info!(
                                    %id,
                                    %remote_addr,
                                    rtt = ?last_stats.rtt,
                                    "path closed"
                                );
                            }
                            PathEvent::Lagged { missed, .. } => {
                                warn!(
                                    missed,
                                    "path event consumer lagged, some events were missed"
                                );
                            }
                            // PathEvent 是 #[non_exhaustive]，兜底分支必须保留
                            other => {
                                debug!(
                                    event = ?other,
                                    "received unhandled path event variant"
                                );
                            }
                        }
                        // 任一路径事件后，从当前选中的路径重算连接类型（中继 / P2P）并通知前端
                        let conn_type = connection
                            .paths()
                            .iter()
                            .find(|p| p.is_selected())
                            .map(|p| {
                                if p.is_relay() {
                                    ConnType::Relay
                                } else {
                                    ConnType::P2p
                                }
                            })
                            .unwrap_or(ConnType::P2p);
                        let _ = event_tx
                            .send(RecordEvent::Connection { conn_type })
                            .await;
                    }

                    // 3. 整条 QUIC 连接关闭：这才是可靠的断开信号
                    err = connection.closed() => {
                        let _=event_tx.send(RecordEvent::DisConnection()).await;
                        warn!(
                            error = ?err,
                            "QUIC connection closed"
                        );
                        break;
                    }
                }
            }
            // 注意：`closed()` 只是等待关闭完成的 future，不会触发关闭
            // 必须调用 `close()` 才会真正优雅关闭 Endpoint
            endpoint_in_task.close().await;
            info!("background task closed the endpoint");
        });
        Ok((
            Self {
                downloader,
                store: Store::from(store),
                server_ticket: end_ticket,
                name: None,
                write: framed.write,
                endpoint,
                record_vec: record_vec,
                events: service_event_tx,
                seen_download_task_ids: Arc::new(RwLock::new(HashSet::new())),
            },
            event_rx,
        ))
    }

    /// 查询已解析并存储的所有消息记录
    pub async fn query_records(&self) -> Vec<Payload> {
        self.record_vec.read().await.clone()
    }

    /// 使用写端 (WriteHalf) 向服务端发送一条消息
    pub async fn send_message(&mut self, payload: Payload) -> Result<()> {
        let msg = payload.to_string();
        send_str(&mut self.write, &msg).await?;
        info!(msg_len = msg.len(), "message sent to server");
        Ok(())
    }

    /// 显式优雅关闭服务，释放 Endpoint 资源
    /// 在不再需要 RecordService 时（如窗口关闭 / 主动断开）应调用此方法
    pub async fn shutdown(&self) {
        info!("shutting down RecordService endpoint...");
        self.endpoint.close().await;
        info!("RecordService endpoint closed");
    }
    /// 按照单据进行下载，并把下载进度通过 `record-event` 推给前端。
    ///
    /// `task_id` 由前端为一次下载任务生成，后端只负责透传给进度事件，
    /// 同时拒绝同一个 id 被重复提交。
    pub async fn download(
        &self,
        task_id: String,
        ticket: BlobTicket,
        out_path: &Path,
    ) -> anyhow::Result<u64> {
        let task_id = task_id.trim().to_owned();
        if task_id.is_empty() {
            anyhow::bail!("download task id must not be empty");
        }
        if !self
            .seen_download_task_ids
            .write()
            .await
            .insert(task_id.clone())
        {
            anyhow::bail!("download task id already used: {task_id}");
        }

        let _ = self
            .events
            .send(RecordEvent::DownloadProgress {
                task_id: task_id.clone(),
                downloaded: 0,
            })
            .await;

        let result = self.run_download(&task_id, &ticket, out_path).await;
        if let Err(err) = &result {
            // iroh-blobs 的错误 Display 只输出最外层（如 "inner error: Error::Io"），
            // 真正的 io/os 错误详情在 source 链里，必须用 {:#} 展开整条链，
            // 否则前端只能看到没有信息量的 "inner error: Error::Io"
            let _ = self
                .events
                .send(RecordEvent::DownloadFailed {
                    task_id: task_id.clone(),
                    error: format!("{err:#}"),
                })
                .await;
        }
        result
    }

    async fn run_download(
        &self,
        task_id: &str,
        ticket: &BlobTicket,
        out_path: &Path,
    ) -> anyhow::Result<u64> {
        // ① 本地 store 已完整（上次已下载过）：跳过下载流直接导出，避免重复走网络卡住
        let already_complete = self.store.has(ticket.hash()).await?;
        if !already_complete {
            // ② 交给下载器:它会请求该 hash 对应的全部切片、校验并拼装好
            let dl = self
                .downloader
                .download(ticket.hash_and_format(), [ticket.addr().id]);
            let mut stream = dl.stream().await?;
            let mut last_progress_at = Instant::now();
            let mut last_downloaded = 0_u64;
            loop {
                // 停滞保护：长时间没有任何进度事件即判定失败
                match tokio::time::timeout(DOWNLOAD_STALL_TIMEOUT, stream.next()).await {
                    Ok(Some(item)) => match item {
                        DownloadProgressItem::Progress(downloaded) => {
                            last_downloaded = downloaded;
                            if last_progress_at.elapsed() >= Duration::from_millis(100) {
                                last_progress_at = Instant::now();
                                let _ = self
                                    .events
                                    .send(RecordEvent::DownloadProgress {
                                        task_id: task_id.to_owned(),
                                        downloaded,
                                    })
                                    .await;
                            }
                        }
                        DownloadProgressItem::Error(err) => {
                            anyhow::bail!("download progress stream error: {err}");
                        }
                        DownloadProgressItem::DownloadError => {
                            anyhow::bail!("download failed");
                        }
                        DownloadProgressItem::TryProvider { id, .. } => {
                            info!(task_id, ?id, "trying provider");
                        }
                        DownloadProgressItem::ProviderFailed { id, .. } => {
                            warn!(task_id, ?id, "provider download failed");
                        }
                        DownloadProgressItem::PartComplete { .. } => {
                            debug!(task_id, "partial download request complete");
                        }
                    },
                    Ok(None) => break,
                    Err(_) => anyhow::bail!(
                        "download stalled: no progress for over {}s, aborting",
                        DOWNLOAD_STALL_TIMEOUT.as_secs()
                    ),
                }
            }
            if last_downloaded > 0 {
                let _ = self
                    .events
                    .send(RecordEvent::DownloadProgress {
                        task_id: task_id.to_owned(),
                        downloaded: last_downloaded,
                    })
                    .await;
            }
        } else {
            info!(task_id, "blob already complete locally, skipping download and exporting directly");
        }

        let options = ExportOptions {
            hash: ticket.hash(),
            // 用 Copy 而非 TryReference：
            // 1) TryReference 对 store 自有数据走 fs::rename 移动，Windows 上目标文件
            //    已存在（os error 183）或被占用时不覆盖，且不在其跨设备 fallback
            //    白名单内，会直接失败；
            // 2) rename 成功后 meta 会把数据记为 External 指向导出路径，用户移动/删除
            //    该文件会让 store 悬空，has() 仍返回 true，下次跳过下载后导出必失败。
            mode: ExportMode::Copy,
            target: out_path.to_owned(),
        };
        let size = self
            .store
            .export_with_opts(options)
            .await
            .map_err(anyhow::Error::new)
            .with_context(|| format!("failed to export file to {}", out_path.display()))?;
        let _ = self
            .events
            .send(RecordEvent::DownloadFinished {
                task_id: task_id.to_owned(),
                size,
            })
            .await;
        info!(%size, path = ?out_path, "blob exported to file");
        Ok(size)
    }
}

/// 兜底：当 RecordService 被 drop 但调用方忘记显式 shutdown 时，
/// 通过 spawn 一个后台任务异步调用 close()，避免
/// "Endpoint dropped without calling `Endpoint::close`" 错误。
impl Drop for RecordService {
    fn drop(&mut self) {
        let endpoint = self.endpoint.clone();
        // 只有在 tokio 运行时上下文中才能 spawn；若运行时已退出则忽略
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::spawn(async move {
                endpoint.close().await;
            });
        }
    }
}

/// 当前数据传输路径的连接类型，序列化后给前端展示（英文）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnType {
    /// 通过中继服务器转发
    Relay,
    /// P2P 直连
    P2p,
}

#[derive(Debug, serde::Serialize)]
pub enum RecordEvent {
    // 有消息来
    MessageUpdate(),
    // 关闭连接
    DisConnection(),
    // 连接类型变化：前端据此显示绿点并展示 Relay / P2P
    Connection { conn_type: ConnType },
    // 下载进度：前端按 task_id 找到对应进度条并渲染 downloaded
    DownloadProgress { task_id: String, downloaded: u64 },
    // 下载并导出完成：size 为最终写入文件的字节数
    DownloadFinished { task_id: String, size: u64 },
    // 下载失败：error 可直接展示给前端
    DownloadFailed { task_id: String, error: String },
}
