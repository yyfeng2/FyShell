//! MySQL 数据编辑服务（P2 阶段）：按主键行编辑、批量编辑、预览 -> 确认 -> 执行管道、
//! 原生插入。
//!
//! 防注入关键设计：
//! - 表名/列名/主键列来自前端，属 SQL 标识符（无法参数化），统一用反引号包裹
//!   并校验非空、不含反引号字符，杜绝标识符逃逸出上下文；
//! - 值统一按字符串字面量写入（单引号成对转义、反斜杠转义），
//!   NULL 由 is_null 显式表达，不依赖空串猜测。
//! - 原生插入（insert_rows）例外：值经 `?` 占位符由二进制协议参数化传输。
//!
//! 事务语义：批量执行时若连接已处于用户显式事务内（mysql_begin 开启），
//! 则加入该事务执行（不重复开、不代为提交/回滚）；否则开隐式事务包裹整批，
//! 任一条失败整体回滚，成功后统一提交。

use mysql_async::prelude::*;
use mysql_async::{Conn, Value};

use crate::error::AppError;
use crate::models::mysql_edit::{MySqlEditPreview, MySqlRowUpdate, MySqlRowUpdateBatch};
use crate::services::mysql::{give_tx_conn, is_dangerous_sql, pool_of, take_tx_conn};

/// mysql_async::Error -> AppError 归入 General 变体（与 services/mysql.rs 同策略）
fn mysql_err(e: mysql_async::Error) -> AppError {
    AppError::general(format!("MySQL 错误: {e}"))
}

/// 标识符校验并用反引号包裹：非空、不含反引号字符（防标识符逃逸）
fn quote_ident(name: &str) -> Result<String, AppError> {
    if name.trim().is_empty() {
        return Err(AppError::general("标识符（表名/列名）不能为空"));
    }
    if name.contains('`') {
        return Err(AppError::general(format!(
            "标识符含非法字符（反引号）: {name}"
        )));
    }
    Ok(format!("`{name}`"))
}

/// 值 -> SQL 字面量：is_null=true 或 value=None 时写 NULL，
/// 否则单引号包裹并转义（先转反斜杠再成对转义单引号，避免 `\'` 歧义）
fn quote_value(value: &Option<String>, is_null: bool) -> String {
    match (is_null, value) {
        (true, _) => "NULL".to_string(),
        (false, Some(v)) => format!("'{}'", v.replace('\\', "\\\\").replace('\'', "''")),
        // is_null=false 但未携带值：与 Option<String> 的 None=NULL 语义一致，按 NULL 写入
        (false, None) => "NULL".to_string(),
    }
}

/// WHERE 谓词：主键值为 NULL 时按 `pk IS NULL` 定位，否则 `pk = value`
fn pk_predicate(pk_column: &str, pk_value: &Option<String>) -> Result<String, AppError> {
    let col = quote_ident(pk_column)?;
    Ok(match pk_value {
        Some(_) => format!("{col} = {}", quote_value(pk_value, false)),
        None => format!("{col} IS NULL"),
    })
}

/// 由更新构造 UPDATE 语句与 COUNT 估算语句（同时完成标识符校验）：
/// 返回 (update_sql, count_sql)，二者共用同一 WHERE 谓词
fn build_update_sql(update: &MySqlRowUpdate) -> Result<(String, String), AppError> {
    let table = quote_ident(&update.table)?;
    let column = quote_ident(&update.column)?;
    let predicate = pk_predicate(&update.pk_column, &update.pk_value)?;
    let update_sql = format!("UPDATE {table} SET {column} = {} WHERE {predicate}", quote_value(&update.value, update.is_null));
    let count_sql = format!("SELECT COUNT(*) FROM {table} WHERE {predicate}");
    Ok((update_sql, count_sql))
}

/// mysql_edit_preview：生成 UPDATE 语句预览，并用 COUNT(*) 估算将被影响的行数
pub async fn preview_update(
    conn_id: &str,
    update: &MySqlRowUpdate,
) -> Result<MySqlEditPreview, AppError> {
    let (sql, count_sql) = build_update_sql(update)?;

    // COUNT(*) 估算受影响行数（优先复用事务连接，保持与执行时同一会话上下文）
    let tx_conn = take_tx_conn(conn_id);
    let from_tx = tx_conn.is_some();
    let mut conn = match tx_conn {
        Some(conn) => conn,
        None => pool_of(conn_id)?.get_conn().await.map_err(mysql_err)?,
    };
    let affected_estimate = conn
        .query_first::<u64, _>(&count_sql)
        .await
        .map_err(mysql_err)?
        .unwrap_or(0);
    if from_tx {
        give_tx_conn(conn_id, conn);
    }

    // danger 判定复用基础服务的词法检测（生成的 UPDATE 恒含 WHERE，
    // 仅 DROP/TRUNCATE/ALTER 等前缀会命中，此字段为管道完整性保留）
    let danger = is_dangerous_sql(&sql);
    Ok(MySqlEditPreview {
        sql,
        affected_estimate,
        danger,
        danger_reason: danger.then_some("命中危险 SQL 词法特征".to_string()),
    })
}

