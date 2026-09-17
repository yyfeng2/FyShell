//! MySQL 表设计器服务（契约第 5.2 节扩展，P2 阶段）。
//!
//! 职责三件套：
//! - `get_design`：information_schema 组装表设计快照（列 / 索引 / 外键 / 引擎 / 字符集）
//! - `apply_change`：从变更生成 ALTER / CREATE DDL 并逐条执行，返回 DDL 文本供前端预览
//! - `validate_change`：表名 / 字段名 / 字段类型等基础校验
//!
//! 连接池与事务连接复用 services::mysql 暴露的 `pool_of` / `take_tx_conn` /
//! `give_tx_conn`（池对象无法跨注册表共享，本模块不另建注册表）。

use mysql_async::prelude::*;
use mysql_async::Conn;

use crate::error::AppError;
use crate::models::mysql_design::{
    ACTION_ADD, ACTION_DROP, ACTION_MODIFY, MySqlColumnChange, MySqlDesignChange, MySqlForeignKeyInfo, MySqlColumnInfo, MySqlIndexInfo,
    MySqlTableDesign,
};

/// mysql_async::Error -> AppError 归入 General 变体（与 services/mysql 同款归一化）
fn mysql_err(e: mysql_async::Error) -> AppError {
    AppError::general(format!("MySQL 错误: {e}"))
}

/// 取连接：优先复用事务内独占连接，否则从池中取
/// 返回 `bool` 表示是否来自事务连接；事务连接用完需 `give_tx_conn` 放回。
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

/// mysql_table_design_get：组装指定表的设计快照
///
/// 列 / 索引 / 外键分别查 information_schema（表名走占位符，无注入风险）：
/// - 列：COLUMNS（COLUMN_KEY 等价 SHOW COLUMNS 的 Key 列）
/// - 索引：STATISTICS（按 INDEX_NAME + SEQ_IN_INDEX 分组）
/// - 外键：KEY_COLUMN_USAGE 关联 REFERENTIAL_CONSTRAINTS 取 ON DELETE / ON UPDATE
pub async fn get_design(conn_id: &str, table: &str) -> Result<MySqlTableDesign, AppError> {
    if !is_valid_identifier(table) {
        return Err(AppError::general(format!("表名非法: {table}")));
    }
    let (mut conn, from_tx) = take_conn(conn_id).await?;
    let outcome = do_get_design(&mut conn, table).await;
    if from_tx {
        restore_tx_conn(conn_id, conn);
    }
    outcome
}

