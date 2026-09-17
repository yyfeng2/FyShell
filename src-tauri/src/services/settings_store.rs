//! 应用设置持久化：Rust 侧 SQLite settings 表（高功能设置对话框）
//!
//! - 数据库与 config_store 共用 `app_data_dir()/fyshell.db`，但持有独立 `Connection`
//!   （SQLite bundled 支持多连接同库读写，WAL/锁由 SQLite 内部仲裁）
//! - 模块级注册表模式：主会话在 lib.rs setup 中调用 `init(app_data_dir)`，
//!   命令层直接调用本模块的同步函数，不经过 AppState
//! - 建表迁移：settings(key TEXT PRIMARY KEY, value TEXT) 单表，
//!   值统一为字符串，由前端按 key 解析为对应类型

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use rusqlite::{params, Connection};

use crate::error::AppError;

/// 内部连接注册表：`OnceLock<Mutex<Connection>>`，应用启动时经 `init` 注入
static CONN: OnceLock<Mutex<Connection>> = OnceLock::new();

/// 初始化设置存储（目录不存在则自动创建，并完成建表迁移）
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
    CONN.get().expect("settings_store 未初始化：请在 lib.rs setup 中调用 init")
}

/// 锁住连接（Mutex 中毒视为不可恢复错误）
fn lock() -> std::sync::MutexGuard<'static, Connection> {
    registry().lock().expect("settings_store 内部连接锁中毒")
}

/// 建表迁移：settings 单表
fn init_schema(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        "BEGIN;
         CREATE TABLE IF NOT EXISTS settings (
             key   TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );
         COMMIT;",
    )?;
    Ok(())
}

/// 全量读取设置项（key -> value 映射；未写入过的键不在结果中，由前端回退默认值）
pub fn get_all() -> Result<HashMap<String, String>, AppError> {
    let conn = lock();
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut map = HashMap::new();
    for item in rows {
        let (k, v) = item?;
        map.insert(k, v);
    }
    Ok(map)
}

/// 按 key 读取单个设置项；不存在返回 Ok(None)
pub fn get(key: &str) -> Result<Option<String>, AppError> {
    let conn = lock();
    match conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
        row.get::<_, String>(0)
    }) {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// 写入单个设置项（按 key 幂等覆盖；value 统一序列化为字符串存储）
pub fn set(key: &str, value: &str) -> Result<(), AppError> {
    let conn = lock();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}
