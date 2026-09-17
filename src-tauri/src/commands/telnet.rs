//! Telnet 终端命令层：telnet_connect / telnet_write / telnet_disconnect。
//! 薄层转发 + 操作 AppState，业务逻辑在 services/telnet.rs。

use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::error::AppError;
use crate::services::telnet;
use crate::state::AppState;

/// 建立 Telnet 连接（TCP + 最小 IAC 协商）；终端输出流走 Channel。
/// id：连接路由键（Telnet 连接无持久会话，每标签唯一，由前端生成）。
#[tauri::command]
#[specta::specta]
pub async fn telnet_connect(
    id: String,
    host: String,
    port: Option<u16>,
    on_output: Channel<Vec<u8>>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    if id.trim().is_empty() {
        return Err(AppError::general("连接路由键 id 不能为空"));
    }
    if host.trim().is_empty() {
        return Err(AppError::general("主机地址不能为空"));
    }
    // 默认 23（Telnet 标准端口）
    let port = port.unwrap_or(23);
    telnet::connect(&app, state.inner(), &id, &host, port, on_output).await
}

/// 关闭并清理 Telnet 会话（shutdown 双向关闭 TCP）
#[tauri::command]
#[specta::specta]
pub fn telnet_disconnect(id: String, state: State<'_, AppState>) -> Result<(), AppError> {
    telnet::disconnect(state.inner(), &id);
    Ok(())
}

/// 键盘输入写入（按连接键路由）
#[tauri::command]
#[specta::specta]
pub fn telnet_write(id: String, data: Vec<u8>, state: State<'_, AppState>) -> Result<(), AppError> {
    telnet::write(state.inner(), &id, &data)
}

/// Telnet 会话存活查询：Rust 侧是否持有该会话连接句柄
#[tauri::command]
#[specta::specta]
pub fn telnet_alive(id: String, state: State<'_, AppState>) -> bool {
    telnet::alive(state.inner(), &id)
}