/// 组装主体（拆出以便错误后仍归还事务连接）
async fn do_get_design(conn: &mut Conn, table: &str) -> Result<MySqlTableDesign, AppError> {
    // 表元信息：引擎 / 排序规则 / 注释，并经 COLLATIONS 反查字符集（COLLATION_NAME 唯一）
    let meta_sql = "SELECT COALESCE(t.ENGINE, ''), COALESCE(t.TABLE_COLLATION, ''), \
                    COALESCE(t.TABLE_COMMENT, ''), COALESCE(cs.CHARACTER_SET_NAME, '') \
                    FROM information_schema.TABLES t \
                    LEFT JOIN information_schema.COLLATIONS cs \
                           ON cs.COLLATION_NAME = t.TABLE_COLLATION \
                    WHERE t.TABLE_SCHEMA = DATABASE() AND t.TABLE_NAME = ?";
    let (engine, _collation, comment, charset): (String, String, String, String) = conn
        .exec_first(meta_sql, (table.to_string(),))
        .await
        .map_err(mysql_err)?
        .ok_or_else(|| AppError::general(format!("表不存在: {table}")))?;

    // 列定义（IS_NULLABLE 为 'YES'/'NO'；COLUMN_KEY 等价 SHOW COLUMNS 的 Key 列）
    let col_sql = "SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT, COLUMN_COMMENT, \
                   EXTRA, COLUMN_KEY, CHARACTER_SET_NAME, COLLATION_NAME \
                   FROM information_schema.COLUMNS \
                   WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? \
                   ORDER BY ORDINAL_POSITION";
    let mut result = conn
        .exec_iter(col_sql, (table.to_string(),))
        .await
        .map_err(mysql_err)?;

    let mut columns = Vec::new();
    while let Some(mut row) = result.next().await.map_err(mysql_err)? {
        columns.push(MySqlColumnInfo {
            name: row
                .take::<Option<String>, _>(0)
                .flatten()
                .unwrap_or_default(),
            data_type: row
                .take::<Option<String>, _>(1)
                .flatten()
                .unwrap_or_default(),
            is_nullable: row.take::<Option<String>, _>(2).flatten().unwrap_or_default() == "YES",
            default_value: row.take::<Option<String>, _>(3).flatten(),
            comment: row
                .take::<Option<String>, _>(4)
                .flatten()
                .unwrap_or_default(),
            extra: row
                .take::<Option<String>, _>(5)
                .flatten()
                .unwrap_or_default(),
            key_type: row.take::<Option<String>, _>(6).flatten(),
            character_set: row.take::<Option<String>, _>(7).flatten(),
            collation: row.take::<Option<String>, _>(8).flatten(),
        });
    }
    result.drop_result().await.map_err(mysql_err)?;

    // 索引（NON_UNIQUE=0 即唯一索引；INDEX_NAME='PRIMARY' 为主键）
    let idx_sql = "SELECT INDEX_NAME, NON_UNIQUE, COLUMN_NAME, INDEX_TYPE \
                   FROM information_schema.STATISTICS \
                   WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? \
                   ORDER BY INDEX_NAME, SEQ_IN_INDEX";
    let mut result = conn
        .exec_iter(idx_sql, (table.to_string(),))
        .await
        .map_err(mysql_err)?;

    let mut indexes: Vec<MySqlIndexInfo> = Vec::new();
    while let Some(mut row) = result.next().await.map_err(mysql_err)? {
        let name: String = row
            .take::<Option<String>, _>(0)
            .flatten()
            .unwrap_or_default();
        let non_unique: u8 = row.take::<Option<u8>, _>(1).flatten().unwrap_or(1);
        let column: String = row
            .take::<Option<String>, _>(2)
            .flatten()
            .unwrap_or_default();
        let index_type: String = row
            .take::<Option<String>, _>(3)
            .flatten()
            .unwrap_or_default();
        let is_primary = name == "PRIMARY";
        // 查询已按 INDEX_NAME + SEQ_IN_INDEX 分组排序：同名行归属当前索引，否则开启新索引
        if indexes.last().map(|i| i.name == name).unwrap_or(false) {
            if let Some(last) = indexes.last_mut() {
                last.columns.push(column);
            }
        } else {
            indexes.push(MySqlIndexInfo {
                name,
                columns: vec![column],
                is_unique: non_unique == 0,
                is_primary,
                index_type,
            });
        }
    }
    result.drop_result().await.map_err(mysql_err)?;

    // 外键（REFERENCED_TABLE_NAME 非空的行为外键；关联 REFERENTIAL_CONSTRAINTS 取规则）
    let fk_sql = "SELECT kcu.CONSTRAINT_NAME, kcu.COLUMN_NAME, kcu.REFERENCED_TABLE_NAME, \
                  kcu.REFERENCED_COLUMN_NAME, COALESCE(rc.DELETE_RULE, ''), COALESCE(rc.UPDATE_RULE, '') \
                  FROM information_schema.KEY_COLUMN_USAGE kcu \
                  JOIN information_schema.REFERENTIAL_CONSTRAINTS rc \
                    ON rc.CONSTRAINT_SCHEMA = kcu.CONSTRAINT_SCHEMA \
                   AND rc.TABLE_NAME = kcu.TABLE_NAME \
                   AND rc.CONSTRAINT_NAME = kcu.CONSTRAINT_NAME \
                  WHERE kcu.TABLE_SCHEMA = DATABASE() AND kcu.TABLE_NAME = ? \
                    AND kcu.REFERENCED_TABLE_NAME IS NOT NULL \
                  ORDER BY kcu.CONSTRAINT_NAME, kcu.ORDINAL_POSITION";
    let mut result = conn
        .exec_iter(fk_sql, (table.to_string(),))
        .await
        .map_err(mysql_err)?;

    let mut foreign_keys: Vec<MySqlForeignKeyInfo> = Vec::new();
    while let Some(mut row) = result.next().await.map_err(mysql_err)? {
        let name: String = row
            .take::<Option<String>, _>(0)
            .flatten()
            .unwrap_or_default();
        let column: String = row
            .take::<Option<String>, _>(1)
            .flatten()
            .unwrap_or_default();
        let ref_table: String = row
            .take::<Option<String>, _>(2)
            .flatten()
            .unwrap_or_default();
        let ref_column: String = row
            .take::<Option<String>, _>(3)
            .flatten()
            .unwrap_or_default();
        let on_delete: String = row
            .take::<Option<String>, _>(4)
            .flatten()
            .unwrap_or_default();
        let on_update: String = row
            .take::<Option<String>, _>(5)
            .flatten()
            .unwrap_or_default();
        // 查询已按 CONSTRAINT_NAME 分组排序：同名行归属当前外键，否则开启新外键
        if foreign_keys.last().map(|f| f.name == name).unwrap_or(false) {
            if let Some(last) = foreign_keys.last_mut() {
                last.columns.push(column);
            }
        } else {
            foreign_keys.push(MySqlForeignKeyInfo {
                name,
                columns: vec![column],
                ref_table,
                ref_columns: vec![ref_column],
                on_delete,
                on_update,
            });
        }
    }
    result.drop_result().await.map_err(mysql_err)?;

    Ok(MySqlTableDesign {
        table: table.to_string(),
        columns,
        indexes,
        foreign_keys,
        engine,
        charset,
        comment,
    })
}

