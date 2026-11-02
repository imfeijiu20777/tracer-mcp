use std::path::PathBuf;
use std::sync::Arc;
use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;
use mcp::TracerMcp;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
mod mcp;

/// 启动参数：`-data` / `-secret` 可指定路径，不传则回退到默认值；`-h` 查看帮助
#[derive(Parser, Debug)]
#[command(name = "tracer-mcp", about = "Tracer MCP server")]
struct Cli {
    /// Blobs data directory; falls back to env TRACET_SERVER_DATA_PATH or cwd/tracer_server_data
    #[arg(short = 'd', long = "data")]
    data: Option<PathBuf>,

    /// Secret key file path; falls back to env TRACET_SERVER_SECRET_KEY or cwd/tracer_server_secret_key.key
    #[arg(short = 's', long = "secret")]
    secret: Option<PathBuf>,

    /// HTTP port to listen on
    #[arg(short = 'p', long = "port", default_value_t = 8000)]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();
    // 没有传递参数时回退到 common 的默认路径解析逻辑
    let secret_path = cli
        .secret
        .unwrap_or_else(|| common::get_secret_path(common::Role::Server));
    let data_path = cli
        .data
        .unwrap_or_else(|| common::get_data_path(common::Role::Server));
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], cli.port));
    info!(secret = %secret_path.display(), data = %data_path.display(), %addr, "tracer mcp start!");
    let tracer = TracerMcp::new(secret_path, data_path).await?;
    // Streamable HTTP 传输：每个客户端会话通过工厂函数克隆出一个 TracerMcp 实例
    let service = StreamableHttpService::new(
        move || Ok(tracer.clone()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    let tcp_listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(tcp_listener, router).await?;
    info!("tracer mcp stop!");
    Ok(())
}

fn init_tracing() {
    tracing_subscriber::fmt()
        // 用 RUST_LOG 环境变量控制级别，默认 info
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)   // ★ 核心：写 stderr，不是默认的 stdout
        .with_ansi(false)               // 关掉颜色转义码，避免污染日志文件/客户端解析
        .init();
}