//! MySQL 数据库对象统一服务（契约第 5.2 节扩展）。
//!
//! 对齐 Navicat 的「数据库对象」四件套（kind 驱动分支）：
//! - `object_list`：information_schema 列出视图 / 函数 / 存储过程 / 触发器 / 事件
//! - `object_ddl`：SHOW CREATE 取完整 CREATE 语句
//! - `object_save`：前端编辑 DDL 后保存（DROP 旧对象 + 执行新 CREATE）
//! - `object_drop`：DROP IF EXISTS 删除对象
//!
//! 连接池与事务连接复用 services::mysql 暴露的 `pool_of` / `take_tx_conn` /
//! `give_tx_conn`（池对象无法跨注册表共享，本模块不另建注册表）。

use mysql_async::prelude::*;
use mysql_async::{Conn, Value};

use crate::error::AppError;
use crate::models::mysql_objects::{MySqlObjectDdl, MySqlObjectInfo, MySqlObjectKind};

/// mysql_async::Error -> AppError 归入 General 变体（与 services/mysql 同款归一化）
fn mysql_err(e: mysql_async::Error) -> AppError {
    AppError::general(format!("MySQL 错误: {e}"))
}

/// 取连接：优先复用事务内独占连接，否则从池中取
/// 返回 `bool` 表示是否来自事务连接；事务连接用完需 `restore_tx_conn` 放回。
async fn take_conn(conn_id: &str) -> Result<(Conn, bool), AppError> {
    if let Some(conn) = crate::services::mysql::take_tx_conn(conn_id) {
        return Ok((conn, true));
    }
    let pool = crate::services::mysql::pool_of(conn_id)?;
    let conn = pool.get_conn().await.map_err(mysql_err)?;
    Ok((conn, false))
}

/// 归还事务连接（事务仍在进行中，重新放入独占表）
fn restore_tx_conn(conn_id: &str, conn: Conn) {
    crate::services::mysql::give_tx_conn(conn_id, conn);
}

/// 命令层字符串 -> MySqlObjectKind（变体小写）
pub fn parse_kind(kind: &str) -> Result<MySqlObjectKind, AppError> {
    match kind {
        "view" => Ok(MySqlObjectKind::View),
        "function" => Ok(MySqlObjectKind::Function),
        "procedure" => Ok(MySqlObjectKind::Procedure),
        "trigger" => Ok(MySqlObjectKind::Trigger),
        "event" => Ok(MySqlObjectKind::Event),
        other => Err(AppError::general(format!(
            "未知对象类别: {other}（支持 view/function/procedure/trigger/event）"
        ))),
    }
}

/// 类别 -> SQL 关键字（SHOW CREATE / DROP 语句拼接用，枚举驱动无注入风险）
fn keyword(kind: MySqlObjectKind) -> &'static str {
    match kind {
        MySqlObjectKind::View => "VIEW",
        MySqlObjectKind::Function => "FUNCTION",
        MySqlObjectKind::Procedure => "PROCEDURE",
        MySqlObjectKind::Trigger => "TRIGGER",
        MySqlObjectKind::Event => "EVENT",
    }
}

/// 标识符加反引号包裹（调用前已校验非空、无反引号）
fn quote_ident(name: &str) -> Result<String, AppError> {
    if !is_valid_identifier(name) {
        return Err(AppError::general(format!("对象名非法: {name}")));
    }
    Ok(format!("`{name}`"))
}

/// 标识符基础校验：非空且不含反引号（反引号为 MySQL 标识符转义符，直接拒绝）
fn is_valid_identifier(s: &str) -> bool {
    !s.is_empty() && !s.contains('`')
}

