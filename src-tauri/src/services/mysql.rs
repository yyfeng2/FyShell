//! mysql_async 封装（契约第 5.1 / 5.2 节 MySQL 基础）。
//!
//! 连接池用模块级注册表管理（不触碰 AppState / lib.rs）：
//! `OnceLock<Mutex<HashMap<String, Pool>>>` + `pub fn init()` 模式。
//!
//! mysql_async 0.34 API 细节已按本地 crates 源码（docs.rs）核实：
//! - `Pool::new(OptsBuilder)` / `pool.get_conn().await? -> Conn` / `pool.disconnect()`（消费 self）
//! - `Conn::query_iter`（文本协议）/ `exec_iter`（二进制协议，支持 `?` 占位符）
//! - `QueryResult::next().await -> Option<Row>`、`affected_rows() -> u64`、`columns() -> Option<Arc<[Column]>>`
//! - `Row::take::<T, _>(idx) -> Option<T>`（NULL 由 `Option<T>` 承接）

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use mysql_async::prelude::*;
use mysql_async::{Conn, Pool, PoolConstraints, PoolOpts, Value};
use uuid::Uuid;

use crate::error::AppError;
use crate::models::mysql::{MySqlConnection, MySqlQueryResult, MySqlTableInfo};

/// 连接池注册表：connection_id -> Pool
static POOLS: OnceLock<Mutex<HashMap<String, Pool>>> = OnceLock::new();
/// 活跃事务连接注册表：connection_id -> 事务期间独占的 Conn
///
/// mysql_async 池连接归还时默认会 reset_connection（隐式回滚打开的事务），
/// 因此事务期间的连接必须单独持有，不能随池归还。
static TX_CONNS: OnceLock<Mutex<HashMap<String, Conn>>> = OnceLock::new();
/// 连接配置注册表：connection_id -> 原始连接配置
///
/// 数据库切换（services/mysql_db switch_database）重建连接池时需要原始
/// host/port/username/password，connect 成功时在此存档。
static CONFIGS: OnceLock<Mutex<HashMap<String, MySqlConnection>>> = OnceLock::new();

/// 初始化模块级注册表（lib.rs 启动时调用一次；重复调用无害）
pub fn init() {
    let _ = POOLS.set(Mutex::new(HashMap::new()));
    let _ = TX_CONNS.set(Mutex::new(HashMap::new()));
    let _ = CONFIGS.set(Mutex::new(HashMap::new()));
}

/// mysql_async::Error -> AppError 归入 General 变体（未修改 error.rs，避免动他人名下文件）
fn mysql_err(e: mysql_async::Error) -> AppError {
    AppError::general(format!("MySQL 错误: {e}"))
}

/// 连接池：确保注册表已初始化（init 未调用时惰性兜底）
fn pools() -> &'static Mutex<HashMap<String, Pool>> {
    POOLS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 事务连接注册表：同上惰性兜底
fn tx_conns() -> &'static Mutex<HashMap<String, Conn>> {
    TX_CONNS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 连接配置注册表：同上惰性兜底
fn configs() -> &'static Mutex<HashMap<String, MySqlConnection>> {
    CONFIGS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 从注册表取连接池（不移出）
fn get_pool(conn_id: &str) -> Result<Pool, AppError> {
    pools()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(conn_id)
        .cloned()
        .ok_or_else(|| AppError::general(format!("MySQL 连接不存在: {conn_id}")))
}

/// 取连接：优先复用事务内独占连接（remove 出表），否则从池中取
/// 返回 `bool` 表示是否来自事务连接；事务连接用完需 `restore_tx_conn` 放回。
async fn take_conn(conn_id: &str) -> Result<(Conn, bool), AppError> {
    if let Some(conn) = tx_conns()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(conn_id)
    {
        return Ok((conn, true));
    }
    let pool = get_pool(conn_id)?;
    let conn = pool.get_conn().await.map_err(mysql_err)?;
    Ok((conn, false))
}

/// 归还事务连接（事务仍在进行中，重新放入独占表）
fn restore_tx_conn(conn_id: &str, conn: Conn) {
    tx_conns()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(conn_id.to_string(), conn);
}

/// 暴露连接池获取逻辑（供数据编辑等服务模块复用，不移出池）
pub fn pool_of(conn_id: &str) -> Result<Pool, AppError> {
    get_pool(conn_id)
}

/// 取事务内独占连接（remove 出表；无活跃事务返回 None）
pub fn take_tx_conn(conn_id: &str) -> Option<Conn> {
    tx_conns()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(conn_id)
}

