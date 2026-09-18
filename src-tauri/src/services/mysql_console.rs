//! SQL 控制台服务（P2 阶段：查询历史 + 执行计划 + 多结果集 + 已保存查询）。
//!
//! - 查询历史：SQLite（同 fyshell.db，独立 Connection，模块级
//!   `OnceLock<Mutex<Connection>>` + `init(app_data_dir)` 注入，参照 tunnel.rs 模式，
//!   不触碰 AppState / lib.rs）
//! - 已保存查询：同库 saved_queries 表（命名保存，与自动记录的历史区分）
//! - 执行计划：经 `services::mysql::pool_of` 复用现有连接池，标准 `EXPLAIN`
//!   表格 + `EXPLAIN FORMAT=TREE` 树形文本；`EXPLAIN ANALYZE` 会真实执行语句，
//!   仅对 SELECT 开头的语句提供（防止借此执行写语句）
//! - 多结果集：连接池未开 multi_statements，因此用拆分后的语句逐条执行，
//!   SELECT 复用 mysql::query 分页，其余语句走 mysql::execute

use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

use mysql_async::prelude::*;
use mysql_async::{Conn, Value};
use rusqlite::{params, Connection};

use crate::error::AppError;
use crate::models::mysql_console::{MySqlExplainResult, MySqlQueryHistoryItem, MySqlSavedQueryItem};

// ---------------------------------------------------------------------------
// 查询历史：SQLite 持久化
// ---------------------------------------------------------------------------

/// SQLite 连接注册表：与 config_store / tunnel 同用 fyshell.db，但持独立
/// Connection，避免内部锁互相竞争。路径只能在应用启动后获取，
/// 故用模块级 OnceLock + `init(app_data_dir)` 注入（参照 tunnel.rs 模式）。
static CONN: OnceLock<Mutex<Connection>> = OnceLock::new();

/// 历史记录中 SQL 的最大字符数（防止超大 SQL / 批量脚本撑爆数据库）
const SQL_MAX_LEN: usize = 10_000;

/// 锁住 SQLite 连接（init 未调用视为编程错误）
fn lock_conn() -> MutexGuard<'static, Connection> {
    CONN.get()
        .expect("SQL 控制台数据库未初始化（init 未调用）")
        .lock()
        .expect("SQL 控制台数据库连接锁中毒")
}

/// 初始化：打开同 fyshell.db（独立 Connection）并完成建表迁移。
/// 由应用启动流程（集成层）调用一次；重复调用幂等。
pub fn init(app_data_dir: &Path) -> Result<(), AppError> {
    // 建库时自动创建目录
    std::fs::create_dir_all(app_data_dir)?;
    let conn = Connection::open(app_data_dir.join("fyshell.db"))?;
    let cell = CONN.get_or_init(|| Mutex::new(conn));
    let guard = cell.lock().expect("SQL 控制台数据库连接锁中毒");
    guard.execute_batch(
        "BEGIN;
         CREATE TABLE IF NOT EXISTS query_history (
             id          INTEGER PRIMARY KEY AUTOINCREMENT,
             sql         TEXT NOT NULL,
             conn_host   TEXT,
             duration_ms INTEGER,
             created_at  TEXT DEFAULT (datetime('now','localtime'))
         );
         CREATE INDEX IF NOT EXISTS idx_query_history_conn ON query_history (conn_host);
         CREATE TABLE IF NOT EXISTS saved_queries (
             id          INTEGER PRIMARY KEY AUTOINCREMENT,
             name        TEXT NOT NULL UNIQUE,
             conn_id     TEXT,
             sql         TEXT NOT NULL,
             created_at  TEXT DEFAULT (datetime('now','localtime'))
         );
         COMMIT;",
    )?;
    Ok(())
}

/// 行 -> MySqlQueryHistoryItem 的映射（conn_host 列承载连接标识）
fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<MySqlQueryHistoryItem> {
    Ok(MySqlQueryHistoryItem {
        id: row.get("id")?,
        conn_id: row.get("conn_host")?,
        sql: row.get("sql")?,
        created_at: row.get("created_at")?,
        duration_ms: row.get("duration_ms")?,
        // 表结构无 success 列：当前版本 history_add 仅在执行成功后调用，恒为 true
        success: true,
    })
}

