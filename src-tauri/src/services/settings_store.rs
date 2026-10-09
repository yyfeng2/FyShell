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
    // 多实例并发硬化：WAL 允许读写并发（多连接同库），busy_timeout 让写冲突等待
    // 而非立即 SQLITE_BUSY。默认单实例无并发，仅显式开启「允许多个客户端实例」时受益。
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "busy_timeout", 5_000)?;
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

// ---------------- 会话级 SSH 选项（sshopt_session_{session_id}_{key} 前缀） ----------------

/// 会话级键前缀：`sshopt_session_{session_id}_`（复用 settings 表，无 schema 迁移）
fn session_prefix(session_id: &str) -> String {
    format!("sshopt_session_{session_id}_")
}

/// 读取会话级全部覆盖项（返回裸 key -> value；未覆盖的键不在结果中，由前端回退全局值）
pub fn session_list(session_id: &str) -> Result<HashMap<String, String>, AppError> {
    let conn = lock();
    // LIKE 通配符转义 + 显式匹配会话 id 后的分隔下划线，避免短 id 前缀误命中其他会话
    let escaped = session_id
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("sshopt_session_{escaped}\\_%");
    let mut stmt = conn.prepare("SELECT key, value FROM settings WHERE key LIKE ?1 ESCAPE '\\'")?;
    let rows = stmt.query_map([pattern], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let prefix_len = session_prefix(session_id).len();
    let mut map = HashMap::new();
    for item in rows {
        let (k, v) = item?;
        if k.len() > prefix_len {
            map.insert(k[prefix_len..].to_string(), v);
        }
    }
    Ok(map)
}

/// 写入会话级覆盖项（幂等覆盖，key 为裸 key，如 bell_style）
pub fn session_set(session_id: &str, key: &str, value: &str) -> Result<(), AppError> {
    set(&format!("{}{}", session_prefix(session_id), key), value)
}

/// 删除会话级覆盖项（恢复继承全局值；键不存在时同样返回 Ok）
pub fn session_delete(session_id: &str, key: &str) -> Result<(), AppError> {
    let conn = lock();
    conn.execute(
        "DELETE FROM settings WHERE key = ?1",
        [format!("{}{}", session_prefix(session_id), key)],
    )?;
    Ok(())
}

// ---------------- 全部用户数据清理（恢复到首次运行状态） ----------------

/// 全部用户数据表：各服务模块在 setup 时统一建表（config_store/vault 共用 meta 表）。
/// 清空即删除导航树/保存的连接/查询历史/备份档案/快捷命令/键位映射/SFTP 收藏/
/// 隧道/认证配置/应用设置/主密码保险库，恢复到首次运行状态。
const USER_DATA_TABLES: &[&str] = &[
    "folders",
    "sessions",
    "meta",
    "settings",
    "key_mappings",
    "backup_profiles",
    "backup_runs",
    "query_history",
    "saved_queries",
    "quick_folders",
    "quick_commands",
    "sftp_favorites",
    "tunnels",
    "auth_profiles",
];

/// 清空全部用户数据表（不可恢复）。
///
/// 运行中的内存状态（连接池、vault DEK 等）不受影响，应用重启后回到首次运行状态。
pub fn clear_all_user_data() -> Result<(), AppError> {
    let conn = lock();
    for table in USER_DATA_TABLES {
        // 逐表清理：个别表缺失（服务建表条件变化）时跳过，其余表照常清空
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |r| r.get(0),
        )?;
        if exists {
            conn.execute(&format!("DELETE FROM {table}"), [])?;
        }
    }
    Ok(())
}