/// 单行更新：按主键定位写入新值，返回受影响行数
pub async fn update_row(conn_id: &str, update: &MySqlRowUpdate) -> Result<u64, AppError> {
    let (sql, _) = build_update_sql(update)?;

    // 优先复用事务内独占连接（不重复开事务），否则从池中取
    let tx_conn = take_tx_conn(conn_id);
    let from_tx = tx_conn.is_some();
    let mut conn = match tx_conn {
        Some(conn) => conn,
        None => pool_of(conn_id)?.get_conn().await.map_err(mysql_err)?,
    };
    let outcome = exec_write(&mut conn, &sql).await;
    if from_tx {
        give_tx_conn(conn_id, conn);
    }
    outcome
}

/// 批量更新：逐条执行，返回总受影响行数。
///
/// 事务包裹策略：
/// - 连接已处于用户事务内（mysql_begin）：加入该事务执行，不重复开事务，
///   也不代为提交/回滚（事务控制权在用户手中）；
/// - 无活跃事务：开隐式事务包裹整批，任一条失败则整体回滚（错误信息带上
///   已执行条数），全部成功后统一提交。
pub async fn update_rows(conn_id: &str, batch: &MySqlRowUpdateBatch) -> Result<u64, AppError> {
    if batch.updates.is_empty() {
        return Err(AppError::general("批量更新列表为空"));
    }
    // 预先构造全部 SQL（同时校验标识符），避免执行中途才发现非法输入；
    // 批次表名与条目表名不一致视为非法输入，防止错位写入
    let mut sqls = Vec::with_capacity(batch.updates.len());
    for update in &batch.updates {
        if !batch.table.is_empty()
            && !update.table.is_empty()
            && update.table != batch.table
        {
            return Err(AppError::general(format!(
                "批量更新中的条目表名 {} 与批次表名 {} 不一致",
                update.table, batch.table
            )));
        }
        let (sql, _) = build_update_sql(update)?;
        sqls.push(sql);
    }

    // 已有活跃事务：加入该事务执行，用完放回独占表
    if let Some(mut conn) = take_tx_conn(conn_id) {
        let outcome = exec_batch(&mut conn, &sqls).await;
        give_tx_conn(conn_id, conn);
        return outcome;
    }

    // 无活跃事务：开隐式事务包裹整批
    let mut conn = pool_of(conn_id)?.get_conn().await.map_err(mysql_err)?;
    if let Err(e) = conn.query_drop("START TRANSACTION").await {
        // START TRANSACTION 失败时事务未开，无需回滚，连接随 drop 归还池
        return Err(mysql_err(e));
    }
    match exec_batch(&mut conn, &sqls).await {
        Ok(total) => {
            conn.query_drop("COMMIT").await.map_err(mysql_err)?;
            Ok(total)
        }
        Err(e) => {
            // 任一条失败：回滚整批（隐式事务保证原子性）
            let _ = conn.query_drop("ROLLBACK").await;
            Err(e)
        }
    }
}

/// 批量逐条执行主体：任一条失败则中止，错误信息带上已执行条数
async fn exec_batch(conn: &mut Conn, sqls: &[String]) -> Result<u64, AppError> {
    let mut total = 0u64;
    for sql in sqls {
        match exec_write(conn, sql).await {
            Ok(affected) => total += affected,
            Err(e) => {
                return Err(AppError::general(format!(
                    "批量更新中止：已执行 {total} 条后失败 —— {e}"
                )))
            }
        }
    }
    Ok(total)
}

/// 单行删除：按主键定位删除，返回受影响行数（事务连接复用逻辑同 update_row）
pub async fn delete_row(
    conn_id: &str,
    table: &str,
    pk_column: &str,
    pk_value: Option<String>,
) -> Result<u64, AppError> {
    let table_quoted = quote_ident(table)?;
    let predicate = pk_predicate(pk_column, &pk_value)?;
    let sql = format!("DELETE FROM {table_quoted} WHERE {predicate}");

    let tx_conn = take_tx_conn(conn_id);
    let from_tx = tx_conn.is_some();
    let mut conn = match tx_conn {
        Some(conn) => conn,
        None => pool_of(conn_id)?.get_conn().await.map_err(mysql_err)?,
    };
    let outcome = exec_write(&mut conn, &sql).await;
    if from_tx {
        give_tx_conn(conn_id, conn);
    }
    outcome
}

