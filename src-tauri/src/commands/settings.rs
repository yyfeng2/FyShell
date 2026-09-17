//! 高功能设置命令：薄层，包装 services/settings_store 的读写接口。
//!
//! settings 表结构：key TEXT PRIMARY KEY, value TEXT（值统一为字符串，
//! 由前端按 key 解析为对应类型）。IPC 字段一律 snake_case（契约治理）。

use std::collections::HashMap;

use crate::error::AppError;
use crate::services::settings_store;

/// 全量读取设置项（key -> value 映射；未写入过的键不在结果中，由前端回退默认值）
#[tauri::command]
pub fn settings_get_all() -> Result<HashMap<String, String>, AppError> {
    settings_store::get_all()
}

/// 按 key 读取单个设置项；未设置时返回 Ok(None)
#[tauri::command]
pub fn settings_get(key: String) -> Result<Option<String>, AppError> {
    settings_store::get(&key)
}

/// 写入单个设置项（幂等覆盖；value 统一序列化为字符串）
#[tauri::command]
pub fn settings_set(key: String, value: String) -> Result<(), AppError> {
    settings_store::set(&key, &value)
}