/// 放回事务内独占连接（事务仍在进行中，转发 restore_tx_conn）
pub fn give_tx_conn(conn_id: &str, conn: Conn) {
    restore_tx_conn(conn_id, conn)
}

/// mysql_connect：OptsBuilder 建立连接池，生成 uuid connection_id 存模块级注册表。
/// 首次取连接以尽早暴露认证/网络错误。
pub async fn connect(cfg: &MySqlConnection) -> Result<String, AppError> {
    let mut builder = mysql_async::OptsBuilder::default()
        .ip_or_hostname(cfg.host.clone())
        .tcp_port(cfg.port)
        .user(Some(cfg.username.clone()))
        .pass(Some(cfg.password.clone()))
        .db_name(cfg.schema.clone());
    // 桌面工具单连接低频使用，池约束收窄（min=1 保证常驻一条、max=10 封顶）
    if let Some(constraints) = PoolConstraints::new(1, 10) {
        builder = builder.pool_opts(PoolOpts::default().with_constraints(constraints));
    }
    let pool = Pool::new(builder);

    // 建立一次真实连接验证可达性与凭据；disconnect 关闭该连接（不归还池）
    let probe = pool.get_conn().await.map_err(mysql_err)?;
    probe.disconnect().await.map_err(mysql_err)?;

    let id = Uuid::new_v4().to_string();
    pools()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(id.clone(), pool);
    // 连接配置存档：数据库切换重建连接池时取用（services/mysql_db）
    configs()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(id.clone(), cfg.clone());
    Ok(id)
}

/// 取连接的原始配置（clone；数据库切换重建连接池用）
///
/// 连接断开后旧 conn_id 的配置仍留在注册表中（与 POOLS 的 remove 时机不同步），
/// 属可接受的少量残留——connection_id 为 uuid 不会复用，不影响正确性。
pub fn config_of(conn_id: &str) -> Result<MySqlConnection, AppError> {
    configs()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(conn_id)
        .cloned()
        .ok_or_else(|| AppError::general(format!("MySQL 连接不存在: {conn_id}")))
}

/// mysql_disconnect：从注册表取出并关闭（含事务连接清理）
pub async fn disconnect(conn_id: &str) -> Result<(), AppError> {
    let tx_conn = tx_conns()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(conn_id);
    // 尽力回滚未提交的事务再断开
    if let Some(mut conn) = tx_conn {
        let _ = conn.query_drop("ROLLBACK").await;
        let _ = conn.disconnect().await;
    }
    let pool = pools()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(conn_id)
        .ok_or_else(|| AppError::general(format!("MySQL 连接不存在: {conn_id}")))?;
    pool.disconnect().await.map_err(mysql_err)
}

/// mysql_list_tables：information_schema 单查询取所有表的行数/注释/引擎
pub async fn list_tables(conn_id: &str, db: Option<&str>) -> Result<Vec<MySqlTableInfo>, AppError> {
    let (mut conn, from_tx) = take_conn(conn_id).await?;
    let outcome = do_list_tables(&mut conn, db).await;
    if from_tx {
        restore_tx_conn(conn_id, conn);
    }
    outcome
}

