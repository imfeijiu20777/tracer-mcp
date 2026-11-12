use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Envelope {
    owner: String,
    req: Req,
}

#[derive(Serialize, Deserialize)]
pub enum Req {
    Init,
    Diff(Vec<(String, u64)>),
    Push(Vec<String>),
    Del(Vec<String>),
}
#[derive(Debug, PartialEq)]
pub struct ZNote {
    pub id: i64,
    pub owner: String,
    pub title: Option<String>,
    pub content: String,
    pub created_at: Option<i64>,
    pub hash: String,
    pub updated_at: Option<i64>,
}