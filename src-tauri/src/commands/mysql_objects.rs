//! MySQL 数据库对象命令（契约第 5.2 节扩展）：薄层转发到 services/mysql_objects。
//!
//! kind 参数为字符串（view / function / procedure / trigger / event），
//! 由服务层 parse_kind 解析为 MySqlObjectKind 后驱动分支。

use crate::error::AppError;
use crate::models::mysql_objects::{MySqlObjectDdl, MySqlObjectInfo};
use crate::services;

/// `mysql_object_list` (conn_id: String, kind: String) -> Vec<MySqlObjectInfo>
#[tauri::command]
#[specta::specta]
pub async fn mysql_object_list(
    conn_id: String,
    kind: String,
) -> Result<Vec<MySqlObjectInfo>, AppError> {
    let kind = services::mysql_objects::parse_kind(&kind)?;
    services::mysql_objects::object_list(&conn_id, kind).await
}

/// `mysql_object_ddl` (conn_id: String, kind: String, name: String) -> MySqlObjectDdl
#[tauri::command]
#[specta::specta]
pub async fn mysql_object_ddl(
    conn_id: String,
    kind: String,
    name: String,
) -> Result<MySqlObjectDdl, AppError> {
    let kind = services::mysql_objects::parse_kind(&kind)?;
    services::mysql_objects::object_ddl(&conn_id, kind, &name).await
}

/// `mysql_object_save` (conn_id: String, kind: String, name: String, create_sql: String) -> ()
///
/// create_sql 为前端编辑后的完整 CREATE 语句；校验失败（首词非 CREATE /
/// OR REPLACE、对象名含反引号）返回错误，保存语义为「替换旧对象」。
#[tauri::command]
#[specta::specta]
pub async fn mysql_object_save(
    conn_id: String,
    kind: String,
    name: String,
    create_sql: String,
) -> Result<(), AppError> {
    let kind = services::mysql_objects::parse_kind(&kind)?;
    services::mysql_objects::object_save(&conn_id, kind, &name, &create_sql).await
}

/// `mysql_object_drop` (conn_id: String, kind: String, name: String) -> ()
#[tauri::command]
#[specta::specta]
pub async fn mysql_object_drop(conn_id: String, kind: String, name: String) -> Result<(), AppError> {
    let kind = services::mysql_objects::parse_kind(&kind)?;
    services::mysql_objects::object_drop(&conn_id, kind, &name).await
}
