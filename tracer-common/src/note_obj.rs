use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Envelope {
   pub owner: String,
   pub req: Req,
}
#[derive(Serialize, Deserialize)]
pub enum SendNoteBodyEnum {
    // 初始化全量笔记列表
    InitList(Vec<ZNote>),
    // Diff 结果：要求客户端推送这些 id 的完整笔记（同 id 但 hash 不一致）
    NeedPush(Vec<String>),
    // Push / Del 操作完成
    Done,
}
#[derive(Serialize, Deserialize)]
pub enum Req {
    // 初始化：请求全量笔记
    Init,
    // 差异比对：客户端发来 (id, hash) 列表
    Diff(Vec<(String, String)>),
    // 推送：客户端发来需要同步的完整笔记（id 一致更新，不一致创建）
    Push(Vec<ZNote>),
    // 删除：客户端要求删除的 id 列表
    Del(Vec<String>),
}
#[derive(Debug, PartialEq,Serialize, Deserialize)]
pub struct ZNote {
    pub id: i64,
    pub owner: String,
    pub title: Option<String>,
    pub content: String,
    pub created_at: Option<i64>,
    pub hash: String,
    pub updated_at: Option<i64>,
}