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
#[specta::specta]
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
#[specta::specta]
pub fn ssh_disconnect(id: String, app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    ssh::disconnect(state.inner(), &app, &id);
    Ok(())
}

/// 键盘输入写入（按会话 ID 路由）
#[tauri::command]
#[specta::specta]
pub fn ssh_write(id: String, data: Vec<u8>, state: State<'_, AppState>) -> Result<(), AppError> {
    ssh::write(state.inner(), &id, &data)
}

/// 终端尺寸变更（按会话 ID 路由 resize）
#[tauri::command]
#[specta::specta]
pub fn ssh_resize(
    id: String,
    cols: u32,
    rows: u32,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    ssh::resize(state.inner(), &id, cols, rows)
}

/// 控制通道查询会话当前 cwd（SFTP 快捷双栏远程栏初始路径）。
/// 重设计：在 SSH 连接上另开独立 channel 执行 `pwd`，终端通道零字节写入，
/// 不注入任何命令 → 终端物理上不可能闪现注入内容（带内 OSC7 注入已废除）。
#[tauri::command]
#[specta::specta]
pub async fn ssh_query_cwd(id: String, state: State<'_, AppState>) -> Result<String, AppError> {
    ssh::query_cwd(state.inner(), &id).await
}

/// HostKey 首次确认：前端确认后取出 AppState::pending_hostkey 中的
/// oneshot::Sender 并 send(bool)，挂起中的连接验证随之继续或中止。
#[tauri::command]
#[specta::specta]
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

/// 终端 rz/sz 文件传输的用户选择回传：收到 rzsz-start 事件后
/// 前端弹对话框，用户选择接收（local_path 为保存目录）/ 发送（local_path
/// 为本地文件）/ 取消后调用本命令，选择经响应通道传给传输任务。
#[tauri::command]
#[specta::specta]
pub fn rzsz_respond(
    key: String,
    action: String,
    local_path: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let choice = match action.as_str() {
        "recv" => crate::services::rzsz::RzszChoice::Recv { dir: local_path },
        "send" => crate::services::rzsz::RzszChoice::Send { path: local_path },
        "cancel" => crate::services::rzsz::RzszChoice::Cancel,
        other => return Err(AppError::general(format!("未知的 rz/sz 操作: {other}"))),
    };
    let respond_tx = state
        .rzsz_sessions
        .lock()
        .map_err(|_| AppError::general("全局状态锁被污染"))?
        .get(&key)
        .map(|entry| entry.respond_tx.clone());
    match respond_tx {
        Some(tx) => {
            // 任务刚结束时 send 失败无碍（rzsz-end 事件紧随其后）
            let _ = tx.send(choice);
            Ok(())
        }
        None => Err(AppError::general(format!("连接 {key} 没有进行中的 rz/sz 传输"))),
    }
}
