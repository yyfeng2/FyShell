//! MySQL 工具级功能命令：数据传输/数据生成/数据同步/结构同步（对齐 Navicat 工具菜单）。
//!
//! 薄层转发到 services/mysql_tools。源连接由前端传入 conn_id（当前活动连接），
//! 目标连接由前端预先通过 mysql_connect 建立并注册（target_conn_id），
//! 本层不做连接管理；同步类命令的 execute=false 为仅比对模式。

use crate::error::AppError;
use crate::models::mysql_tools::{
    MySqlDataSyncOptions, MySqlDataSyncOutcome, MySqlGenerateOptions, MySqlStructureSyncOptions,
    MySqlStructureSyncPlan, MySqlTransferOptions, MySqlTransferTableResult,
};
use crate::services;

/// `mysql_data_transfer` (source_conn_id: String, options: MySqlTransferOptions)
/// -> Vec<MySqlTransferTableResult>（逐表行数与跳过标记）
#[tauri::command]
#[specta::specta]
pub async fn mysql_data_transfer(
    source_conn_id: String,
    options: MySqlTransferOptions,
) -> Result<Vec<MySqlTransferTableResult>, AppError> {
    services::mysql_tools::data_transfer(&source_conn_id, &options).await
}

/// `mysql_data_generate` (conn_id: String, options: MySqlGenerateOptions) -> u64（插入行数）
#[tauri::command]
#[specta::specta]
pub async fn mysql_data_generate(
    conn_id: String,
    options: MySqlGenerateOptions,
) -> Result<u64, AppError> {
    services::mysql_tools::data_generate(&conn_id, &options).await
}

/// `mysql_data_sync` (source_conn_id: String, options: MySqlDataSyncOptions [, execute: bool])
/// -> MySqlDataSyncOutcome（execute=false 仅比对返回差异，true 按选项应用）
#[tauri::command]
#[specta::specta]
pub async fn mysql_data_sync(
    source_conn_id: String,
    options: MySqlDataSyncOptions,
    execute: Option<bool>,
) -> Result<MySqlDataSyncOutcome, AppError> {
    services::mysql_tools::data_sync(&source_conn_id, &options, execute.unwrap_or(false)).await
}

/// `mysql_structure_sync` (source_conn_id: String, options: MySqlStructureSyncOptions [, execute: bool])
/// -> MySqlStructureSyncPlan（execute=false 仅比对返回计划，true 在目标端执行）
#[tauri::command]
#[specta::specta]
pub async fn mysql_structure_sync(
    source_conn_id: String,
    options: MySqlStructureSyncOptions,
    execute: Option<bool>,
) -> Result<MySqlStructureSyncPlan, AppError> {
    services::mysql_tools::structure_sync(&source_conn_id, &options, execute.unwrap_or(false)).await
}
