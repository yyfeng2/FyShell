//! MySQL 工具级功能的数据模型（对齐 Navicat 的数据传输/数据生成/数据同步/结构同步）。
//!
//! 字段一律 snake_case（契约红线：禁用 rename_all="camelCase"）。
//!
//! 通用约定：源连接由前端传入 conn_id（当前活动连接），目标连接由前端预先通过
//! mysql_connect 建立并注册在连接注册表（target_conn_id）；同一连接可同时充当
//! 源与目标（同服务器跨库）。全部库/表名在 services 层校验并反引号包裹。

use serde::{Deserialize, Serialize};

// ---------------- 数据传输 ----------------

/// 数据传输请求（mysql_data_transfer）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlTransferOptions {
    /// 源库名（source_conn_id 连接，全部 SQL 以 `db`.`table` 限定）
    pub source_db: String,
    /// 目标连接 ID（前端 mysql_connect 预先建立的独立连接，可与源同连接）
    pub target_conn_id: String,
    /// 目标库名
    pub target_db: String,
    /// 要传输的表名列表（源库中的表，顺序即传输顺序）
    pub tables: Vec<String>,
    /// 传输结构（目标端建表）
    pub include_structure: bool,
    /// 传输数据（批量 INSERT）
    pub include_data: bool,
    /// 目标表已存在时 DROP 重建（false = 跳过该表结构，数据仍按 include_data 传输）
    pub recreate: bool,
}

/// 数据传输逐表结果（mysql_data_transfer 返回）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlTransferTableResult {
    pub table: String,
    /// 传输的行数（仅结构时为 0）
    pub rows: u64,
    /// 是否跳过（目标表已存在且未开启覆盖重建）
    pub skipped: bool,
}

// ---------------- 数据生成 ----------------

/// 数据生成单列规则
///
/// kind 取值：int / decimal / string / uuid / name / phone / email / datetime / fixed；
/// min/max 用于 int 与 decimal，length 用于 string，values 用于 fixed（候选池），
/// null_ratio 为 NULL 概率百分比（0-100，None = 不产生 NULL）。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlGenerateColumnRule {
    pub name: String,
    pub kind: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub length: Option<u32>,
    pub values: Vec<String>,
    pub null_ratio: Option<u32>,
}

/// 数据生成请求（mysql_data_generate）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlGenerateOptions {
    /// 目标表名（当前连接的当前库）
    pub table: String,
    /// 生成行数（1-100000）
    pub rows: u32,
    /// 生成前清空表（TRUNCATE）
    pub truncate: bool,
    /// 每列生成规则
    pub columns: Vec<MySqlGenerateColumnRule>,
}

// ---------------- 数据同步 ----------------

/// 数据同步请求（mysql_data_sync）
///
/// 按主键比对两表数据；主键结构不一致或缺失时报错。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlDataSyncOptions {
    /// 源表名（source_conn_id 连接的 source_db 库）
    pub source_db: String,
    pub source_table: String,
    /// 目标连接 ID + 库名 + 表名
    pub target_conn_id: String,
    pub target_db: String,
    pub target_table: String,
    /// 插入目标端缺失的行
    pub insert_missing: bool,
    /// 删除目标端多余的行
    pub delete_extra: bool,
    /// 更新两端都有但内容不一致的行（REPLACE INTO）
    pub update_diff: bool,
}

/// 数据同步比对结果（mysql_data_sync 返回）
///
/// execute=false 时仅有计数与样例；execute=true 为应用后的计数。
/// 样例为差异行的主键值（按主键列序），三组各限 20 条。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlDataSyncOutcome {
    /// 主键列名（按列序）
    pub key_columns: Vec<String>,
    /// 仅源端有（目标端缺失）
    pub only_source: u64,
    /// 仅目标端有（源端没有）
    pub only_target: u64,
    /// 两端都有但非主键列不一致
    pub changed: u64,
    pub sample_source: Vec<Vec<String>>,
    pub sample_target: Vec<Vec<String>>,
    pub sample_changed: Vec<Vec<String>>,
}

// ---------------- 结构同步 ----------------

/// 结构同步请求（mysql_structure_sync）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlStructureSyncOptions {
    /// 源库名（source_conn_id 连接）
    pub source_db: String,
    /// 目标连接 ID + 库名
    pub target_conn_id: String,
    pub target_db: String,
}

/// 结构同步逐表差异项
///
/// kind：create = 目标缺失该表（sql 为源端 SHOW CREATE TABLE 原文，表名已重限定）；
/// alter = 双方共有但列定义有差异（sql 为逐条 ALTER）；
/// same = 结构一致（sql 为空）。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlStructureSyncItem {
    pub table: String,
    pub kind: String,
    pub sql: String,
}

/// 结构同步计划（mysql_structure_sync 返回）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlStructureSyncPlan {
    pub items: Vec<MySqlStructureSyncItem>,
}