/// mysql_table_design_save：从变更生成 DDL 并逐条执行，返回 DDL 文本（前端实时预览）
///
/// 多语句逐条执行（文本协议，DDL 无占位符）；调用前应先过 `validate_change`。
pub async fn apply_change(conn_id: &str, change: &MySqlDesignChange) -> Result<String, AppError> {
    let stmts = build_ddl(change)?;

    let (mut conn, from_tx) = take_conn(conn_id).await?;
    let outcome: Result<(), AppError> = async {
        for sql in &stmts {
            conn.query_drop(sql).await.map_err(mysql_err)?;
        }
        Ok(())
    }
    .await;
    if from_tx {
        restore_tx_conn(conn_id, conn);
    }
    outcome?;
    let mut ddl = stmts.join(";\n");
    ddl.push(';');
    Ok(ddl)
}

/// 变更 -> DDL 语句列表（is_new=true 生成 CREATE TABLE，否则逐条 ALTER TABLE）
fn build_ddl(change: &MySqlDesignChange) -> Result<Vec<String>, AppError> {
    let table = quote_ident(&change.table)?;

    // 新建表：单条 CREATE TABLE 内联全部列 / 索引 / 外键
    if change.is_new {
        let mut defs: Vec<String> = Vec::new();
        for col in &change.columns {
            defs.push(column_definition(col)?);
        }
        for idx in &change.indexes {
            defs.push(index_definition(idx)?);
        }
        for fk in &change.foreign_keys {
            defs.push(foreign_key_definition(fk)?);
        }
        let mut ddl = format!("CREATE TABLE {table} (\n  {}\n)", defs.join(",\n  "));
        // 引擎 / 字符集缺省时沿用服务器默认值；注释需转义单引号
        if let Some(engine) = non_blank(&change.engine) {
            ddl.push_str(&format!(" ENGINE={engine}"));
        }
        if let Some(charset) = non_blank(&change.charset) {
            ddl.push_str(&format!(" DEFAULT CHARSET={charset}"));
        }
        if let Some(comment) = non_blank(&change.comment) {
            ddl.push_str(&format!(" COMMENT '{}'", comment.replace('\'', "''")));
        }
        return Ok(vec![ddl]);
    }

    let mut stmts = Vec::new();
    // 字段级变更
    for col in &change.columns {
        match col.action.as_str() {
            "add" => stmts.push(format!(
                "ALTER TABLE {table} ADD COLUMN {}",
                column_definition(col)?
            )),
            "modify" => stmts.push(format!(
                "ALTER TABLE {table} MODIFY COLUMN {}",
                column_definition(col)?
            )),
            "drop" => stmts.push(format!(
                "ALTER TABLE {table} DROP COLUMN {}",
                quote_ident(&col.name)?
            )),
            other => {
                return Err(AppError::general(format!(
                    "未知的字段变更类型: {other}（应为 add/modify/drop）"
                )))
            }
        }
    }
    // 新增索引：ADD PRIMARY KEY / ADD [UNIQUE] INDEX
    for idx in &change.indexes {
        if idx.is_primary {
            let cols = join_quoted(&idx.columns)?;
            stmts.push(format!("ALTER TABLE {table} ADD PRIMARY KEY ({cols})"));
        } else {
            let kind = if idx.is_unique { "UNIQUE INDEX" } else { "INDEX" };
            let name = quote_ident(&idx.name)?;
            let cols = join_quoted(&idx.columns)?;
            stmts.push(format!("ALTER TABLE {table} ADD {kind} {name} ({cols})"));
        }
    }
    // 删除索引（主键用 DROP PRIMARY KEY）
    for name in &change.dropped_indexes {
        if name.eq_ignore_ascii_case("PRIMARY") {
            stmts.push(format!("ALTER TABLE {table} DROP PRIMARY KEY"));
        } else {
            stmts.push(format!("ALTER TABLE {table} DROP INDEX {}", quote_ident(name)?));
        }
    }
    // 新增 / 删除外键
    for fk in &change.foreign_keys {
        stmts.push(format!("ALTER TABLE {table} ADD {}", foreign_key_definition(fk)?));
    }
    for name in &change.dropped_foreign_keys {
        stmts.push(format!(
            "ALTER TABLE {table} DROP FOREIGN KEY {}",
            quote_ident(name)?
        ));
    }
    // engine / charset / comment 变更（None 或空串 = 不变）
    if let Some(engine) = non_blank(&change.engine) {
        stmts.push(format!("ALTER TABLE {table} ENGINE={engine}"));
    }
    if let Some(charset) = non_blank(&change.charset) {
        stmts.push(format!("ALTER TABLE {table} DEFAULT CHARSET={charset}"));
    }
    if let Some(comment) = non_blank(&change.comment) {
        stmts.push(format!(
            "ALTER TABLE {table} COMMENT '{}'",
            comment.replace('\'', "''")
        ));
    }
    Ok(stmts)
}

