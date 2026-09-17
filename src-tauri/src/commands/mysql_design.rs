//! MySQL 表设计器命令（契约第 5.2 节扩展，P2 阶段）：薄层转发到 services/mysql_design。
//!
//! 命令名对齐 P0 基础命令的 `mysql_` 前缀约定；
//! save 返回生成的 DDL 文本，前端可用于实时 DDL 预览与二次确认。

use crate::error::AppError;
use crate::models::mysql_design::{MySqlDesignChange, MySqlTableDesign};
use crate::services;

/// `mysql_table_design_get` (conn_id: String, table: String) -> MySqlTableDesign
#[tauri::command]
pub async fn mysql_table_design_get(
    conn_id: String,
    table: String,
) -> Result<MySqlTableDesign, AppError> {
    services::mysql_design::get_design(&conn_id, &table).await
}

/// `mysql_table_design_save` (conn_id: String, change: MySqlDesignChange) -> String 生成的 DDL 文本
///
/// 先经 validate_change 基础校验，再逐条执行 DDL；返回的 DDL 文本供前端展示预览。
#[tauri::command]
pub async fn mysql_table_design_save(
    conn_id: String,
    change: MySqlDesignChange,
) -> Result<String, AppError> {
    services::mysql_design::validate_change(&change)?;
    services::mysql_design::apply_change(&conn_id, &change).await
}