/// 插入一条查询历史记录；超长 SQL 截断到 SQL_MAX_LEN（按字符截断，不切坏多字节字符）
pub fn history_add(sql: &str, conn_host: &str, duration_ms: u64) -> Result<(), AppError> {
    let truncated: String = sql.chars().take(SQL_MAX_LEN).collect();
    let conn = lock_conn();
    conn.execute(
        "INSERT INTO query_history (sql, conn_host, duration_ms) VALUES (?1, ?2, ?3)",
        params![truncated, conn_host, duration_ms as i64],
    )?;
    Ok(())
}

/// 最近 N 条历史记录（limit 为 0 时取默认 100，按 id 倒序）
pub fn history_list(limit: u32) -> Result<Vec<MySqlQueryHistoryItem>, AppError> {
    let limit: i64 = if limit == 0 { 100 } else { limit.min(1000) as i64 };
    let conn = lock_conn();
    let mut stmt = conn.prepare(
        "SELECT id, sql, conn_host, duration_ms, created_at
         FROM query_history
         ORDER BY id DESC
         LIMIT ?1",
    )?;
    let items: Vec<MySqlQueryHistoryItem> = stmt
        .query_map([limit], row_to_item)?
        .collect::<Result<_, _>>()?;
    Ok(items)
}

/// LIKE 模糊匹配历史（keyword 含 % / _ 时按 SQLite LIKE 语义生效，仅影响匹配范围，无注入风险）
pub fn history_search(keyword: &str, limit: u32) -> Result<Vec<MySqlQueryHistoryItem>, AppError> {
    let limit: i64 = if limit == 0 { 100 } else { limit.min(1000) as i64 };
    let pattern = format!("%{keyword}%");
    let conn = lock_conn();
    let mut stmt = conn.prepare(
        "SELECT id, sql, conn_host, duration_ms, created_at
         FROM query_history
         WHERE sql LIKE ?1
         ORDER BY id DESC
         LIMIT ?2",
    )?;
    let items: Vec<MySqlQueryHistoryItem> = stmt
        .query_map(params![pattern, limit], row_to_item)?
        .collect::<Result<_, _>>()?;
    Ok(items)
}

/// 清空全部查询历史
pub fn history_clear() -> Result<(), AppError> {
    let conn = lock_conn();
    conn.execute("DELETE FROM query_history", [])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 已保存查询（命名保存，区别于自动记录的查询历史）
// ---------------------------------------------------------------------------

/// 查询名称的最大字符数
const NAME_MAX_LEN: usize = 100;

/// 名称校验：去首尾空白、非空、限长（返回 trim 后的名称）
fn validate_name(name: &str) -> Result<&str, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::general("查询名称不能为空"));
    }
    if trimmed.chars().count() > NAME_MAX_LEN {
        return Err(AppError::general(format!(
            "查询名称过长（最多 {NAME_MAX_LEN} 字符）"
        )));
    }
    Ok(trimmed)
}

/// 行 -> MySqlSavedQueryItem 的映射
fn row_to_saved_query(row: &rusqlite::Row<'_>) -> rusqlite::Result<MySqlSavedQueryItem> {
    Ok(MySqlSavedQueryItem {
        id: row.get("id")?,
        name: row.get("name")?,
        conn_id: row.get("conn_id")?,
        sql: row.get("sql")?,
        created_at: row.get("created_at")?,
    })
}

/// 列出已保存查询（按名称排序，便于查找）
pub fn saved_query_list() -> Result<Vec<MySqlSavedQueryItem>, AppError> {
    let conn = lock_conn();
    let mut stmt = conn.prepare(
        "SELECT id, name, conn_id, sql, created_at
         FROM saved_queries
         ORDER BY name ASC",
    )?;
    let items: Vec<MySqlSavedQueryItem> = stmt.query_map([], row_to_saved_query)?.collect::<Result<_, _>>()?;
    Ok(items)
}

