//! 认证配置文件存储：SQLite 独立 Connection + 模块级注册表（OnceLock）
//!
//! - 不挂在 `AppState` 上（约束：state.rs / lib.rs 不改动），
//!   lib.rs setup 中调用一次 `auth_profile::init(&app_data_dir)` 即可
//! - 表：`auth_profiles`（id / name / auth_type JSON），与 ConfigStore 共用
//!   `fyshell.db` 文件但使用**独立 Connection**，因此删除配置文件时可以直接
//!   把引用它的会话的 `profile_id` 置空（不删除会话）

use std::path::Path;
use std::sync::{Mutex, OnceLock};

use rusqlite::{params, Connection};

use crate::error::AppError;
use crate::models::auth_profile::AuthProfile;
use crate::models::session::AuthType;

/// 模块级注册表：init 后全局可用
static STORE: OnceLock<AuthProfileStore> = OnceLock::new();

/// 认证配置文件存储：内部自带 `Mutex<Connection>`，方法均为同步签名
pub struct AuthProfileStore {
    conn: Mutex<Connection>,
}

/// 初始化（lib.rs setup 中调用；目录不存在则自动创建，并完成建表迁移）。
///
/// 幂等：重复调用时保留首次初始化的实例。
pub fn init(app_data_dir: &Path) -> Result<(), AppError> {
    std::fs::create_dir_all(app_data_dir)?;
    let conn = Connection::open(app_data_dir.join("fyshell.db"))?;
    let store = AuthProfileStore {
        conn: Mutex::new(conn),
    };
    {
        let conn = store.lock();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS auth_profiles (
                 id        TEXT PRIMARY KEY,
                 name      TEXT NOT NULL,
                 auth_type TEXT NOT NULL
             );",
        )?;
    }
    // 已初始化时丢弃本次实例（保留首次注册，避免替换运行中的连接）
    let _ = STORE.set(store);
    Ok(())
}

/// 访问注册表（未初始化视为调用方错误）
fn store() -> Result<&'static AuthProfileStore, AppError> {
    STORE.get().ok_or_else(|| AppError::general("认证配置文件存储未初始化"))
}

/// 列出全部认证配置文件（按名称排序）
pub fn list() -> Result<Vec<AuthProfile>, AppError> {
    let store = store()?;
    let conn = store.lock();
    let mut stmt = conn.prepare(
        "SELECT id, name, auth_type FROM auth_profiles ORDER BY name",
    )?;
    let profiles = stmt
        .query_map([], row_to_profile)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(profiles)
}

/// 保存认证配置文件（按 id upsert；新配置文件的 id 由 commands 层生成后传入）。
///
/// 主密码保险库已解锁时对 auth_type 的 password 字段加密落盘（与
/// sessions.auth_type 的既有加解密模式一致）；未解锁/未设置主密码时原样写入
///（密文 round-trip 无损，明文由下次解锁时的存量迁移兜底），不阻塞保存。
pub fn save(profile: &AuthProfile) -> Result<(), AppError> {
    let store = store()?;
    let conn = store.lock();
    let auth_json =
        crate::services::vault::encrypt_auth_json(&serde_json::to_string(&profile.auth_type)?)?;
    conn.execute(
        "INSERT INTO auth_profiles (id, name, auth_type) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET
             name = excluded.name,
             auth_type = excluded.auth_type",
        params![profile.id, profile.name, auth_json],
    )?;
    Ok(())
}

/// 删除认证配置文件：**不删除**引用该配置文件的会话，只把会话的 profile_id 置空
///（会话回退到使用自身 auth_type 配置）
pub fn delete(id: &str) -> Result<(), AppError> {
    let store = store()?;
    let conn = store.lock();
    // 先把引用置空（sessions 表与 auth_profiles 同文件，跨 Connection 同样生效）
    conn.execute(
        "UPDATE sessions SET profile_id = NULL WHERE profile_id = ?1",
        [id],
    )?;
    conn.execute("DELETE FROM auth_profiles WHERE id = ?1", [id])?;
    Ok(())
}

/// 锁住内部连接（Mutex 中毒视为不可恢复错误）
impl AuthProfileStore {
    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("auth_profile 内部连接锁中毒")
    }
}

/// 行 -> AuthProfile 的映射函数（auth_type 为 JSON 字符串）
fn row_to_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuthProfile> {
    // serde_json::Error 需手动转入 rusqlite::Error（query_map 闭包内无法提前用 ?）
    let auth_json: String = row.get("auth_type")?;
    // 已解锁时把密文 password 解回明文；未解锁/解密失败原样透传（密文 round-trip 无损）
    let auth_json = crate::services::vault::decrypt_auth_json(&auth_json);
    let auth_type: AuthType = serde_json::from_str(&auth_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(e),
        )
    })?;
    Ok(AuthProfile {
        id: row.get("id")?,
        name: row.get("name")?,
        auth_type,
    })
}
