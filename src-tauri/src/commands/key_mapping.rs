//! 键位映射命令：薄层，参数校验 + 调用 key_mapping_store。
//!
//! 拦截在前端 xterm attachCustomKeyEventHandler（发送复用既有 ssh_write），
//! 菜单命令经 ui store 分发到 WorkspaceView。存储经模块级注册表（services/key_mapping_store）。

use crate::error::AppError;
use crate::models::key_mapping::{KeyMapping, ACTION_MENU_COMMAND, ACTION_SEND_STRING};
use crate::services::key_mapping_store;

/// 键位映射列表
#[tauri::command]
#[specta::specta]
pub fn key_mapping_list() -> Result<Vec<KeyMapping>, AppError> {
    key_mapping_store::list()
}

/// 保存键位映射（新映射生成 id）
#[tauri::command]
#[specta::specta]
pub fn key_mapping_save(mut mapping: KeyMapping) -> Result<KeyMapping, AppError> {
    if mapping.key_combo.trim().is_empty() {
        return Err(AppError::general("键位不能为空"));
    }
    if mapping.payload.trim().is_empty() {
        return Err(AppError::general("动作内容不能为空"));
    }
    if mapping.action_type != ACTION_SEND_STRING && mapping.action_type != ACTION_MENU_COMMAND {
        return Err(AppError::general("动作类型不支持"));
    }
    if mapping.id.trim().is_empty() {
        mapping.id = uuid::Uuid::new_v4().to_string();
    }
    key_mapping_store::save(&mapping)?;
    Ok(mapping)
}

/// 删除键位映射（前端已二次确认）
#[tauri::command]
#[specta::specta]
pub fn key_mapping_delete(id: String) -> Result<(), AppError> {
    if id.trim().is_empty() {
        return Err(AppError::general("id 不能为空"));
    }
    key_mapping_store::delete(&id)
}