/// 保存查询：新增或覆盖（同名 UPSERT，保留原 id 与创建时间）。
///
/// 同名且 `overwrite=false` 时返回带提示错误，前端覆盖确认后带
/// `overwrite=true` 重新调用（与 mysql_update_row 的 confirmed 流程一致）；
/// 返回 true 表示覆盖了同名记录，false 表示新增。
pub fn saved_query_save(
    name: &str,
    conn_id: Option<&str>,
    sql: &str,
    overwrite: bool,
) -> Result<bool, AppError> {
    let name = validate_name(name)?;
    let truncated: String = sql.chars().take(SQL_MAX_LEN).collect();
    let conn = lock_conn();
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM saved_queries WHERE name = ?1",
        params![name],
        |r| r.get(0),
    )?;
    if exists > 0 && !overwrite {
        return Err(AppError::general(format!(
            "同名查询已存在：{name}，请确认后以 overwrite=true 重新保存"
        )));
    }
    conn.execute(
        "INSERT INTO saved_queries (name, conn_id, sql) VALUES (?1, ?2, ?3)
         ON CONFLICT(name) DO UPDATE SET conn_id = excluded.conn_id, sql = excluded.sql",
        params![name, conn_id, truncated],
    )?;
    Ok(exists > 0)
}

/// 重命名已保存查询（按 id 定位；新名称与其它条目冲突时报错）
pub fn saved_query_rename(id: i64, new_name: &str) -> Result<(), AppError> {
    let new_name = validate_name(new_name)?;
    let conn = lock_conn();
    // 同名冲突检查（排除自身）
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM saved_queries WHERE name = ?1 AND id != ?2",
        params![new_name, id],
        |r| r.get(0),
    )?;
    if exists > 0 {
        return Err(AppError::general(format!("名称已存在：{new_name}")));
    }
    let affected = conn.execute(
        "UPDATE saved_queries SET name = ?1 WHERE id = ?2",
        params![new_name, id],
    )?;
    if affected == 0 {
        return Err(AppError::general(format!("未找到 id 为 {id} 的已保存查询")));
    }
    Ok(())
}

