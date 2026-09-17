//! 键位映射持久化：Rust 侧 SQLite
//!
//! - 数据库与 config_store 共用 `app_data_dir()/fyshell.db`，但持有独立 `Connection`
//!   （SQLite bundled 支持多连接同库读写，WAL/锁由 SQLite 内部仲裁）
//! - 模块级注册表模式：主会话在 lib.rs setup 中调用 `init(app_data_dir)`，
//!   命令层直接调用本模块的同步函数，不经过 AppState
//! - 建表迁移：key_mappings 一张表

use std::path::Path;
use std::sync::{Mutex, OnceLock};

use rusqlite::{params, Connection};

use crate::error::AppError;
use crate::models::key_mapping::KeyMapping;

/// 内部连接注册表：`OnceLock<Mutex<Connection>>`，应用启动时经 `init` 注入
static CONN: OnceLock<Mutex<Connection>> = OnceLock::new();

/// 初始化键位映射存储（数据库不存在则自动创建，并完成建表迁移）
///
/// 幂等：已初始化时直接返回 Ok。主会话在 lib.rs setup 中调用一次。
pub fn init(app_data_dir: &Path) -> Result<(), AppError> {
    if CONN.get().is_some() {
        return Ok(());
    }
    // 建库时自动创建目录
    std::fs::create_dir_all(app_data_dir)?;
    let conn = Connection::open(app_data_dir.join("fyshell.db"))?;
    init_schema(&conn)?;
    // set 失败（并发重复初始化）时丢弃新连接即可，保持幂等语义
    let _ = CONN.set(Mutex::new(conn));
    Ok(())
}

/// 取内部连接注册表（未初始化视为不可恢复的程序错误）
fn registry() -> &'static Mutex<Connection> {
    CONN.get().expect("key_mapping_store 未初始化：请在 lib.rs setup 中调用 init")
}

/// 锁住连接（Mutex 中毒视为不可恢复错误）
fn lock() -> std::sync::MutexGuard<'static, Connection> {
    registry().lock().expect("key_mapping_store 内部连接锁中毒")
}

/// 建表迁移：key_mappings 表
fn init_schema(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        "BEGIN;
         CREATE TABLE IF NOT EXISTS key_mappings (
             id          TEXT PRIMARY KEY,
             key_combo   TEXT NOT NULL,
             action_type TEXT NOT NULL,
             payload     TEXT NOT NULL
         );
         COMMIT;",
    )?;
    Ok(())
}

/// 键位映射列表（按 key_combo 排序）
pub fn list() -> Result<Vec<KeyMapping>, AppError> {
    let conn = lock();
    let mut stmt =
        conn.prepare_cached("SELECT id, key_combo, action_type, payload FROM key_mappings ORDER BY key_combo")?;
    let mappings = stmt
        .query_map([], |row| {
            Ok(KeyMapping {
                id: row.get(0)?,
                key_combo: row.get(1)?,
                action_type: row.get(2)?,
                payload: row.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(mappings)
}

/// 保存键位映射（按 id upsert；新映射的 id 由 commands 层生成后传入）
pub fn save(mapping: &KeyMapping) -> Result<(), AppError> {
    let conn = lock();
    conn.execute(
        "INSERT INTO key_mappings (id, key_combo, action_type, payload)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET
             key_combo = excluded.key_combo,
             action_type = excluded.action_type,
             payload = excluded.payload",
        params![mapping.id, mapping.key_combo, mapping.action_type, mapping.payload],
    )?;
    Ok(())
}

/// 删除单个键位映射
pub fn delete(id: &str) -> Result<(), AppError> {
    let conn = lock();
    conn.execute("DELETE FROM key_mappings WHERE id = ?1", [id])?;
    Ok(())
}
