//! SSH 终端命令层：契约第 2 节的 ssh_connect / ssh_disconnect / ssh_write /
//! ssh_resize / ssh_hostkey_accept。薄层转发 + 操作 AppState，业务逻辑在 services/ssh.rs。

use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::error::AppError;
use crate::services::ssh;
use crate::state::AppState;

/// 建立 SSH 连接并打开 PTY/shell；终端输出流走 Channel（契约：`ssh_connect(id, onOutput)`）。
/// conn_id：多标签同会话独立连接的路由键（Xshell 行为：每个标签一条独立连接）。
#[tauri::command]
pub async fn ssh_connect(
    id: String,
    conn_id: Option<String>,
    on_output: Channel<Vec<u8>>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    // 契约只传会话 id：从持久化存储按 id 取出会话配置
    let cfg = state
        .config_store
        .get_session(&id)?
        .ok_or_else(|| AppError::general(format!("会话 {id} 不存在")))?;
    // 连接注册/输出/状态/输入均按连接键路由；未传 conn_id 时退回会话 id
    let key = conn_id.unwrap_or_else(|| id.clone());
    ssh::connect(&app, state.inner(), &cfg, &key, on_output).await
}

/// 关闭并清理 session/PTY（契约红线：标签关闭时 Rust 侧同步清理）
#[tauri::command]
pub fn ssh_disconnect(id: String, state: State<'_, AppState>) -> Result<(), AppError> {
    ssh::disconnect(state.inner(), &id);
    Ok(())
}

/// 键盘输入写入（按会话 ID 路由）
#[tauri::command]
pub fn ssh_write(id: String, data: Vec<u8>, state: State<'_, AppState>) -> Result<(), AppError> {
    ssh::write(state.inner(), &id, &data)
}

/// 终端尺寸变更（按会话 ID 路由 resize）
#[tauri::command]
pub fn ssh_resize(
    id: String,
    cols: u32,
    rows: u32,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    ssh::resize(state.inner(), &id, cols, rows)
}

/// 会话存活查询：Rust 侧是否持有该会话连接句柄（dev 重启/HMR 后
/// 前端状态可能残留，以 Rust 侧为真源校正，避免"以为已连接"而黑屏）
#[tauri::command]
pub fn ssh_alive(id: String, state: State<'_, AppState>) -> bool {
    ssh::alive(state.inner(), &id)
}

/// HostKey 首次确认：前端确认后取出 AppState::pending_hostkey 中的
/// oneshot::Sender 并 send(bool)，挂起中的连接验证随之继续或中止。
#[tauri::command]
pub fn ssh_hostkey_accept(id: String, accept: bool, state: State<'_, AppState>) -> Result<(), AppError> {
    let sender = state
        .pending_hostkey
        .lock()
        .map_err(|_| AppError::general("全局状态锁被污染"))?
        .remove(&id);
    match sender {
        Some(sender) => {
            // 接收方已丢弃时 send 失败无碍（连接已被清理）
            let _ = sender.send(accept);
            Ok(())
        }
        None => Err(AppError::general(format!("会话 {id} 没有待确认的 HostKey"))),
    }
}
