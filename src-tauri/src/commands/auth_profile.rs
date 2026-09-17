//! 认证配置文件命令（契约第 5.2 节）：薄层，参数校验 + 调用 auth_profile 注册表。

use crate::error::AppError;
use crate::models::auth_profile::AuthProfile;
use crate::services::auth_profile;

/// 列出全部认证配置文件
#[tauri::command]
pub fn auth_profile_list() -> Result<Vec<AuthProfile>, AppError> {
    auth_profile::list()
}

/// 保存认证配置文件（新配置文件生成 id；一处改全局生效）
#[tauri::command]
pub fn auth_profile_save(mut profile: AuthProfile) -> Result<AuthProfile, AppError> {
    if profile.name.trim().is_empty() {
        return Err(AppError::general("配置文件名称不能为空"));
    }
    if profile.id.trim().is_empty() {
        profile.id = uuid::Uuid::new_v4().to_string();
    }
    auth_profile::save(&profile)?;
    Ok(profile)
}

/// 删除认证配置文件（引用它的会话不删除，profile_id 引用被置空）
#[tauri::command]
pub fn auth_profile_delete(id: String) -> Result<(), AppError> {
    if id.trim().is_empty() {
        return Err(AppError::general("id 不能为空"));
    }
    auth_profile::delete(&id)
}
