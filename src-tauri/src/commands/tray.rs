//! 托盘命令（↔ api/settings.ts traySetCloseToTray）：设置对话框修改关闭到托盘行为。

use crate::error::AppError;
use crate::tray;

/// 设置关闭到托盘行为（运行时状态 + 托盘菜单勾选态同步）
#[tauri::command]
#[specta::specta]
pub fn tray_set_close_to_tray(enabled: bool) -> Result<(), AppError> {
    tray::set_close_to_tray(enabled);
    Ok(())
}