/// 表列表主体（拆出以便错误早退时仍归还事务连接：take 出的独占连接必须放回，
/// 否则用户显式事务失去句柄，后续 commit/rollback 报「该连接无活跃事务」）
///
/// db 为 None 时取当前默认数据库（SELECT DATABASE()；事务连接与池连接同源，
/// 当前库上下文一致，统一用此查询兜底）；Some(db) 时以 `db` 限定
/// （information_schema TABLE_SCHEMA 过滤，供工具对话框列出任意库的表清单）。
/// 无默认库（schema 为空）时不报错：information_schema 查询无法限定库，
/// 返回空表清单，用户在库切换下拉中选库后自动刷新
async fn do_list_tables(conn: &mut Conn, db: Option<&str>) -> Result<Vec<MySqlTableInfo>, AppError> {
    let Some(schema) = (match db {
        Some(db) => Some(db.to_string()),
        None => conn
            .query_first::<Option<String>, _>("SELECT DATABASE()")
            .await
            .map_err(mysql_err)?
            .flatten(),
    }) else {
        return Ok(Vec::new());
    };

    // 单条查询拿全：TABLE_ROWS 为 InnoDB 预估行数（契约允许 information_schema 方案）
    let info_sql = "SELECT TABLE_NAME, COALESCE(TABLE_ROWS, 0), COALESCE(TABLE_COMMENT, ''), \
                    COALESCE(ENGINE, '') \
                    FROM information_schema.TABLES \
                    WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'BASE TABLE' \
                    ORDER BY TABLE_NAME";
    let mut result = conn
        .exec_iter(info_sql, (Value::Bytes(schema.clone().into_bytes()),))
        .await
        .map_err(mysql_err)?;

    let mut tables = Vec::new();
    while let Some(mut row) = result.next().await.map_err(mysql_err)? {
        tables.push(MySqlTableInfo {
            name: row
                .take::<Option<String>, _>(0)
                .flatten()
                .unwrap_or_default(),
            rows: row.take::<Option<u64>, _>(1).flatten().unwrap_or(0),
            comment: row
                .take::<Option<String>, _>(2)
                .flatten()
                .unwrap_or_default(),
            engine: row
                .take::<Option<String>, _>(3)
                .flatten()
                .unwrap_or_default(),
        });
    }
    result.drop_result().await.map_err(mysql_err)?;
    // 精确行数：TABLE_ROWS 是估算值（performance_schema 表显示固定估计数如 131072，
    // 与实际不符），逐表 COUNT(*) 用 UNION ALL 合并单条查询得出。
    // COUNT 带 schema 限定（`schema`.`table`）：db 参数指定库时连接当前库可能是别的库
    if !tables.is_empty() {
        let count_sql = tables
            .iter()
            .map(|t| {
                format!(
                    "SELECT COUNT(*) FROM `{}`.`{}`",
                    schema,
                    t.name.replace('`', "``")
                )
            })
            .collect::<Vec<_>>()
            .join(" UNION ALL ");
        let mut result = conn.query_iter(count_sql).await.map_err(mysql_err)?;
        let mut idx = 0usize;
        while let Some(mut row) = result.next().await.map_err(mysql_err)? {
            if idx >= tables.len() {
                break;
            }
            tables[idx].rows = row.take::<Option<u64>, _>(0).flatten().unwrap_or(0);
            idx += 1;
        }
        result.drop_result().await.map_err(mysql_err)?;
    }
    Ok(tables)
}

/// mysql_query：SELECT 查询——SQL 已含 LIMIT 时不重复包装，否则自动追加
/// `LIMIT ? OFFSET ?` 分页（占位符由 exec_iter 二进制协议填充，page/page_size 为
/// u32 无注入风险）；total 用 COUNT(*) 包装原查询得出。
pub async fn query(
    conn_id: &str,
    sql: &str,
    page: u32,
    page_size: u32,
) -> Result<MySqlQueryResult, AppError> {
    if sql.trim().is_empty() {
        return Err(AppError::general("SQL 语句为空"));
    }
    let page = page.max(1);
    let page_size = page_size.clamp(1, 1000);
    let offset = (page - 1) * page_size;

    let started = std::time::Instant::now();
    let (mut conn, from_tx) = take_conn(conn_id).await?;
    let outcome = do_query(&mut conn, sql, page, page_size, offset).await;
    if from_tx {
        restore_tx_conn(conn_id, conn);
    }
    // 查询历史：仅首页记录一次（翻页 page>1 不重复写）；记录失败不影响查询结果
    if outcome.is_ok() && page == 1 {
        let _ = crate::services::mysql_console::history_add(
            sql,
            conn_id,
            started.elapsed().as_millis() as u64,
        );
    }
    outcome
}

/// 查询主体（拆出以便错误后仍归还事务连接）
async fn do_query(
    conn: &mut Conn,
    sql: &str,
    page: u32,
    page_size: u32,
    offset: u32,
) -> Result<MySqlQueryResult, AppError> {
    let has_limit = contains_limit_clause(sql);

    // total：COUNT(*) 包装原查询。
    // - 原查询无 LIMIT：统计全量，total 为真实总行数；
    //   原查询已含 LIMIT 时，计数与分页一致（LIMIT 内的总数），行为对齐 Navicat。
    // 用文本协议查询，避免占位符与准备语句参数错位。
    let count_sql = format!("SELECT COUNT(*) FROM ({sql}) AS _fyshell_count");
    let total = conn
        .query_first::<u64, _>(&count_sql)
        .await
        .map_err(mysql_err)?
        .unwrap_or(0);

    // 数据页：已含 LIMIT 用原 SQL，否则包装 LIMIT ? OFFSET ?
    let fetch_sql = if has_limit {
        sql.to_string()
    } else {
        format!("{sql} LIMIT ? OFFSET ?")
    };

    let mut result = if has_limit {
        conn.exec_iter(fetch_sql.as_str(), ())
            .await
            .map_err(mysql_err)?
    } else {
        conn.exec_iter(
            fetch_sql.as_str(),
            (Value::UInt(page_size as u64), Value::UInt(offset as u64)),
        )
        .await
        .map_err(mysql_err)?
    };

    let col_names: Vec<String> = result
        .columns()
        .map(|cols| cols.iter().map(|c| c.name_str().into_owned()).collect())
        .unwrap_or_default();

    let mut rows: Vec<Vec<Option<String>>> = Vec::new();
    while let Some(mut row) = result.next().await.map_err(mysql_err)? {
        let mut out = Vec::with_capacity(col_names.len());
        for i in 0..col_names.len() {
            // take::<Option<Value>> 承接 NULL（None），再统一转字符串
            let value = row.take::<Option<Value>, _>(i).flatten();
            out.push(value.and_then(value_to_string));
        }
        rows.push(out);
    }
    // 消费剩余结果集/清理语句（全量遍历后调用是安全的，参照 query<>/exec_first 实现）
    result.drop_result().await.map_err(mysql_err)?;

    Ok(MySqlQueryResult {
        columns: col_names,
        rows,
        total,
        page,
        page_size,
    })
}