/// 词边界前缀匹配：`lowered_sql` 以关键字 `kw` 开头
/// （关键字后必须紧跟非标识符字符，避免 "order" 误命中 "or" 等）
fn starts_with_kw(lowered_sql: &str, kw: &str) -> bool {
    lowered_sql
        .strip_prefix(kw)
        .map(|rest| {
            rest.chars()
                .next()
                .map(|c| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// mysql_object_list：按类别列出当前库下的数据库对象
///
/// information_schema 单查询（schema 走占位符，参照 services/mysql list_tables
/// 的 SELECT DATABASE() 兜底）；仅视图回传 TABLE_COMMENT，其余对象注释留空。
pub async fn object_list(
    conn_id: &str,
    kind: MySqlObjectKind,
) -> Result<Vec<MySqlObjectInfo>, AppError> {
    let (mut conn, from_tx) = take_conn(conn_id).await?;
    let outcome = do_list(&mut conn, kind).await;
    if from_tx {
        restore_tx_conn(conn_id, conn);
    }
    outcome
}

/// 列表主体（拆出以便错误后仍归还事务连接）
async fn do_list(
    conn: &mut Conn,
    kind: MySqlObjectKind,
) -> Result<Vec<MySqlObjectInfo>, AppError> {
    // 未显式指定 schema 时取当前默认数据库（SELECT DATABASE() 兜底）。
    // 无默认库（schema 为空）时不报错，返回空清单，用户选库后自动刷新
    let Some(schema) = conn
        .query_first::<Option<String>, _>("SELECT DATABASE()")
        .await
        .map_err(mysql_err)?
        .flatten()
    else {
        return Ok(Vec::new());
    };

    // 各类别的清单 SQL：第二列为注释占位（VIEWS 表无 TABLE_COMMENT 列，视图注释统一空串）
    let sql = match kind {
        MySqlObjectKind::View => {
            "SELECT TABLE_NAME, '' \
             FROM information_schema.VIEWS \
             WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME"
        }
        MySqlObjectKind::Function => {
            "SELECT ROUTINE_NAME, '' \
             FROM information_schema.ROUTINES \
             WHERE ROUTINE_SCHEMA = ? AND ROUTINE_TYPE = 'FUNCTION' ORDER BY ROUTINE_NAME"
        }
        MySqlObjectKind::Procedure => {
            "SELECT ROUTINE_NAME, '' \
             FROM information_schema.ROUTINES \
             WHERE ROUTINE_SCHEMA = ? AND ROUTINE_TYPE = 'PROCEDURE' ORDER BY ROUTINE_NAME"
        }
        MySqlObjectKind::Trigger => {
            "SELECT TRIGGER_NAME, '' \
             FROM information_schema.TRIGGERS \
             WHERE TRIGGER_SCHEMA = ? ORDER BY TRIGGER_NAME"
        }
        MySqlObjectKind::Event => {
            "SELECT EVENT_NAME, '' \
             FROM information_schema.EVENTS \
             WHERE EVENT_SCHEMA = ? ORDER BY EVENT_NAME"
        }
    };

    let mut result = conn
        .exec_iter(sql, (Value::Bytes(schema.into_bytes()),))
        .await
        .map_err(mysql_err)?;

    let kind_str = kind.as_str();
    let mut objects = Vec::new();
    while let Some(mut row) = result.next().await.map_err(mysql_err)? {
        objects.push(MySqlObjectInfo {
            name: row
                .take::<Option<String>, _>(0)
                .flatten()
                .unwrap_or_default(),
            kind: kind_str.to_string(),
            comment: row
                .take::<Option<String>, _>(1)
                .flatten()
                .unwrap_or_default(),
        });
    }
    result.drop_result().await.map_err(mysql_err)?;
    Ok(objects)
}

/// mysql_object_ddl：取对象的完整 CREATE 语句（SHOW CREATE 结果）
///
/// SHOW CREATE 语句的对象名走反引号拼接（先校验防注入）。注意各对象类型的
/// DDL 所在列不同（见 `is_ddl_column`），统一按列名定位而非固定第二列——
/// 函数 / 过程 / 触发器的第二列实为 sql_mode 等元信息。
pub async fn object_ddl(
    conn_id: &str,
    kind: MySqlObjectKind,
    name: &str,
) -> Result<MySqlObjectDdl, AppError> {
    let quoted = quote_ident(name)?;
    let stmt = format!("SHOW CREATE {} {quoted}", keyword(kind));

    let (mut conn, from_tx) = take_conn(conn_id).await?;
    let outcome = do_ddl(&mut conn, kind, name, &stmt).await;
    if from_tx {
        restore_tx_conn(conn_id, conn);
    }
    outcome
}

/// DDL 主体（拆出以便错误后仍归还事务连接）
async fn do_ddl(
    conn: &mut Conn,
    kind: MySqlObjectKind,
    name: &str,
    stmt: &str,
) -> Result<MySqlObjectDdl, AppError> {
    // SHOW CREATE 无占位符，直接文本协议查询
    let mut result = conn.query_iter(stmt).await.map_err(mysql_err)?;

    // 列名先转字符串（columns() 借用 result，行遍历前取出）
    let cols: Vec<String> = result
        .columns()
        .map(|cols| cols.iter().map(|c| c.name_str().into_owned()).collect())
        .unwrap_or_default();

    let mut sql_text: Option<String> = None;
    if let Some(mut row) = result.next().await.map_err(mysql_err)? {
        let idx = cols.iter().position(|c| is_ddl_column(c));
        if let Some(i) = idx {
            sql_text = row.take::<Option<String>, _>(i).flatten();
        }
    }
    result.drop_result().await.map_err(mysql_err)?;

    match sql_text {
        Some(sql) => Ok(MySqlObjectDdl {
            name: name.to_string(),
            kind: kind.as_str().to_string(),
            sql,
        }),
        None => Err(AppError::general(format!(
            "无法解析 SHOW CREATE 结果（对象 {name} 不存在或结果列为空）"
        ))),
    }
}

/// SHOW CREATE 结果中 DDL 列的名称匹配：
/// 视图为 "Create View"，函数 / 过程为 "Create Function" / "Create Procedure"，
/// 触发器为 "SQL Original Statement"（MySQL 5.7+），事件为 "Create Event"
fn is_ddl_column(col_name: &str) -> bool {
    let lowered = col_name.to_ascii_lowercase();
    matches!(
        lowered.as_str(),
        "create view"
            | "create function"
            | "create procedure"
            | "create trigger"
            | "create event"
            | "sql original statement"
    )
}

/// mysql_object_save：前端编辑 DDL 后保存——先 DROP 旧对象再执行新 CREATE
///
/// 校验：对象名合法（非空、拒绝反引号）；create_sql 非空且首词必须为
/// CREATE 或 OR REPLACE（词边界匹配，防注入：不允许执行任意语句）。
/// 视图免 DROP：create_sql 已为 CREATE OR REPLACE VIEW 时直接替换；
/// 普通 CREATE VIEW 先 DROP IF EXISTS 避免重名报错。
/// 其余类别统一 DROP IF EXISTS + 执行 create_sql（逐条执行，无多语句）。
pub async fn object_save(
    conn_id: &str,
    kind: MySqlObjectKind,
    name: &str,
    create_sql: &str,
) -> Result<(), AppError> {
    if create_sql.trim().is_empty() {
        return Err(AppError::general("DDL 语句为空"));
    }
    let quoted = quote_ident(name)?;
    let lowered = create_sql.trim().to_ascii_lowercase();
    if !starts_with_kw(&lowered, "create") && !starts_with_kw(&lowered, "or replace") {
        return Err(AppError::general("DDL 语句必须以 CREATE 或 OR REPLACE 开头"));
    }

    let (mut conn, from_tx) = take_conn(conn_id).await?;
    let outcome = do_save(&mut conn, kind, &lowered, &quoted, create_sql).await;
    if from_tx {
        restore_tx_conn(conn_id, conn);
    }
    outcome
}

/// 取对象既有 DDL（DROP 前备份用；对象不存在或读取失败返回 None，不阻塞保存）
async fn fetch_old_ddl(conn: &mut Conn, kind: MySqlObjectKind, quoted: &str) -> Option<String> {
    let stmt = format!("SHOW CREATE {} {quoted}", keyword(kind));
    let mut result = conn.query_iter(stmt.as_str()).await.ok()?;
    let cols: Vec<String> = result
        .columns()
        .map(|cols| cols.iter().map(|c| c.name_str().into_owned()).collect())
        .unwrap_or_default();
    let mut sql_text = None;
    if let Some(mut row) = result.next().await.ok()? {
        let idx = cols.iter().position(|c| is_ddl_column(c));
        if let Some(i) = idx {
            sql_text = row.take::<Option<String>, _>(i).flatten();
        }
    }
    let _ = result.drop_result().await;
    sql_text
}

/// 保存主体（拆出以便错误后仍归还事务连接）
async fn do_save(
    conn: &mut Conn,
    kind: MySqlObjectKind,
    lowered: &str,
    quoted: &str,
    create_sql: &str,
) -> Result<(), AppError> {
    // 视图且用户 DDL 已为 CREATE OR REPLACE：服务器端原子替换，免 DROP，
    // 失败不破坏原视图
    if kind == MySqlObjectKind::View && lowered.starts_with("or replace") {
        let result = conn.query_iter(create_sql).await.map_err(mysql_err)?;
        result.drop_result().await.map_err(mysql_err)?;
        return Ok(());
    }

    // 其余对象无 CREATE OR REPLACE 语法，只能 DROP + CREATE。
    // MySQL DDL 隐式提交（无事务可回滚）：若先 DROP 后 CREATE 失败，旧对象将丢失。
    // 因此 DROP 前先备份旧定义（SHOW CREATE），CREATE 失败时自动恢复原对象——
    // 尽力保证「要么新对象生效，要么原对象保留」。
    let old_ddl = fetch_old_ddl(conn, kind, quoted).await;
    let drop_sql = match kind {
        MySqlObjectKind::View => format!("DROP VIEW IF EXISTS {quoted}"),
        MySqlObjectKind::Function => format!("DROP FUNCTION IF EXISTS {quoted}"),
        MySqlObjectKind::Procedure => format!("DROP PROCEDURE IF EXISTS {quoted}"),
        MySqlObjectKind::Trigger => format!("DROP TRIGGER IF EXISTS {quoted}"),
        MySqlObjectKind::Event => format!("DROP EVENT IF EXISTS {quoted}"),
    };
    conn.query_drop(drop_sql).await.map_err(mysql_err)?;

    // 执行新 CREATE（文本协议单条语句；触发器体的 BEGIN...END 无需 DELIMITER，
    // DELIMITER 是 mysql CLI 客户端概念，整段 DDL 一次下发即可）
    match conn.query_iter(create_sql).await {
        Ok(result) => {
            result.drop_result().await.map_err(mysql_err)?;
            Ok(())
        }
        Err(e) => {
            // CREATE 失败：用旧定义尽力恢复原对象（保证「创建失败不丢原对象」）
            let restore_msg = match &old_ddl {
                Some(old) => match conn.query_drop(old.as_str()).await {
                    Ok(_) => "已用旧定义恢复原对象".to_string(),
                    Err(re) => format!("原对象恢复失败（旧定义执行报错: {re}）"),
                },
                None => "原对象此前不存在或旧定义读取失败，无法恢复".to_string(),
            };
            Err(AppError::general(format!(
                "保存对象失败: {e}。{restore_msg}。注意：MySQL DDL 隐式提交、不参与事务回滚，对象替换无法整体回滚。"
            )))
        }
    }
}

/// mysql_object_drop：按类别删除对象（DROP ... IF EXISTS）
pub async fn object_drop(
    conn_id: &str,
    kind: MySqlObjectKind,
    name: &str,
) -> Result<(), AppError> {
    let quoted = quote_ident(name)?;
    let sql = format!("DROP {} IF EXISTS {quoted}", keyword(kind));

    let (mut conn, from_tx) = take_conn(conn_id).await?;
    let outcome = conn.query_drop(&sql).await.map_err(mysql_err);
    if from_tx {
        restore_tx_conn(conn_id, conn);
    }
    outcome
}
