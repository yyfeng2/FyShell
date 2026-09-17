//! MySQL 备份/还原 + 自动运行命令（对齐 Navicat 备份功能）：薄层转发到
//! services/mysql_backup。
//!
//! 运行历史记录策略：本轮「自动运行 = 保存任务配置 + 立即运行 + 运行历史」，
//! 真实定时调度（cron）不在本轮范围——每次 mysql_backup 执行完成（成功/失败）
//! 后由本命令层记录一条运行历史（手动备份的 profile_id 为 None）。

use uuid::Uuid;

use crate::error::AppError;
use crate::models::mysql_backup::{MySqlBackupProfile, MySqlBackupRun};
use crate::services;

/// `mysql_backup` (conn_id: String, tables: Option<Vec<String>>, file_path: String,
/// include_data: Option<bool>, include_create: Option<bool>) -> u64
///
/// - `tables` None = 全库所有表；
/// - `include_data` None 默认 true（含 INSERT 数据）；
/// - `include_create` 对应服务层的 include_drop 开关（true = 附带
///   DROP TABLE IF EXISTS；建表结构始终导出）。
/// 返回导出的数据行数。
#[tauri::command]
pub async fn mysql_backup(
    conn_id: String,
    tables: Option<Vec<String>>,
    file_path: String,
    include_data: Option<bool>,
    include_create: Option<bool>,
) -> Result<u64, AppError> {
    let outcome = services::mysql_backup::backup(
        &conn_id,
        tables.as_deref(),
        &file_path,
        include_data.unwrap_or(true),
        include_create.unwrap_or(false),
    )
    .await;
    // 执行完成后记录运行历史（尽力而为，历史记录失败不影响主流程返回）
    match &outcome {
        Ok(rows_total) => {
            let _ = services::mysql_backup::run_history_add(None, &file_path, *rows_total, true);
        }
        Err(_) => {
            let _ = services::mysql_backup::run_history_add(None, &file_path, 0, false);
        }
    }
    outcome
}

/// `mysql_restore` (conn_id: String, file_path: String [, confirmed: bool]) -> u64
///
/// 覆盖确认由前端负责（与 mysql_import 的 confirmed 流程一致），后端直接执行。
/// 返回总受影响行数。
#[tauri::command]
pub async fn mysql_restore(
    conn_id: String,
    file_path: String,
    confirmed: Option<bool>,
) -> Result<u64, AppError> {
    services::mysql_backup::restore(&conn_id, &file_path, confirmed).await
}

/// `mysql_backup_profile_list` () -> Vec<MySqlBackupProfile>
#[tauri::command]
pub async fn mysql_backup_profile_list() -> Result<Vec<MySqlBackupProfile>, AppError> {
    // SQLite 操作为亚毫秒级阻塞，async 命令内直接调用（与 tunnel 同策略）
    services::mysql_backup::profile_list()
}

/// `mysql_backup_profile_save` (id: Option<String>, name: String,
/// conn_host: Option<String>, tables: Option<Vec<String>>, include_data: bool,
/// include_create: bool) -> ()
///
/// id None = 新建（生成 uuid）；conn_host = 关联的 MySQL 连接标识（host 描述，
/// 可选，前端省略时存空串）；tables None = 全库（空表）。
#[tauri::command]
pub async fn mysql_backup_profile_save(
    id: Option<String>,
    name: String,
    conn_host: Option<String>,
    tables: Option<Vec<String>>,
    include_data: bool,
    include_create: bool,
) -> Result<(), AppError> {
    let profile = MySqlBackupProfile {
        id: id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        name,
        // conn_host 为档案关联的连接标识/描述（展示用；实际执行时前端
        // 传 conn_id 给 mysql_backup，经连接池定位）
        conn_host: conn_host.unwrap_or_default(),
        tables: tables.unwrap_or_default(),
        include_data,
        include_create,
        created_at: chrono::Local::now().to_rfc3339(),
    };
    services::mysql_backup::profile_save(&profile)
}

/// `mysql_backup_profile_delete` (id: String) -> ()
#[tauri::command]
pub async fn mysql_backup_profile_delete(id: String) -> Result<(), AppError> {
    services::mysql_backup::profile_delete(&id)
}

/// `mysql_backup_run_list` () -> Vec<MySqlBackupRun>
///
/// 运行历史列表（最近 100 条，倒序）。
#[tauri::command]
pub async fn mysql_backup_run_list() -> Result<Vec<MySqlBackupRun>, AppError> {
    services::mysql_backup::run_history_list(100)
}