/// 基础校验：表名 / 字段名非空且不含反引号，字段类型非空白，变更动作合法，
/// 索引 / 外键的列清单非空，engine / charset 为合法词法单元，外键规则在白名单内。
pub fn validate_change(change: &MySqlDesignChange) -> Result<(), AppError> {
    if !is_valid_identifier(&change.table) {
        return Err(AppError::general(format!("表名非法: {}", change.table)));
    }
    if change.is_new && change.columns.is_empty() {
        return Err(AppError::general(format!(
            "新表 {} 至少需要一个字段",
            change.table
        )));
    }
    for col in &change.columns {
        if !is_valid_identifier(&col.name) {
            return Err(AppError::general(format!("字段名非法: {}", col.name)));
        }
        if col.action != ACTION_ADD && col.action != ACTION_MODIFY && col.action != ACTION_DROP {
            return Err(AppError::general(format!(
                "字段 {} 的变更类型非法: {}（应为 add/modify/drop）",
                col.name, col.action
            )));
        }
        if col.action != ACTION_DROP {
            if col.data_type.trim().is_empty() {
                return Err(AppError::general(format!("字段 {} 缺少类型定义", col.name)));
            }
            if contains_ddl_breaker(&col.data_type) || contains_ddl_breaker(&col.extra) {
                return Err(AppError::general(format!(
                    "字段 {} 的类型或附加属性含非法字符（反引号 / 分号）",
                    col.name
                )));
            }
        }
    }
    for idx in &change.indexes {
        if !is_valid_identifier(&idx.name) {
            return Err(AppError::general(format!("索引名非法: {}", idx.name)));
        }
        ensure_identifiers(&idx.columns, "索引列")?;
    }
    for name in &change.dropped_indexes {
        if !is_valid_identifier(name) {
            return Err(AppError::general(format!("索引名非法: {name}")));
        }
    }
    for fk in &change.foreign_keys {
        if !is_valid_identifier(&fk.name) {
            return Err(AppError::general(format!("外键名非法: {}", fk.name)));
        }
        if !is_valid_identifier(&fk.ref_table) {
            return Err(AppError::general(format!("外键引用表名非法: {}", fk.ref_table)));
        }
        if fk.columns.is_empty() || fk.ref_columns.is_empty() {
            return Err(AppError::general(format!("外键 {} 的列清单为空", fk.name)));
        }
        ensure_identifiers(&fk.columns, "外键列")?;
        ensure_identifiers(&fk.ref_columns, "外键引用列")?;
        // ON DELETE / ON UPDATE 仅允许 MySQL 已知关键字，防 DDL 注入
        for rule in [&fk.on_delete, &fk.on_update] {
            if !rule.is_empty() && !is_valid_rule(rule) {
                return Err(AppError::general(format!(
                    "外键规则非法: {rule}（应为 RESTRICT/CASCADE/SET NULL/NO ACTION）"
                )));
            }
        }
    }
    for name in &change.dropped_foreign_keys {
        if !is_valid_identifier(name) {
            return Err(AppError::general(format!("外键名非法: {name}")));
        }
    }
    if let Some(engine) = change.engine.as_deref() {
        if !is_valid_keyword(engine.trim()) {
            return Err(AppError::general(format!("引擎名非法: {engine}")));
        }
    }
    if let Some(charset) = change.charset.as_deref() {
        if !is_valid_keyword(charset.trim()) {
            return Err(AppError::general(format!("字符集名非法: {charset}")));
        }
    }
    Ok(())
}

