//! MySQL 备份/还原数据模型（对齐 Navicat 备份功能）。
//!
//! TS 侧同名同构，引用方式：`crate::models::mysql_backup::*`。
//! 字段一律 snake_case（契约红线，禁 rename_all="camelCase"）。

use serde::{Deserialize, Serialize};

/// 备份档案（自动运行任务的持久化配置）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MySqlBackupProfile {
    /// 档案 id（TEXT uuid，新建由服务层生成）
    pub id: String,
    /// 档案名称（展示用）
    pub name: String,
    /// 关联的 MySQL 连接标识（host 描述，展示用；实际执行用 conn_id 连接池）
    pub conn_host: String,
    /// 参与备份的表名列表（空 = 全库）
    pub tables: Vec<String>,
    /// true = 含 INSERT 数据，false = 仅结构
    pub include_data: bool,
    /// true = 附带 DROP TABLE IF EXISTS（对齐备份执行的 DROP 开关；
    /// 建表结构始终导出，该字段控制的是 DROP 语句）
    pub include_create: bool,
    /// 创建时间（ISO 8601 字符串）
    pub created_at: String,
}

/// 备份运行历史单条记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MySqlBackupRun {
    /// 运行记录 id（SQLite 自增）
    pub id: i64,
    /// 关联的备份档案 id（TEXT uuid；None = 未关联档案的立即运行）
    pub profile_id: Option<String>,
    /// 备份产物文件路径
    pub file_path: String,
    /// 导出/写入的数据行数
    pub rows_total: u64,
    /// 耗时（毫秒；本轮未接入真实调度，由执行方尽力记录，无记录时为 0）
    pub duration_ms: u64,
    /// 创建时间（ISO 8601 字符串）
    pub created_at: String,
    /// true = 备份成功，false = 失败
    pub success: bool,
}
