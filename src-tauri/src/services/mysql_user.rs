//! MySQL 用户管理服务（对齐 Navicat 用户面板）。
//!
//! 用户管理均需管理员权限（mysql.user 表 SELECT / CREATE USER 权限），
//! 权限不足时的错误信息带上 MySQL 原始错误与提示（不回显密码）。
//! 复用 services/mysql 的连接池注册表（`pool_of`），不触碰 AppState / lib.rs。

use mysql_async::prelude::*;

use crate::error::AppError;
use crate::models::mysql_user::{MySqlGrantItem, MySqlUserInfo};
use crate::services::mysql::pool_of;

/// mysql_async::Error -> AppError 归入 General 变体（services/mysql 的同名助手为私有，此处独立定义）
fn mysql_err(e: mysql_async::Error) -> AppError {
    AppError::general(format!("MySQL 错误: {e}"))
}

/// 权限不足提示包装：错误信息带上原错误（密码不出现在 MySQL 错误文本中，安全）
fn admin_err(e: mysql_async::Error) -> AppError {
    AppError::general(format!(
        "MySQL 用户管理失败（需要管理员权限：mysql.user 表 SELECT / CREATE USER 权限）。原始错误: {e}"
    ))
}

/// 账户定位 `'user'@'host'`：用户/主机不含单引号（标识符校验，防注入）
fn quote_account(user: &str, host: &str) -> Result<String, AppError> {
    if user.contains('\'') || host.contains('\'') {
        return Err(AppError::general(
            "用户名与主机名不能包含单引号",
        ));
    }
    Ok(format!("'{user}'@'{host}'"))
}

/// 密码 -> SQL 字面量：单引号成对转义（先转反斜杠再成对转义单引号，参照 mysql_edit 思路）
///
/// 密码安全：转义结果仅内联进 SQL 发给服务端，不写入错误信息与日志。
fn quote_password(password: &str) -> String {
    password.replace('\\', "\\\\").replace('\'', "''")
}

/// user_list：列出 mysql.user 表全部用户（需要管理员权限）
pub async fn user_list(conn_id: &str) -> Result<Vec<MySqlUserInfo>, AppError> {
    let mut conn = pool_of(conn_id)?.get_conn().await.map_err(mysql_err)?;

    // mysql.user 表无 Comment 列，只取 User / Host（ORDER BY User 对齐 Navicat 排序）
    let sql = "SELECT User, Host FROM mysql.user ORDER BY User";
    let mut result = conn.query_iter(sql).await.map_err(admin_err)?;

    let mut users = Vec::new();
    while let Some(mut row) = result.next().await.map_err(mysql_err)? {
        users.push(MySqlUserInfo {
            user: row
                .take::<Option<String>, _>(0)
                .flatten()
                .unwrap_or_default(),
            host: row
                .take::<Option<String>, _>(1)
                .flatten()
                .unwrap_or_default(),
            // mysql.user 无 Comment 列，字段为契约预留（空串表示无备注）
            comment: String::new(),
        });
    }
    result.drop_result().await.map_err(mysql_err)?;
    Ok(users)
}

/// user_create：`CREATE USER 'user'@'host' IDENTIFIED BY 'password'`
///
/// 用户/主机不含单引号（标识符校验）；密码单引号成对转义后内联；
/// 权限不足时错误信息带提示与原错误。
pub async fn user_create(
    conn_id: &str,
    user: &str,
    host: &str,
    password: &str,
) -> Result<(), AppError> {
    let account = quote_account(user, host)?;
    let sql = format!(
        "CREATE USER {account} IDENTIFIED BY '{}'",
        quote_password(password)
    );
    let mut conn = pool_of(conn_id)?.get_conn().await.map_err(mysql_err)?;
    conn.query_drop(&sql).await.map_err(admin_err)?;
    Ok(())
}

/// user_drop：`DROP USER 'user'@'host'`（需要 CREATE USER 权限，权限不足时错误带提示）
pub async fn user_drop(conn_id: &str, user: &str, host: &str) -> Result<(), AppError> {
    let account = quote_account(user, host)?;
    let sql = format!("DROP USER {account}");
    let mut conn = pool_of(conn_id)?.get_conn().await.map_err(mysql_err)?;
    conn.query_drop(&sql).await.map_err(admin_err)?;
    Ok(())
}

/// user_grants：`SHOW GRANTS FOR 'user'@'host'`，多行结果（每行一条授权语句）
pub async fn user_grants(
    conn_id: &str,
    user: &str,
    host: &str,
) -> Result<Vec<MySqlGrantItem>, AppError> {
    let account = quote_account(user, host)?;
    let mut conn = pool_of(conn_id)?.get_conn().await.map_err(mysql_err)?;

    let sql = format!("SHOW GRANTS FOR {account}");
    let mut result = conn.query_iter(&sql).await.map_err(admin_err)?;

    let mut grants = Vec::new();
    while let Some(mut row) = result.next().await.map_err(mysql_err)? {
        grants.push(MySqlGrantItem {
            grant_sql: row
                .take::<Option<String>, _>(0)
                .flatten()
                .unwrap_or_default(),
        });
    }
    result.drop_result().await.map_err(mysql_err)?;
    Ok(grants)
}
