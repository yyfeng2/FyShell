//! 本地终端命令层：local_shell_connect / local_shell_write / local_shell_resize /
//! local_shell_disconnect。薄层转发 + 操作 AppState，业务逻辑在 services/local_shell.rs。

use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::error::AppError;
use crate::services::local_shell;
use crate::state::AppState;

/// 建立本地终端连接并打开 PTY/shell；终端输出流走 Channel。
/// conn_id：连接路由键（本地终端无持久会话，每标签唯一，由前端生成）。
#[tauri::command]
#[specta::specta]
pub async fn local_shell_connect(
    conn_id: String,
    on_output: Channel<Vec<u8>>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    if conn_id.trim().is_empty() {
        return Err(AppError::general("连接路由键 conn_id 不能为空"));
    }
    local_shell::connect(&app, state.inner(), &conn_id, on_output).await
}

/// 关闭并清理本地终端会话/PTY（标签关闭时调用，Rust 侧同步清理）
#[tauri::command]
#[specta::specta]
pub fn local_shell_disconnect(id: String, state: State<'_, AppState>) -> Result<(), AppError> {
    local_shell::disconnect(state.inner(), &id);
    Ok(())
}

/// 键盘输入写入（按连接键路由）
#[tauri::command]
#[specta::specta]
pub fn local_shell_write(
    id: String,
    data: Vec<u8>,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    local_shell::write(state.inner(), &id, &data)
}

/// 终端尺寸变更（按连接键路由 resize）
#[tauri::command]
#[specta::specta]
pub fn local_shell_resize(
    id: String,
    cols: u16,
    rows: u16,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    local_shell::resize(state.inner(), &id, cols, rows)
}

/// 本地终端会话存活查询：Rust 侧是否持有该会话连接句柄
#[tauri::command]
#[specta::specta]
pub fn local_shell_alive(id: String, state: State<'_, AppState>) -> bool {
    local_shell::alive(state.inner(), &id)
}