/// mysql_execute：非 SELECT 写操作，返回受影响行数
pub async fn execute(conn_id: &str, sql: &str) -> Result<u64, AppError> {
    if sql.trim().is_empty() {
        return Err(AppError::general("SQL 语句为空"));
    }
    let started = std::time::Instant::now();
    let (mut conn, from_tx) = take_conn(conn_id).await?;
    let outcome = do_execute(&mut conn, sql).await;
    if from_tx {
        restore_tx_conn(conn_id, conn);
    }
    // 写操作成功后记录查询历史；记录失败不影响执行结果
    if outcome.is_ok() {
        let _ = crate::services::mysql_console::history_add(
            sql,
            conn_id,
            started.elapsed().as_millis() as u64,
        );
    }
    outcome
}

/// 执行写操作主体
async fn do_execute(conn: &mut Conn, sql: &str) -> Result<u64, AppError> {
    let result = conn.query_iter(sql).await.map_err(mysql_err)?;
    let affected = result.affected_rows();
    // 写操作可能附带额外结果集（如多语句），统一消费掉
    result.drop_result().await.map_err(mysql_err)?;
    Ok(affected)
}

/// 命令列界面专用：任意语句直接执行，不做 COUNT(*) 包装、不分页。
/// 行语句（SELECT/SHOW/DESC/EXPLAIN 等）返回全部列与行（最多 1000 行截断），
/// 非行语句（INSERT/UPDATE/DDL）total 为受影响行数；前端按 columns 是否为空区分展示。
pub async fn cli_exec(conn_id: &str, sql: &str) -> Result<MySqlQueryResult, AppError> {
    if sql.trim().is_empty() {
        return Err(AppError::general("SQL 语句为空"));
    }
    let started = std::time::Instant::now();
    let (mut conn, from_tx) = take_conn(conn_id).await?;
    let outcome = do_cli_exec(&mut conn, sql).await;
    if from_tx {
        restore_tx_conn(conn_id, conn);
    }
    // 命令列执行同样记录查询历史；记录失败不影响执行结果
    if outcome.is_ok() {
        let _ = crate::services::mysql_console::history_add(
            sql,
            conn_id,
            started.elapsed().as_millis() as u64,
        );
    }
    outcome
}

/// 命令列执行主体（拆出以便错误后仍归还事务连接，与 do_execute 同型）
async fn do_cli_exec(conn: &mut Conn, sql: &str) -> Result<MySqlQueryResult, AppError> {
    const MAX_ROWS: usize = 1000;
    let mut result = conn.query_iter(sql).await.map_err(mysql_err)?;
    let col_names: Vec<String> = result
        .columns()
        .map(|cols| cols.iter().map(|c| c.name_str().into_owned()).collect())
        .unwrap_or_default();
    let mut rows: Vec<Vec<Option<String>>> = Vec::new();
    while let Some(mut row) = result.next().await.map_err(mysql_err)? {
        let mut out = Vec::with_capacity(col_names.len());
        for i in 0..col_names.len() {
            // take::<Option<Value>> 承接 NULL（None），再统一转字符串
            let value = row.take::<Option<Value>, _>(i).flatten();
            out.push(value.and_then(value_to_string));
        }
        rows.push(out);
        if rows.len() >= MAX_ROWS {
            break;
        }
    }
    let affected = result.affected_rows();
    // 提前 break 时残余结果集由 drop_result 统一消费（与 do_query 同理）
    result.drop_result().await.map_err(mysql_err)?;
    Ok(MySqlQueryResult {
        columns: col_names,
        rows,
        total: affected,
        page: 1,
        page_size: MAX_ROWS as u32,
    })
}

