use tokio::sync::{Mutex, RwLock};
use anyhow::Result;
use iroh_blobs::ticket::BlobTicket;
use std::path::Path;
use tracing_subscriber::EnvFilter;
use crate::service::RecordService;
use common::{Role, get_data_path, payload::Payload};
use tauri::{Emitter, Manager};
use tracing::{debug,info,warn};
// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod service;
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}
struct AppState {
    service: Mutex<Option<RecordService>>,
    name:RwLock<Option<String>>
    
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tauri::Builder::default()
        .manage(AppState{service:Mutex::new(None),name:RwLock::new(None)})
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            greet,
             init_ticket,
              query_records,
              set_name,
              download])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
fn init_tracing() {
    tracing_subscriber::fmt()
        // 用 RUST_LOG 环境变量控制级别，默认 info
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();
}
#[tauri::command]
async fn init_ticket(
    ticket: String,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    // ① 解构出 service 和 receiver（错误转成 String，否则编译不过：
    //    iroh 的错误类型没实现 serde::Serialize，不能直接 ?）
    // blobs 存储目录放在应用数据目录下，注意要在 app 被 move 进 spawn 之前取
    let data_path = get_data_path(Role::Client);
    info!(data_path = ?data_path, "using client data path");
    let (service, mut rx) = RecordService::new(ticket, data_path)
        .await
        .map_err(|e| e.to_string())?;
    info!("init RecordService success");
    // ② spawn 消费循环，把消息推给前端
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            // RecordEvent 需要 #[derive(serde::Serialize)] 或 serde_json 能序列化
            if let Err(e) = app.emit("record-event", &event) {
                eprintln!("emit failed: {}", e);
            }
        }
    });
    // ③ 短持锁写入 State
    let _ = state.service.lock().await.insert(service);
    Ok(())
}

/// 查询当前连接已接收并解析的所有消息记录
#[tauri::command]
async fn query_records(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Payload>, String> {
    let guard = state.service.lock().await;
    let service = guard.as_ref().ok_or("service not initialized, call init_ticket first")?;
    Ok(service.query_records().await)
}
/// 调用设置名称
#[tauri::command]
async fn set_name(name: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    // 记录到本地状态
    let _ = state.name.write().await.insert(name.clone());
    // 通过写端把名称以 Payload::Name 发送给服务端
    let mut guard = state.service.lock().await;
    let service = guard.as_mut().ok_or("service not initialized, call init_ticket first")?;
    service.send_message(Payload::Name(name)).await.map_err(|e| e.to_string())
}
/// 进行下载
///
/// `Payload::FileNotice` 是枚举的一个变体（值级别的路径），并不是一个类型，
/// 所以不能直接写在参数位置做类型标注。这里改成接收下载真正需要的字段：
/// `ticker` 是 FileNotice 里携带的 BlobTicket 编码字符串，解析后交给下载器，
/// 导出到 `out_file_name` 指定的本地路径。
/// `out_file_name` 为前端通过保存对话框选出的完整路径（含文件名）。
/// `task_id` 是前端为本次下载生成的唯一任务 id，后端用它推送进度并拒绝重复提交。
#[tauri::command]
async fn download(
    task_id: String,
    ticker: String,
    out_file_name: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let guard = state.service.lock().await;
    let service = guard.as_ref().ok_or("service not initialized, call init_ticket first")?;
    let ticket = ticker.parse::<BlobTicket>().map_err(|e| e.to_string())?;
    service
        .download(task_id, ticket, Path::new(&out_file_name))
        .await
        // 完整错误链透传给调用方，避免 iroh-blobs 吞掉底层 os error 详情
        .map_err(|e| format!("{e:#}"))?;
    Ok(())
}