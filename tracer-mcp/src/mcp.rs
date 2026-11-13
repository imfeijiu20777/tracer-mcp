use anyhow::Result;
use rmcp::ErrorData;
use common::payload::Payload;
use iroh_tickets::Ticket;
use rmcp::{
    ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{ Parameters},
    },
    model::{ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};
use std::{path::Path, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;
use tracer_mcp::engine::service::ZTransportHub;
use tracing::info;

/// send_file 工具的入参
#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct SendFileParams {
    /// 目标用户名
    #[schemars(description = "Target user name")]
    name: String,

    /// 要发送的文件的完整路径，例如 /tmp/a.txt
    #[schemars(description = "Full path of the file to send, e.g. /tmp/a.txt")]
    file_path: String,
}

/// send_note 工具的入参
#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct SendNoteParams {
    /// 目标用户名（必须在线）
    #[schemars(description = "Target user name (must be online)")]
    name: String,

    /// 可选的笔记标题
    #[schemars(description = "Optional note title")]
    title: Option<String>,

    /// 笔记内容
    #[schemars(description = "Note content")]
    content: String,
}

#[derive(Clone)]
pub struct TracerMcp {
    tool_router: ToolRouter<Self>,
    service: Arc<ZTransportHub>,
}

#[tool_router]
impl TracerMcp {
    /// 创建 MCP 服务：`secret_path` / `data_path` 由 main 解析命令行参数后透传，
    /// 缺省路径的解析逻辑也在那边完成（common::get_*_path）
    pub async fn new(secret_path: PathBuf, data_path: PathBuf) -> Result<Self> {
        info!("TracerMcp init");
        let z = ZTransportHub::new(secret_path, data_path).await?;
        let service = Arc::new(z);
        Ok(Self {
            service,
            tool_router: Self::tool_router(),
        })
    }

    #[tool(description = "Get the server node public key (stable node id)")]
    async fn get_node_id(&self) -> String {
        self.service.get_node_id()
    }
    #[tool(description = "List all currently registered users")]
    async fn user_all(&self) -> String {
        self.service.get_all_user().await.join(",")
    }
    #[tool(description = "Send a file to a specific user")]
    async fn send_file(
        &self,
        Parameters(params): Parameters<SendFileParams>,
    ) -> std::result::Result<(), ErrorData> {
        let service = self.service.clone();
        if !service.has_user(&params.name).await {
            return Err(ErrorData::invalid_params(
                format!("user not found: {}", params.name),
                None,
            ));
        }
        let path = Path::new(&params.file_path);
        let size = std::fs::metadata(path)
    .map_err(|e| ErrorData::internal_error(format!("metadata: {e}"), None))?
    .len();
        let (ticket, file_name) = service
            .share(path)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        service
            .send_to(
                &params.name,
                Payload::FileNotice {
                    filename: file_name,
                    ticker: ticket.encode_string(),
                    size,
                },
            )
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))
    }

    #[tool(description = "Create a note and send it to a specific online user")]
    async fn send_note(
        &self,
        Parameters(params): Parameters<SendNoteParams>,
    ) -> std::result::Result<String, ErrorData> {
        if !self.service.has_user(&params.name).await {
            return Err(ErrorData::invalid_params(
                format!("user not online: {}", params.name),
                None,
            ));
        }
        let id = self
            .service
            .create_note_for(&params.name, params.title.as_deref(), &params.content)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        Ok(id.to_string())
    }
}

#[tool_handler]
impl ServerHandler for TracerMcp {
    fn get_info(&self) -> ServerConfig {
        let mut config = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        config.instructions = Some("Tracer tool service".into());
        config
    }
}