/// mysql_begin：在连接池的会话内开启事务（独占连接，存事务注册表）
pub async fn begin(conn_id: &str) -> Result<(), AppError> {
    {
        let tx = tx_conns().lock().unwrap_or_else(|p| p.into_inner());
        if tx.contains_key(conn_id) {
            return Err(AppError::general("该连接已有活跃事务"));
        }
    }
    let pool = get_pool(conn_id)?;
    let mut conn = pool.get_conn().await.map_err(mysql_err)?;
    conn.query_drop("START TRANSACTION").await.map_err(mysql_err)?;
    tx_conns()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(conn_id.to_string(), conn);
    Ok(())
}

/// mysql_commit：提交事务并归还独占连接到池
pub async fn commit(conn_id: &str) -> Result<(), AppError> {
    let mut conn = tx_conns()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(conn_id)
        .ok_or_else(|| AppError::general(format!("该连接无活跃事务: {conn_id}")))?;
    conn.query_drop("COMMIT").await.map_err(mysql_err)
    // conn drop 时自动归还连接池
}

/// mysql_rollback：回滚事务并归还独占连接到池
pub async fn rollback(conn_id: &str) -> Result<(), AppError> {
    let mut conn = tx_conns()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(conn_id)
        .ok_or_else(|| AppError::general(format!("该连接无活跃事务: {conn_id}")))?;
    conn.query_drop("ROLLBACK").await.map_err(mysql_err)
    // conn drop 时自动归还连接池
}

/// 危险 SQL 词法检测辅助（大小写不敏感的词边界匹配）：
/// DROP / TRUNCATE / ALTER 开头，或 DELETE / UPDATE 无 WHERE
pub fn is_dangerous_sql(sql: &str) -> bool {
    let lowered = sql.trim().to_ascii_lowercase();
    let starts_with = |kw: &str| {
        lowered
            .strip_prefix(kw)
            .map(|rest| {
                rest.chars()
                    .next()
                    .map(|c| !c.is_ascii_alphanumeric() && c != '_')
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    };
    if starts_with("drop") || starts_with("truncate") || starts_with("alter") {
        return true;
    }
    if starts_with("delete") || starts_with("update") {
        // 无 WHERE 判定：词边界找最后一个 where，其后无内容视为无 WHERE
        return find_word(&lowered, "where").is_none();
    }
    false
}

/// 词边界查找：返回关键字在原文中的起始下标（找不到返回 None）
fn find_word(haystack: &str, needle: &str) -> Option<usize> {
    let bytes = haystack.as_bytes();
    let n = needle.len();
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs = start + pos;
        let before_ok = abs == 0
            || !bytes[abs - 1].is_ascii_alphanumeric() && bytes[abs - 1] != b'_';
        let after = abs + n;
        let after_ok = after >= bytes.len()
            || !bytes[after].is_ascii_alphanumeric() && bytes[after] != b'_';
        if before_ok && after_ok {
            return Some(abs);
        }
        start = abs + 1;
    }
    None
}

/// 检测 SQL 是否已含 LIMIT 子句（词边界匹配最后一个 limit 关键字）
///
/// 简化说明：不解析 SQL 注释与字符串字面量，若 LIMIT 出现在字面量内会被误判为已含
/// LIMIT 而跳过自动分页——属可接受的边界情况，前端 SQL 编辑器通常不产生此类输入。
fn contains_limit_clause(sql: &str) -> bool {
    find_word(&sql.to_ascii_lowercase(), "limit").is_some()
}

/// 值 -> 契约的 Option<String>：NULL -> None，其余统一转字符串
fn value_to_string(value: Value) -> Option<String> {
    match value {
        Value::NULL => None,
        Value::Bytes(b) => Some(String::from_utf8_lossy(&b).into_owned()),
        Value::Int(i) => Some(i.to_string()),
        Value::UInt(u) => Some(u.to_string()),
        Value::Float(f) => Some(f.to_string()),
        Value::Double(d) => Some(d.to_string()),
        Value::Date(y, mo, d, h, mi, s, us) => {
            let micro = if us == 0 { String::new() } else { format!(".{us:06}") };
            Some(format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}{micro}"))
        }
        // MySQL TIME 可超 24 小时（如 "838:59:59"），天数折算进小时
        Value::Time(neg, days, h, m, s, us) => {
            let hours = days * 24 + h as u32;
            let sign = if neg { "-" } else { "" };
            let micro = if us == 0 { String::new() } else { format!(".{us:06}") };
            Some(format!("{sign}{hours:03}:{m:02}:{s:02}{micro}"))
        }
    }
}
