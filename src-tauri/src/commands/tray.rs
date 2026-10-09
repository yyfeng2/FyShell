//! 托盘命令（↔ api/settings.ts traySetCloseToTray）：设置对话框修改关闭到托盘行为。

use crate::error::AppError;
use crate::tray;

/// 设置关闭到托盘行为（运行时状态 + 托盘图标按需显隐）；AppHandle 由 Tauri 注入，
/// JS 调用形态与 bindings 均不破坏。
#[tauri::command]
#[specta::specta]
pub fn tray_set_close_to_tray(app: tauri::AppHandle, enabled: bool) -> Result<(), AppError> {
    tray::apply_close_to_tray(&app, enabled);
    Ok(())
}