/// 单列定义片段：`name TYPE [NOT NULL] [DEFAULT x] [AUTO_INCREMENT] [COMMENT 'x']`
///
/// 默认值 / 注释中的单引号统一双写转义（`'` -> `''`）；默认值为 NULL 时生成 `DEFAULT NULL`。
fn column_definition(col: &MySqlColumnChange) -> Result<String, AppError> {
    let name = quote_ident(&col.name)?;
    let mut def = format!("{name} {}", col.data_type.trim());
    if !col.is_nullable {
        def.push_str(" NOT NULL");
    }
    if let Some(default_value) = col.default_value.as_deref().filter(|d| !d.is_empty()) {
        if default_value.eq_ignore_ascii_case("null") {
            def.push_str(" DEFAULT NULL");
        } else {
            def.push_str(&format!(" DEFAULT '{}'", default_value.replace('\'', "''")));
        }
    }
    let extra = col.extra.trim();
    if !extra.is_empty() {
        def.push_str(&format!(" {extra}"));
    }
    let comment = col.comment.trim();
    if !comment.is_empty() {
        def.push_str(&format!(" COMMENT '{}'", comment.replace('\'', "''")));
    }
    Ok(def)
}

/// CREATE TABLE 内联索引片段：PRIMARY KEY (...) / UNIQUE KEY name (...) / KEY name (...)
fn index_definition(idx: &MySqlIndexInfo) -> Result<String, AppError> {
    let cols = join_quoted(&idx.columns)?;
    if idx.is_primary {
        return Ok(format!("PRIMARY KEY ({cols})"));
    }
    let kind = if idx.is_unique { "UNIQUE KEY" } else { "KEY" };
    let name = quote_ident(&idx.name)?;
    Ok(format!("{kind} {name} ({cols})"))
}

/// 外键定义片段（CREATE TABLE 内联与 ALTER ADD CONSTRAINT 共用）：
/// `CONSTRAINT name FOREIGN KEY (cols) REFERENCES ref (cols) [ON DELETE x] [ON UPDATE x]`
fn foreign_key_definition(fk: &MySqlForeignKeyInfo) -> Result<String, AppError> {
    let name = quote_ident(&fk.name)?;
    let columns = join_quoted(&fk.columns)?;
    let ref_table = quote_ident(&fk.ref_table)?;
    let ref_columns = join_quoted(&fk.ref_columns)?;
    let mut def =
        format!("CONSTRAINT {name} FOREIGN KEY ({columns}) REFERENCES {ref_table} ({ref_columns})");
    if !fk.on_delete.is_empty() {
        def.push_str(&format!(" ON DELETE {}", fk.on_delete));
    }
    if !fk.on_update.is_empty() {
        def.push_str(&format!(" ON UPDATE {}", fk.on_update));
    }
    Ok(def)
}

/// 标识符加反引号包裹（调用前已校验非空、无反引号）
fn quote_ident(name: &str) -> Result<String, AppError> {
    if !is_valid_identifier(name) {
        return Err(AppError::general(format!("标识符非法: {name}")));
    }
    Ok(format!("`{name}`"))
}

/// 列列表反引号包裹后逗号连接（调用方已校验各列名）
fn join_quoted(cols: &[String]) -> Result<String, AppError> {
    let mut out = Vec::with_capacity(cols.len());
    for c in cols {
        out.push(quote_ident(c)?);
    }
    Ok(out.join(", "))
}

/// 标识符基础校验：非空且不含反引号（反引号为 MySQL 标识符转义符，直接拒绝）
fn is_valid_identifier(s: &str) -> bool {
    !s.is_empty() && !s.contains('`')
}

/// 引擎 / 字符集等词法 token 校验：非空且仅含字母 / 数字 / 下划线（防 DDL 注入）
fn is_valid_keyword(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// ON DELETE / ON UPDATE 规则白名单（MySQL InnoDB 仅支持这四种）
fn is_valid_rule(rule: &str) -> bool {
    matches!(
        rule.to_ascii_lowercase().as_str(),
        "cascade" | "set null" | "restrict" | "no action"
    )
}

/// DDL 拼接安全检查：类型 / 附加属性中不允许反引号与分号（防语句注入）
fn contains_ddl_breaker(s: &str) -> bool {
    s.contains('`') || s.contains(';')
}

/// 列清单统一校验：非空且每个元素均为合法标识符
fn ensure_identifiers(names: &[String], label: &str) -> Result<(), AppError> {
    if names.is_empty() {
        return Err(AppError::general(format!("{label}清单为空")));
    }
    for name in names {
        if !is_valid_identifier(name) {
            return Err(AppError::general(format!("{label}非法: {name}")));
        }
    }
    Ok(())
}

/// Option<String> 空串归一化：None 或空白串 -> None（空白串视为"不变"）
fn non_blank(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|s| !s.is_empty())
}
