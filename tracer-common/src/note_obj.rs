use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Envelope {
   pub owner: String,
   pub req: Req,
}

/// 两阶段增量同步请求：
/// 阶段一 Meta 分块发全量元数据，阶段二 Push 按服务器要求分块补差异全量行；
/// last=true 代表该阶段最后一块，中间帧一律 lockstep 一发一收，避免流控死锁
#[derive(Debug, Serialize, Deserialize)]
pub enum Req {
    // 阶段一：全量元数据块（含墓碑）
    Meta { metas: Vec<NoteMeta>, last: bool },
    // 阶段二：差异全量行块
    Push { notes: Vec<ZNote>, last: bool },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum SendNoteBodyEnum {
    // 中间块确认
    Ack,
    // 服务器->客户端的差异全量行块（此期间客户端只读不发）
    Give(Vec<ZNote>),
    // 阶段一完成：需要客户端补内容的 id + 合并后的墓碑赢家
    Need {
        need_push: Vec<i64>,
        tombstones: Vec<NoteMeta>,
    },
    // 同步完成
    Done,
}

/// 同步元数据：足够做 LWW 合并判定（hash 覆盖 title+content，元数据相等即语义相等）；
/// 墓碑传播不需要内容
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteMeta {
    pub id: i64,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub hash: String,
    pub deleted: bool,
}

impl From<&ZNote> for NoteMeta {
    fn from(n: &ZNote) -> Self {
        Self {
            id: n.id,
            created_at: n.created_at,
            updated_at: n.updated_at,
            hash: n.hash.clone(),
            deleted: n.deleted,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZNote {
    pub id: i64,
    pub owner: String,
    pub title: Option<String>,
    pub content: String,
    pub created_at: Option<i64>,
    pub hash: String,
    pub updated_at: Option<i64>,
    // 墓碑标记：true 代表已删除，作为一行状态参与同步合并
    pub deleted: bool,
}

/// 分块单帧预算（framed 上限 4MiB，留余量）；单条笔记自身超 4MiB 不支持
pub const SYNC_CHUNK_BYTES: usize = 2 * 1024 * 1024;

pub fn meta_size(m: &NoteMeta) -> usize {
    48 + m.hash.len()
}

pub fn note_size(n: &ZNote) -> usize {
    128 + n.content.len() + n.title.as_deref().map_or(0, str::len) + n.hash.len()
}

/// 按估算字节数分块；空输入返回空 vec（调用方自行决定是否发空结束帧）
pub fn chunk_by<T>(items: Vec<T>, size: impl Fn(&T) -> usize) -> Vec<Vec<T>> {
    let mut chunks = Vec::new();
    let mut cur: Vec<T> = Vec::new();
    let mut cur_bytes = 0;
    for item in items {
        let s = size(&item);
        if !cur.is_empty() && cur_bytes + s > SYNC_CHUNK_BYTES {
            chunks.push(std::mem::take(&mut cur));
            cur_bytes = 0;
        }
        cur_bytes += s;
        cur.push(item);
    }
    if !cur.is_empty() {
        chunks.push(cur);
    }
    chunks
}