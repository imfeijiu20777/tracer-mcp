use std::sync::Arc;
use anyhow::Result;

use common::framed::FramedBiStream;
use futures_util::lock::Mutex;
use iroh::{endpoint::Connection, protocol::{AcceptError, ProtocolHandler}};
use iroh_blobs::ticket;
use rusqlite;
#[derive(Debug,Clone)]
pub struct NodeManage{
    // 关联连接
    conn:Arc<Mutex<rusqlite::Connection>>
}
impl NodeManage {
    pub fn new(conn:rusqlite::Connection)->Result<Self>{
        return Ok(Self { conn:Arc::new(Mutex::new(conn))  });
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
        let conn=self.conn.clone();
        tokio::spawn(async move{
            
        })
        Ok(())
     }
}
#[derive(Serialize, Deserialize, Clone, Copy)]
#[repr(u8)]
enum MsgType {
    INIT = 0,
    Text = 1,
    Note = 2,
}

#[derive(Serialize, Deserialize)]
struct Frame {
    msg_type: MsgType,   // type：1 字节
    name:String,
    body: Vec<u8>,       // body：内层 postcard 编码的字节
}