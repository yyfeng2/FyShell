//! 串口终端命令层：serial_list / serial_connect / serial_write / serial_disconnect。
//! 薄层转发 + 操作 AppState，业务逻辑在 services/serial.rs。

use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::error::AppError;
use crate::services::serial;
use crate::state::AppState;

/// 枚举本机串口列表（连接表单下拉选择用）
#[tauri::command]
#[specta::specta]
pub fn serial_list() -> Result<Vec<serial::SerialPortInfo>, AppError> {
    serial::list()
}

/// 打开串口并启动读写循环；终端输出流走 Channel。
/// id：连接路由键（串口连接无持久会话，每标签唯一，由前端生成）。
/// port_name：串口名（如 "COM3"）；baud_rate：波特率（默认 115200）。
#[tauri::command]
#[specta::specta]
pub async fn serial_connect(
    id: String,
    port_name: String,
    baud_rate: Option<u32>,
    on_output: Channel<Vec<u8>>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    if id.trim().is_empty() {
        return Err(AppError::general("连接路由键 id 不能为空"));
    }
    if port_name.trim().is_empty() {
        return Err(AppError::general("串口名不能为空"));
    }
    // 默认 115200
    let baud_rate = baud_rate.unwrap_or(115_200);
    serial::connect(
        &app,
        state.inner(),
        &id,
        &port_name,
        baud_rate,
        on_output,
    )
    .await
}

/// 关闭并清理串口会话
#[tauri::command]
#[specta::specta]
pub fn serial_disconnect(id: String, state: State<'_, AppState>) -> Result<(), AppError> {
    serial::disconnect(state.inner(), &id);
    Ok(())
}

/// 键盘输入写入（按连接键路由）
#[tauri::command]
#[specta::specta]
pub fn serial_write(id: String, data: Vec<u8>, state: State<'_, AppState>) -> Result<(), AppError> {
    serial::write(state.inner(), &id, &data)
}

/// 串口会话存活查询：Rust 侧是否持有该会话连接句柄
#[tauri::command]
#[specta::specta]
pub fn serial_alive(id: String, state: State<'_, AppState>) -> bool {
    serial::alive(state.inner(), &id)
}
