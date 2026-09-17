//! 服务器监控命令层：契约第 5.2 节的 4 个命令（monitor_start / monitor_stop /
//! docker_list / docker_operate），薄层转发（参数校验 + 调用 services）。
//!
//! 监控采样经 `Channel<MonitorSample>` 推送，无新增事件（契约第 5.3 节）。

use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::error::AppError;
use crate::services::monitor::{DockerContainer, MonitorSample};
use crate::services::{monitor as monitor_service};
use crate::state::AppState;

/// 开始采集（SSH exec /proc）：采样经 Channel 推送，返回即已开始
#[tauri::command]
pub async fn monitor_start(
    app: AppHandle,
    id: String,
    interval_secs: u32,
    on_sample: Channel<MonitorSample>,
) -> Result<(), AppError> {
    monitor_service::start(&app, &id, interval_secs, on_sample)
}

/// 停止采集（按会话 ID 路由）
#[tauri::command]
pub fn monitor_stop(id: String) -> Result<(), AppError> {
    monitor_service::stop(&id);
    Ok(())
}

/// Docker 容器列表：`Vec<DockerContainer>`
#[tauri::command]
pub async fn docker_list(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<DockerContainer>, AppError> {
    monitor_service::docker_list(&state, &id).await
}

/// 容器操作：action 为 start / stop / restart
#[tauri::command]
pub async fn docker_operate(
    state: State<'_, AppState>,
    id: String,
    container_id: String,
    action: String,
) -> Result<(), AppError> {
    monitor_service::docker_operate(&state, &id, &container_id, &action).await
}
