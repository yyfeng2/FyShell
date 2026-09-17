//! 传输任务数据模型（契约第 1 节）与 SFTP 文件条目。
//!
//! FileEntry 是 SFTP 双栏 / 传输层的基础数据模型，随本模块定义，
//! 引用方式：`crate::models::transfer::FileEntry`。

use serde::{Deserialize, Serialize};

/// 传输方向：上传（本地 → 远端）/ 下载（远端 → 本地）
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
pub enum TransferKind {
    Upload,
    Download,
}

/// 传输状态：排队 → 运行 → 完成 / 失败 / 已取消
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
pub enum TransferStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// 传输任务（TS 侧同名同构）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct TransferTask {
    pub id: String,
    pub session_id: String,
    pub kind: TransferKind,
    pub local_path: String,
    pub remote_path: String,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub status: TransferStatus,
    pub error: Option<String>,
}

/// SFTP 文件条目（契约第 1 节）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// Unix 秒
    pub modified_at: i64,
    /// 八进制权限串（如 "644"，本地 Windows 目录返回空串）
    pub permissions: String,
}