/// 执行写语句并取受影响行数（文本协议，值已转义内联，无需占位符）
async fn exec_write(conn: &mut Conn, sql: &str) -> Result<u64, AppError> {
    let result = conn.query_iter(sql).await.map_err(mysql_err)?;
    let affected = result.affected_rows();
    // 写操作可能附带额外结果集（如多语句），统一消费掉
    result.drop_result().await.map_err(mysql_err)?;
    Ok(affected)
}

// ---------------------------------------------------------------------------
// 原生插入：逐行参数化 INSERT（预览 -> 确认 -> 执行管道）
// ---------------------------------------------------------------------------

/// mysql_insert_rows：逐行参数化 INSERT，返回插入行数。
///
/// 防注入：表名/列名统一反引号包裹并校验（见 quote_ident）；值经 `?` 占位符
/// 由 exec_iter 二进制协议传输（参照 mysql_io::import_csv，Value::Bytes 承接
/// 字符串，NULL 由 Option 显式表达），不做字面量拼接。
///
/// 事务包裹策略与批量更新一致：
/// - 连接已处于用户事务内（mysql_begin）：加入该事务执行，不重复开、不代为提交/回滚；
/// - 无活跃事务：开隐式事务包裹整批，任一条失败整体回滚，成功后统一提交。
///
/// columns 为空时按 `INSERT INTO {table} () VALUES ()` 逐行插入（全默认值，
/// 由数据库生成），对齐前端"插入 N 行空行"的语义。
pub async fn insert_rows(
    conn_id: &str,
    table: &str,
    columns: &[String],
    rows: &[Vec<Option<String>>],
) -> Result<u64, AppError> {
    if rows.is_empty() {
        return Err(AppError::general("插入行列表为空"));
    }
    let table_quoted = quote_ident(table)?;

    // 列数校验：每行值个数必须与列数一致（防错位写入）；columns 为空表示全默认值行
    if !columns.is_empty() {
        for (i, row) in rows.iter().enumerate() {
            if row.len() != columns.len() {
                return Err(AppError::general(format!(
                    "第 {} 行的值个数 {} 与列数 {} 不一致",
                    i + 1,
                    row.len(),
                    columns.len()
                )));
            }
        }
    }

    // 构造 INSERT 骨架（占位符与列数一致）
    let sql = match columns.is_empty() {
        true => format!("INSERT INTO {table_quoted} () VALUES ()"),
        false => {
            let cols = columns
                .iter()
                .map(|c| quote_ident(c))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ");
            let placeholders = vec!["?"; columns.len()].join(", ");
            format!("INSERT INTO {table_quoted} ({cols}) VALUES ({placeholders})")
        }
    };

    // 逐行参数：Some -> Bytes（字符串按参数传输），None -> NULL
    let param_rows: Vec<Vec<Value>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|v| match v {
                    Some(v) => Value::Bytes(v.as_bytes().to_vec()),
                    None => Value::NULL,
                })
                .collect()
        })
        .collect();

    // 已有活跃事务：加入该事务执行，用完放回独占表
    if let Some(mut conn) = take_tx_conn(conn_id) {
        let outcome = exec_inserts(&mut conn, &sql, &param_rows).await;
        give_tx_conn(conn_id, conn);
        return outcome;
    }

    // 无活跃事务：开隐式事务包裹整批
    let mut conn = pool_of(conn_id)?.get_conn().await.map_err(mysql_err)?;
    if let Err(e) = conn.query_drop("START TRANSACTION").await {
        // START TRANSACTION 失败时事务未开，无需回滚，连接随 drop 归还池
        return Err(mysql_err(e));
    }
    match exec_inserts(&mut conn, &sql, &param_rows).await {
        Ok(total) => {
            conn.query_drop("COMMIT").await.map_err(mysql_err)?;
            Ok(total)
        }
        Err(e) => {
            // 任一条失败：回滚整批（隐式事务保证原子性）
            let _ = conn.query_drop("ROLLBACK").await;
            Err(e)
        }
    }
}

/// 逐行参数化执行主体：任一条失败则中止，错误信息带上失败行号与已执行条数
async fn exec_inserts(conn: &mut Conn, sql: &str, rows: &[Vec<Value>]) -> Result<u64, AppError> {
    let stmt = conn.prep(sql).await.map_err(mysql_err)?;
    let mut total = 0u64;
    for (i, params) in rows.iter().enumerate() {
        let result = conn
            .exec_iter(&stmt, params)
            .await
            .map_err(|e| AppError::general(format!("第 {} 行插入失败 —— {e}", i + 1)))?;
        let affected = result.affected_rows();
        // 消费剩余结果集（保持与 exec_write 一致的清理语义）
        result.drop_result().await.map_err(mysql_err)?;
        total += affected;
    }
    Ok(total)
}
