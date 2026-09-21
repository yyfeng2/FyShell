//! 高功能设置命令：薄层，包装 services/settings_store 的读写接口。
//!
//! settings 表结构：key TEXT PRIMARY KEY, value TEXT（值统一为字符串，
//! 由前端按 key 解析为对应类型）。IPC 字段一律 snake_case（契约治理）。

use std::collections::HashMap;

use crate::error::AppError;
use crate::services::settings_store;

/// 全量读取设置项（key -> value 映射；未写入过的键不在结果中，由前端回退默认值）
#[tauri::command]
#[specta::specta]
pub fn settings_get_all() -> Result<HashMap<String, String>, AppError> {
    settings_store::get_all()
}

/// 按 key 读取单个设置项；未设置时返回 Ok(None)
#[tauri::command]
#[specta::specta]
pub fn settings_get(key: String) -> Result<Option<String>, AppError> {
    settings_store::get(&key)
}

/// 写入单个设置项（幂等覆盖；value 统一序列化为字符串）
#[tauri::command]
#[specta::specta]
pub fn settings_set(key: String, value: String) -> Result<(), AppError> {
    settings_store::set(&key, &value)
}

// ---------------- 会话级 SSH 选项（sshopt_session_{session_id}_{key} 前缀） ----------------

/// 读取会话级 SSH 选项覆盖项（裸 key -> value 映射；未覆盖的键由前端回退全局值）
#[tauri::command]
#[specta::specta]
pub fn sshopt_session_list(session_id: String) -> Result<HashMap<String, String>, AppError> {
    settings_store::session_list(&session_id)
}

/// 写入会话级 SSH 选项覆盖项（覆盖全局值；value 统一序列化为字符串）
#[tauri::command]
#[specta::specta]
pub fn sshopt_session_set(session_id: String, key: String, value: String) -> Result<(), AppError> {
    settings_store::session_set(&session_id, &key, &value)
}

/// 删除会话级 SSH 选项覆盖项（恢复继承全局值）
#[tauri::command]
#[specta::specta]
pub fn sshopt_session_delete(session_id: String, key: String) -> Result<(), AppError> {
    settings_store::session_delete(&session_id, &key)
}

/// 删除全部用户数据（恢复到首次运行状态，不可恢复）：清空全部用户数据表 +
/// 会话日志。含导航树/保存的连接/查询历史/备份档案/快捷命令/键位映射/
/// SFTP 收藏/隧道/认证配置/应用设置/主密码保险库；运行中的内存状态重启后重置。
#[tauri::command]
#[specta::specta]
pub fn user_data_clear() -> Result<(), AppError> {
    settings_store::clear_all_user_data()?;
    crate::services::session_log::clear_all()?;
    Ok(())
}
