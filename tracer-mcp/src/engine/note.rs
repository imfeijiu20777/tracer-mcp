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