/// 删除已保存查询（按 id 定位；id 不存在视为错误）
pub fn saved_query_delete(id: i64) -> Result<(), AppError> {
    let conn = lock_conn();
    let affected = conn.execute("DELETE FROM saved_queries WHERE id = ?1", params![id])?;
    if affected == 0 {
        return Err(AppError::general(format!("未找到 id 为 {id} 的已保存查询")));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 执行计划
// ---------------------------------------------------------------------------

/// mysql_err：mysql_async::Error -> AppError 归入 General 变体
/// （与 services::mysql::mysql_err 一致，便于本模块独立映射）
fn mysql_err(e: mysql_async::Error) -> AppError {
    AppError::general(format!("MySQL 错误: {e}"))
}

/// mysql_explain：执行计划。
///
/// - analyze=true：`EXPLAIN ANALYZE`（真实执行语句，仅允许 SELECT 开头），
///   MySQL 8 输出本身即单列树形文本，返回 format="tree"、tree 字段填充；
/// - analyze=false：标准 `EXPLAIN` 表格（列名 + 行数据），返回 format="rows"，
///   并附带尽力执行的 `EXPLAIN FORMAT=TREE` 文本树（失败不影响表格结果，
///   前端可在表格视图之外直接展示树形视图，省一次往返）。
pub async fn explain(
    conn_id: &str,
    sql: &str,
    analyze: bool,
) -> Result<MySqlExplainResult, AppError> {
    if sql.trim().is_empty() {
        return Err(AppError::general("SQL 语句为空"));
    }
    // ANALYZE 会真实执行语句：检查首词必须是 select，防止用 ANALYZE 执行写语句
    if analyze && !starts_with_keyword(sql, "select") {
        return Err(AppError::general(
            "EXPLAIN ANALYZE 仅支持 SELECT 开头的语句（ANALYZE 会真实执行语句，防止误执行写操作）",
        ));
    }

    // 复用现有连接池。事务内独占连接无法获取（pool_of 仅暴露池逻辑）：
    // EXPLAIN 看不到未提交事务的数据，属可接受的边界情况
    let pool = crate::services::mysql::pool_of(conn_id)?;
    let mut conn = pool.get_conn().await.map_err(mysql_err)?;

    if analyze {
        // EXPLAIN ANALYZE：结果为单行单列的树形文本，取第一行文本返回
        let q = format!("EXPLAIN ANALYZE {sql}");
        let mut result = conn
            .query_iter(q.as_str())
            .await
            .map_err(mysql_err)?;
        let mut tree = None;
        while let Some(mut row) = result.next().await.map_err(mysql_err)? {
            // take::<Option<Value>> 承接 NULL，再统一转字符串（与 do_query 一致）
            if tree.is_none() {
                tree = row
                    .take::<Option<Value>, _>(0)
                    .flatten()
                    .and_then(value_to_string);
            }
        }
        result.drop_result().await.map_err(mysql_err)?;
        return Ok(MySqlExplainResult {
            format: "tree".into(),
            columns: Vec::new(),
            rows: Vec::new(),
            tree,
        });
    }

    // 标准 EXPLAIN：表格列名 + 行数据（文本协议，避免占位符与准备语句参数错位）
    let q = format!("EXPLAIN {sql}");
    let mut result = conn
        .query_iter(q.as_str())
        .await
        .map_err(mysql_err)?;

    let columns: Vec<String> = result
        .columns()
        .map(|cols| cols.iter().map(|c| c.name_str().into_owned()).collect())
        .unwrap_or_default();

    let mut rows: Vec<Vec<Option<String>>> = Vec::new();
    while let Some(mut row) = result.next().await.map_err(mysql_err)? {
        let mut out = Vec::with_capacity(columns.len());
        for i in 0..columns.len() {
            let value = row.take::<Option<Value>, _>(i).flatten();
            out.push(value.and_then(value_to_string));
        }
        rows.push(out);
    }
    result.drop_result().await.map_err(mysql_err)?;

    // FORMAT=TREE 文本树：尽力填充（语句不支持 FORMAT=TREE 时静默忽略）
    let tree = fetch_tree_text(&mut conn, sql).await;

    Ok(MySqlExplainResult {
        format: "rows".into(),
        columns,
        rows,
        tree,
    })
}

/// 执行 `EXPLAIN FORMAT=TREE` 取单列文本树（单行单列）；不支持时返回 None
async fn fetch_tree_text(conn: &mut Conn, sql: &str) -> Option<String> {
    let q = format!("EXPLAIN FORMAT=TREE {sql}");
    let mut result = conn.query_iter(q.as_str()).await.ok()?;
    let mut tree = None;
    while let Some(mut row) = result.next().await.ok()? {
        if tree.is_none() {
            tree = row
                .take::<Option<Value>, _>(0)
                .flatten()
                .and_then(value_to_string);
        }
    }
    result.drop_result().await.ok()?;
    tree
}

/// 检查 SQL 首词（大小写不敏感）是否等于给定关键字（词边界匹配）
fn starts_with_keyword(sql: &str, keyword: &str) -> bool {
    sql.trim_start()
        .to_ascii_lowercase()
        .strip_prefix(keyword)
        .map(|rest| {
            rest.chars()
                .next()
                .map(|c| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(true)
        })
        .unwrap_or(false)
}

/// 值 -> 契约的 Option<String>：NULL -> None，其余统一转字符串
/// （与 services::mysql::value_to_string 一致；EXPLAIN 输出以字符串与数字为主，
/// 无日期/时间类型，故此处省略 Date/Time 分支）
fn value_to_string(value: Value) -> Option<String> {
    match value {
        Value::NULL => None,
        Value::Bytes(b) => Some(String::from_utf8_lossy(&b).into_owned()),
        Value::Int(i) => Some(i.to_string()),
        Value::UInt(u) => Some(u.to_string()),
        Value::Float(f) => Some(f.to_string()),
        Value::Double(d) => Some(d.to_string()),
        _ => None,
    }
}
