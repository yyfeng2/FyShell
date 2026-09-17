//! 快捷命令命令（契约第 5.2 节）：薄层，参数校验 + 调用 quick_command_store。
//!
//! 发送到会话复用既有 `ssh_write`，无新命令（契约 5.2 备注）。
//! 存储经模块级注册表（services/quick_command_store），不依赖 AppState。

use crate::error::AppError;
use crate::models::quick_command::{QuickCommand, QuickCommandFolder, QuickCommandNode};
use crate::services::quick_command_store;

/// 快捷命令/文件夹树（扁平：文件夹+命令混合节点，前端按 groupId/parentId 组树）
///
/// 契约签名为 `()`。
#[tauri::command]
#[specta::specta]
pub fn qc_list() -> Result<Vec<QuickCommandNode>, AppError> {
    quick_command_store::list_nodes()
}

/// 保存快捷命令（新命令生成 id）
#[tauri::command]
#[specta::specta]
pub fn qc_save_command(mut cmd: QuickCommand) -> Result<QuickCommand, AppError> {
    if cmd.name.trim().is_empty() {
        return Err(AppError::general("命令名称不能为空"));
    }
    if cmd.command_text.trim().is_empty() {
        return Err(AppError::general("命令内容不能为空"));
    }
    if cmd.id.trim().is_empty() {
        cmd.id = uuid::Uuid::new_v4().to_string();
    }
    quick_command_store::save_command(&cmd)?;
    Ok(cmd)
}

/// 保存快捷命令文件夹（新文件夹生成 id）
#[tauri::command]
#[specta::specta]
pub fn qc_save_folder(mut folder: QuickCommandFolder) -> Result<QuickCommandFolder, AppError> {
    if folder.name.trim().is_empty() {
        return Err(AppError::general("文件夹名称不能为空"));
    }
    if folder.id.trim().is_empty() {
        folder.id = uuid::Uuid::new_v4().to_string();
    }
    quick_command_store::save_folder(&folder)?;
    Ok(folder)
}

/// 删除快捷命令或文件夹（is_folder 区分，前端已二次确认）
#[tauri::command]
#[specta::specta]
pub fn qc_delete(id: String, is_folder: bool) -> Result<(), AppError> {
    if id.trim().is_empty() {
        return Err(AppError::general("id 不能为空"));
    }
    if is_folder {
        quick_command_store::delete_folder(&id)
    } else {
        quick_command_store::delete_command(&id)
    }
}
