//! SFTP 收藏路径持久化（SQLite，同 fyshell.db 独立 Connection）
//!
//! - 表结构自建 `sftp_favorites`（side + path 唯一），不触碰 config_store.rs；
//! - 模块级注册表 + `init(app_data_dir)` 注入（与 tunnel/monitor 服务同一模式），
//!   不触碰 AppState / lib.rs 命令注册之外的任何结构；
//! - 按 (side, path) 幂等：重复收藏返回既有记录，取消收藏按路径删除。

use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

use rusqlite::{params, Connection};

use crate::error::AppError;

/// SQLite 连接注册表：与 config_store 同用 fyshell.db，但持独立 Connection，
/// 避免与 ConfigStore 内部锁互相竞争。路径依赖 Tauri API（只能在应用启动后获取），
/// 故用模块级 OnceLock + init(app_data_dir) 注入。
static CONN: OnceLock<Mutex<Connection>> = OnceLock::new();

/// 收藏路径条目（side: local | remote，便于下拉按窗格侧过滤，避免跨侧误跳转）
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct SftpFavorite {
    /// uuid v4
    pub id: String,
    /// 路径所属侧：local | remote
    pub side: String,
    /// 收藏的目录绝对路径
    pub path: String,
    /// 收藏时间（Unix 秒）
    pub created_at: i64,
}

/// 锁住 SQLite 连接（init 未调用视为编程错误）
fn lock_conn() -> MutexGuard<'static, Connection> {
    CONN.get()
        .expect("sftp_store 数据库未初始化（init 未调用）")
        .lock()
        .expect("sftp_store 数据库连接锁中毒")
}

/// 初始化：打开同 fyshell.db（独立 Connection）并完成建表迁移。
/// 由应用启动流程（setup / 集成层）调用一次；重复调用幂等。
pub fn init(app_data_dir: &Path) -> Result<(), AppError> {
    std::fs::create_dir_all(app_data_dir)?;
    let conn = Connection::open(app_data_dir.join("fyshell.db"))?;
    let cell = CONN.get_or_init(|| Mutex::new(conn));
    let guard = cell.lock().expect("sftp_store 数据库连接锁中毒");
    guard.execute_batch(
        "BEGIN;
         CREATE TABLE IF NOT EXISTS sftp_favorites (
             id         TEXT PRIMARY KEY,
             side       TEXT NOT NULL,
             path       TEXT NOT NULL,
             created_at INTEGER NOT NULL
         );
         CREATE UNIQUE INDEX IF NOT EXISTS idx_sftp_favorites_side_path
             ON sftp_favorites (side, path);
         COMMIT;",
    )?;
    Ok(())
}

/// 收藏列表：按收藏时间倒序（新收藏在前）
pub fn list() -> Result<Vec<SftpFavorite>, AppError> {
    let conn = lock_conn();
    let mut stmt = conn.prepare(
        "SELECT id, side, path, created_at
         FROM sftp_favorites
         ORDER BY created_at DESC, id DESC",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(SftpFavorite {
                id: row.get("id")?,
                side: row.get("side")?,
                path: row.get("path")?,
                created_at: row.get("created_at")?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// 收藏路径：按 (side, path) 幂等，已收藏时返回既有记录（不重复入库）
pub fn add(side: &str, path: &str) -> Result<SftpFavorite, AppError> {
    if path.is_empty() {
        return Err(AppError::general("路径不能为空"));
    }
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let conn = lock_conn();
    conn.execute(
        "INSERT INTO sftp_favorites (id, side, path, created_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(side, path) DO NOTHING",
        params![uuid::Uuid::new_v4().to_string(), side, path, created_at],
    )?;
    drop(conn);
    // 幂等：upsert 后按 (side, path) 读回既有/新插入的记录
    find_by_path(side, path)?
        .ok_or_else(|| AppError::general("收藏写入失败"))
}

/// 取消收藏：按侧 + 路径删除（未收藏时幂等）
pub fn remove(side: &str, path: &str) -> Result<(), AppError> {
    let conn = lock_conn();
    conn.execute(
        "DELETE FROM sftp_favorites WHERE side = ?1 AND path = ?2",
        params![side, path],
    )?;
    Ok(())
}

/// 按侧 + 路径查询收藏；不存在返回 Ok(None)
fn find_by_path(side: &str, path: &str) -> Result<Option<SftpFavorite>, AppError> {
    let conn = lock_conn();
    let mut stmt = conn.prepare(
        "SELECT id, side, path, created_at
         FROM sftp_favorites WHERE side = ?1 AND path = ?2",
    )?;
    let mut rows = stmt.query_map(params![side, path], |row| {
        Ok(SftpFavorite {
            id: row.get("id")?,
            side: row.get("side")?,
            path: row.get("path")?,
            created_at: row.get("created_at")?,
        })
    })?;
    Ok(rows.next().transpose()?)
}
