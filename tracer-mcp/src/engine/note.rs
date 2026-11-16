use anyhow::Result;
use std::sync::Arc;

use common::{
    framed::{FramedBiStream, recv_msg, send_msg},
    note_obj::{Envelope, NoteMeta, Req, SendNoteBodyEnum, ZNote, chunk_by, note_size},
    simpledb::{sync_phase1, upsert_notes},
};
use tokio::sync::Mutex;
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use rusqlite;

#[derive(Debug, Clone)]
pub struct NodeManage {
    // 关联连接
    conn: Arc<Mutex<rusqlite::Connection>>,
}
impl NodeManage {
    pub fn new(conn: Arc<Mutex<rusqlite::Connection>>) -> Result<Self> {
        return Ok(Self { conn });
    }
}

impl ProtocolHandler for NodeManage {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let bi = match connection.accept_bi().await {
            Ok(bi) => bi,
            Err(e) => {
                return Err(e.into());
            }
        };
        let mut framed = FramedBiStream::new(bi);
        let conn: Arc<Mutex<rusqlite::Connection>> = self.conn.clone();
        // 把 connection 移入 task，用 connection.closed() 做可靠的断连检测
        tokio::spawn(async move {
            // 每连接一个两阶段同步会话：累积元数据/差异分块
            let mut session = SyncSession::default();
            loop {
                tokio::select! {
                    biased;

                    // 1. 读一条消息
                    msg_result = recv_msg::<Envelope>(&mut framed.read) => {
                        match msg_result {
                            Ok(msg) => {
                                // 先在锁内同步构造响应并释放锁，再异步发送。
                                // rusqlite::Connection 是 Send 但不是 Sync，db 引用不能跨 await 持有，
                                // 否则 tokio::spawn 要求整个 future 为 Send 会编译失败
                                let resps = {
                                    let db = conn.lock().await;
                                    handle_request(&db, &mut session, &msg)
                                };
                                match resps {
                                    Ok(resps) => {
                                        for resp in resps {
                                            if let Err(e) = send_msg(&mut framed.write, &resp).await {
                                                tracing::warn!("发送响应失败: {e}");
                                                break;
                                            }
                                        }
                                    }
                                    Err(e) => tracing::warn!("处理请求失败: {e}"),
                                }
                            }
                            Err(e) => {
                                // 流上的任何错误（EOF、断连、解码失败）都视为该流不可恢复，退出
                                tracing::info!("连接流结束: {e}");
                                break;
                            }
                        }
                    }

                    // 2. 整条 QUIC 连接关闭：权威断连信号
                    _err = connection.closed() => {
                        tracing::info!("QUIC 连接已关闭，退出 note 同步任务");
                        break;
                    }
                }
            }
        });
        Ok(())
    }
}

/// 每连接的两阶段同步会话状态
#[derive(Default)]
struct SyncSession {
    metas: Vec<NoteMeta>,
    push: Vec<ZNote>,
}

/// 根据请求构造响应（纯同步，不做 IO）；一个请求可能对应多帧响应（Give 连发 + Need）
fn handle_request(
    db: &rusqlite::Connection,
    session: &mut SyncSession,
    body: &Envelope,
) -> Result<Vec<SendNoteBodyEnum>> {
    match &body.req {
        // 阶段一：累积元数据块；最后一块合并出计划，
        // 服务器赢家差异全量行分块连发（期间客户端只读不发），最后 Need
        Req::Meta { metas, last } => {
            session.metas.extend(metas.iter().cloned());
            if !last {
                return Ok(vec![SendNoteBodyEnum::Ack]);
            }
            let plan = sync_phase1(db, &body.owner, &session.metas)?;
            session.metas.clear();
            let mut out: Vec<SendNoteBodyEnum> = chunk_by(plan.give, note_size)
                .into_iter()
                .map(SendNoteBodyEnum::Give)
                .collect();
            out.push(SendNoteBodyEnum::Need {
                need_push: plan.need_push,
                tombstones: plan.tombstones,
            });
            Ok(out)
        }
        // 阶段二：累积差异块；最后一块批量写库回 Done
        Req::Push { notes, last } => {
            session.push.extend(notes.iter().cloned());
            if !last {
                return Ok(vec![SendNoteBodyEnum::Ack]);
            }
            upsert_notes(db, &body.owner, &session.push)?;
            session.push.clear();
            Ok(vec![SendNoteBodyEnum::Done])
        }
    }
}