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

    #[tool(description = "Get the connection ticket")]
    async fn get_ticker(&self) -> String {
        self.service.get_ticker()
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
}

#[tool_handler]
impl ServerHandler for TracerMcp {
    fn get_info(&self) -> ServerConfig {
        let mut config = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        config.instructions = Some("Tracer tool service".into());
        config
    }
}
