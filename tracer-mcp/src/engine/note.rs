use anyhow::Result;
use std::sync::Arc;

use common::{
    framed::{FramedBiStream, recv_msg, send_msg},
    note_obj::{
        Envelope,
        Req::{self},
        SendNoteBodyEnum,
    },
};
use futures_util::lock::Mutex;
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use rusqlite;

use crate::zdb::simpledb::{compare_hashes, delete_notes, list_notes_by_name, upsert_notes};
#[derive(Debug, Clone)]
pub struct NodeManage {
    // 关联连接
    conn: Arc<Mutex<rusqlite::Connection>>,
}
impl NodeManage {
    pub fn new(conn: rusqlite::Connection) -> Result<Self> {
        return Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        });
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
        tokio::spawn(async move {
            loop {
                // 读一条消息
                let msg: Envelope = match recv_msg(&mut framed.read).await {
                    Ok(msg) => msg,
                    Err(e) => {
                        // 从 anyhow::Error 里取出原始错误，判断是不是 IO 层的断流
                        let is_stream_dead = e
                            .downcast_ref::<std::io::Error>()
                            .map(|io| {
                                matches!(
                                    io.kind(),
                                    std::io::ErrorKind::UnexpectedEof
                                        | std::io::ErrorKind::BrokenPipe
                                        | std::io::ErrorKind::ConnectionReset
                                )
                            })
                            .unwrap_or(false);

                        if is_stream_dead {
                            tracing::info!("流已关闭: {e}");
                            break; // 真断流：退出任务
                        }
                        // 只是这一帧坏了（解码失败等）：记日志，收下一帧
                        tracing::warn!("单条消息损坏，跳过: {e}");
                        continue;
                    }
                };
                // 先在锁内同步构造响应并释放锁，再异步发送。
                // rusqlite::Connection 是 Send 但不是 Sync，db 引用不能跨 await 持有，
                // 否则 tokio::spawn 要求整个 future 为 Send 会编译失败
                let resp = {
                    let db = conn.lock().await;
                    build_response(&db, &msg)
                };
                match resp {
                    Ok(resp) => {
                        if let Err(e) = send_msg(&mut framed.write, &resp).await {
                            tracing::warn!("发送响应失败: {e}");
                        }
                    }
                    Err(e) => tracing::warn!("处理请求失败: {e}"),
                }
            }
        });
        Ok(())
    }
}
/// 根据请求构造响应（纯同步，不做 IO）
fn build_response(db: &rusqlite::Connection, body: &Envelope) -> Result<SendNoteBodyEnum> {
    match &body.req {
        // 初始化：将服务端该用户名下的全部笔记发送给客户端
        Req::Init => Ok(SendNoteBodyEnum::InitList(list_notes_by_name(db, &body.owner)?)),
        // 差异比对：客户端发来 (uuid, hash) 列表，服务器比对同 uuid 但 hash 不同的笔记，
        // 把这些不一致笔记的 id 通过 NeedPush 回复客户端，要求客户端把完整笔记推送回来
        Req::Diff(list) => Ok(SendNoteBodyEnum::NeedPush(compare_hashes(db, &body.owner, list)?)),
        // 推送：客户端按 Diff 结果把需要同步的笔记发过来，
        // id 与服务器一致的执行更新，不一致的创建新记录（upsert）
        Req::Push(notes) => {
            upsert_notes(db, &body.owner, notes)?;
            Ok(SendNoteBodyEnum::Done)
        }
        // 删除：按客户端要求删除指定 id 的笔记
        Req::Del(ids) => {
            delete_notes(db, &body.owner, ids)?;
            Ok(SendNoteBodyEnum::Done)
        }
    }
}
