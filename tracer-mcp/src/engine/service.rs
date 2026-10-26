use std::path::{Path, PathBuf};
use std::{collections::HashMap, sync::Arc};

use crate::engine::transport::{ZTransportEvent, ZTransports};
use anyhow::{Result, anyhow};
use common::{MESSAGE_ALPN, payload::Payload};
use iroh::SecretKey;
use iroh::{Endpoint, endpoint::presets, protocol::Router};
use iroh_blobs::{BlobFormat, BlobsProtocol,Hash};
use iroh_blobs::api::Store;
use iroh_blobs::api::blobs::{AddPathOptions, ImportMode};
use iroh_blobs::ticket::BlobTicket;
use iroh_tickets::Ticket;
use iroh_tickets::endpoint::{EndpointTicket};
use tokio::sync::{RwLock, broadcast};
use tracing::{info, warn};
/// 这个地方的目的是为了对消息进行一定的转换，并提供核心的管理状态等逻辑
pub struct ZTransportHub {
    store:Store,
    endpoint: Endpoint,
    _router:Router,
    transport: ZTransports,
    // id:name
    addrs: Arc<RwLock<HashMap<String, u64>>>,
}

impl ZTransportHub {
    /// 初始化并创建监控循环
    pub async fn new(secert_key:PathBuf,fs_db:PathBuf) -> Result<Self> {
        tracing::info!("secert_key path: {}", secert_key.display());
        tracing::info!("fs_db path: {}", fs_db.display());
        let secert=load_or_create_key(secert_key)?;
        let endpoint = Endpoint::builder(presets::N0).secret_key(secert).bind().await?;
        endpoint.online().await;
        // FsStore 实现了 Into<Store>，这里转换成对外暴露的 api::Store 以便与结构体字段类型一致
        let store = Store::from(iroh_blobs::store::fs::FsStore::load(fs_db).await?);
        let transport = ZTransports::new();
        let _router = Router::builder(endpoint.clone())
            .accept(MESSAGE_ALPN, transport.clone())
            .accept(iroh_blobs::ALPN, BlobsProtocol::new(&store, None))
            .spawn();
        let body = Self {
            store,
            endpoint: endpoint.clone(),_router,
            transport: transport.clone(),
            addrs: Arc::new(RwLock::new(HashMap::new())),
        };
        body.listen(transport.subscribe())?;
        Ok(body)
    }
      /// 依据endpoint 返回单据
    pub fn get_ticker(&self)->String{
          let ticket = EndpointTicket::new(self.endpoint.clone().addr());
          ticket.encode_string()
    }
    /// 判断是否有这个人
    pub async fn has_user(&self,name: &str)->bool{
        self.addrs.read().await.contains_key(name)
    }
    pub async  fn get_all_user(&self)->Vec<String>{
        self.addrs.read().await.keys().cloned().collect()
    }
    /// 分享
    pub async fn share(&self, path: &Path) -> Result<(BlobTicket,String)> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        // 用带选项的流式接口拿到导入进度，mode 使用 TryReference
        let options = AddPathOptions {
            path: path.to_path_buf(),
            mode: ImportMode::TryReference,
            format: BlobFormat::Raw,
        };
        let tag_info = self.store.add_path_with_opts(options).await.inspect_err(|err| {
            eprintln!("import failed: {err:?}");
        })?;
        let blob_ticket=self.ticket(tag_info.hash, tag_info.format).await?;
        Ok((blob_ticket,name))
    }

    pub async fn ticket(&self, hash: Hash, format: BlobFormat) -> Result<BlobTicket> {
        self.endpoint.online().await;
        let addr = self.endpoint.addr();
        Ok(BlobTicket::new(addr, hash, format))
    }
    
    /// 依据名字进行消息的发送
    pub async fn send_to(&self, name: &str, send_body: Payload) -> Result<()> {
        let id = {
            let addrs = self.addrs.read().await; // guard 绑定到变量，延长生命周期
            let Some(id) = addrs.get(name) else {
                return Err(anyhow!("no such peer: {name}")); // 必须发散
            };
            id.clone() // 复制出值，guard 之后可以安全释放
        }; // guard 在这里 drop
        match self.transport.get_userbox(id).await {
            Some(body) => {
              let mut userbox=  body.lock().await;
              userbox.set_name(name.to_string());
              userbox.send_message(send_body.to_string()).await?;
            }
            None => {
                return Err(anyhow!("no such user: {name}")); // 必须发散
            }
        }
        Ok(())
    }

    fn listen(&self, mut receiver: broadcast::Receiver<ZTransportEvent>) -> Result<()> {
        let addrs = self.addrs.clone();
        tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => match event {
                        ZTransportEvent::MessageReceived { id, msg } => {
                            match Payload::parse(&msg) {
                                Payload::Name(name) => {
                                    info!(%name, "user registered");
                                    addrs.write().await.insert(name, id);
                                }
                                _ => {}
                            }
                        }
                        ZTransportEvent::DisConnected(zid) => {
                            let mut addrs = addrs.write().await;
                            let name = addrs
                                .iter()
                                .find(|(_, v)| **v == zid)
                                .map(|(k, _)| k.clone());

                            if let Some(name) = name {
                                addrs.remove(&name);
                                info!("event channel closed, removed user {}", name);
                            }
                        }
                        _ => {}
                    },
                    Err(broadcast::error::RecvError::Lagged(missed)) => {
                        // 滞后了：跳过这些消息，继续消费
                        warn!(missed, "event consumer lagged, skipped some messages");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        // Sender 全部关闭：真正该退出的时候
                        info!("event channel closed, exiting dispatch loop");
                        break;
                    }
                }
            }
        });
        Ok(())
    }

    
}

fn load_or_create_key(path: impl AsRef<Path>) -> anyhow::Result<SecretKey> {
    let path = path.as_ref(); 

    match std::fs::read(path) {
        Ok(bytes) => {
            let bytes: [u8; 32] = bytes
                .try_into()
                .map_err(|v: Vec<u8>| { 
                    anyhow::anyhow!(
                        "{} is not 32 bytes (got {} bytes); the file may be corrupted",
                        path.display(),
                        v.len()
                    )
                })?;
            Ok(SecretKey::from_bytes(&bytes))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // ✅ 只在"文件不存在"时才生成新 key
            let key = SecretKey::generate();
            std::fs::write(path, key.to_bytes())?;
            // 不能 println!：stdio 模式下 stdout 是 JSON-RPC 通道，日志走 tracing(stderr)
            info!("generated new secret key at {}", path.display());
            Ok(key)
        }
        Err(e) => Err(e.into()),  // ✅ 其他错误（权限、磁盘等）如实上报
    }
}