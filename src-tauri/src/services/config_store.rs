//! 会话配置持久化：Rust 侧 SQLite（凭据不进 WebView，架构红线 §1.3）
//!
//! - 数据库路径：`app_data_dir()/fyshell.db`（Tauri API 获取，建库时自动创建目录）
//! - AuthType 序列化为 JSON 字符串存入 sessions 表
//! - 主密码：P0 阶段预留接口（设置/验证口令），哈希为简单实现（FNV-1a 迭代 + 盐）

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::error::AppError;
use crate::models::session::{SessionConfig, SessionFolder, SessionNode};

/// 主密码哈希迭代轮数：P0 简单实现（FNV-1a 迭代拉伸 + 随机盐），
/// 正式版建议替换为 argon2（需在 Cargo.toml 追加依赖）
const MASTER_HASH_ROUNDS: u32 = 100_000;

/// 会话配置存储：内部自带 `Mutex<Connection>`，方法均为同步签名。
///
/// rusqlite 是阻塞 IO，但本地 SQLite 单条操作亚毫秒级，commands 层直接调用即可；
/// 重负载场景可再迁 `spawn_blocking`。数据库路径需在应用启动时
/// 经 `ConfigStore::open(app_data_dir)` 注入（Tauri API 才能拿到 app_data_dir）。
pub struct ConfigStore {
    conn: Mutex<Connection>,
}

impl ConfigStore {
    /// 从应用数据目录打开数据库（目录不存在则自动创建，并完成建表迁移）
    pub fn open(app_data_dir: &Path) -> Result<Self, AppError> {
        // 建库时自动创建目录
        std::fs::create_dir_all(app_data_dir)?;
        let conn = Connection::open(app_data_dir.join("fyshell.db"))?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// 建表迁移：sessions / folders / meta 三张表
    fn init_schema(&self) -> Result<(), AppError> {
        let conn = self.lock();
        conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS folders (
                 id        TEXT PRIMARY KEY,
                 name      TEXT NOT NULL,
                 parent_id TEXT
             );
             CREATE TABLE IF NOT EXISTS sessions (
                 id                 TEXT PRIMARY KEY,
                 name               TEXT NOT NULL,
                 folder_id          TEXT,
                 host               TEXT NOT NULL,
                 port               INTEGER NOT NULL,
                 username           TEXT NOT NULL,
                 auth_type          TEXT NOT NULL,
                 encoding           TEXT NOT NULL,
                 color              TEXT,
                 keepalive_interval INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS meta (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             COMMIT;",
        )?;
        // P1 迁移：老库补 profile_id 列（"duplicate column name" 说明是新库已含该列，忽略）
        match conn.execute("ALTER TABLE sessions ADD COLUMN profile_id TEXT", []) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(_, Some(msg)))
                if msg.contains("duplicate column name") => {}
            Err(e) => return Err(e.into()),
        }
        // 会话类型列："mysql" = 数据库会话，NULL/"ssh" = SSH 会话
        match conn.execute("ALTER TABLE sessions ADD COLUMN session_type TEXT", []) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(_, Some(msg)))
                if msg.contains("duplicate column name") => {}
            Err(e) => return Err(e.into()),
        }
        // 串口会话列：端口名 / 波特率（session_type == "serial" 时生效）
        match conn.execute("ALTER TABLE sessions ADD COLUMN serial_port TEXT", []) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(_, Some(msg)))
                if msg.contains("duplicate column name") => {}
            Err(e) => return Err(e.into()),
        }
        match conn.execute("ALTER TABLE sessions ADD COLUMN baud_rate INTEGER", []) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(_, Some(msg)))
                if msg.contains("duplicate column name") => {}
            Err(e) => return Err(e.into()),
        }
        // 备注/说明 + 最后修改时间列（打开会话对话框 7 列展示）
        match conn.execute("ALTER TABLE sessions ADD COLUMN description TEXT", []) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(_, Some(msg)))
                if msg.contains("duplicate column name") => {}
            Err(e) => return Err(e.into()),
        }
        match conn.execute("ALTER TABLE sessions ADD COLUMN updated_at INTEGER", []) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(_, Some(msg)))
                if msg.contains("duplicate column name") => {}
            Err(e) => return Err(e.into()),
        }
        // 迁移加列前已存在的行 updated_at 为 NULL，回填 0 保持数据一致
        conn.execute("UPDATE sessions SET updated_at = 0 WHERE updated_at IS NULL", [])?;
        Ok(())
    }

    /// 锁住内部连接（Mutex 中毒视为不可恢复错误）
    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .expect("config_store 内部连接锁中毒")
    }

    // ---------- 会话 CRUD ----------

    /// 会话/文件夹树（扁平全量：文件夹在前、会话在后，前端按 folder_id/parent_id 组装层级）。
    ///
    /// `filter` 非空时按名称/主机模糊过滤：返回名称或主机匹配的会话，
    /// 以及这些会话的祖先文件夹 + 名称匹配的文件夹（保证前端可重建树形）。
    pub fn list_nodes(&self, filter: Option<&str>) -> Result<Vec<SessionNode>, AppError> {
        let conn = self.lock();
        let filter = filter
            .map(str::trim)
            .filter(|f| !f.is_empty())
            .map(str::to_lowercase);

        // 全量加载文件夹到内存，用于祖先链追溯
        let mut stmt = conn.prepare("SELECT id, name, parent_id FROM folders")?;
        let all_folders: Vec<SessionFolder> = stmt
            .query_map([], |row| {
                Ok(SessionFolder {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    parent_id: row.get(2)?,
                })
            })?
            .collect::<Result<_, _>>()?;

        let mut nodes = Vec::new();

        if let Some(f) = &filter {
            let like = format!("%{f}%");

            // 匹配的会话（名称/主机模糊匹配）
            let mut stmt = conn.prepare_cached(
                "SELECT id, name, folder_id, host, port, username, auth_type,
                        encoding, color, keepalive_interval, profile_id, session_type,
                        serial_port, baud_rate, description, updated_at
                 FROM sessions
                 WHERE name LIKE ?1 OR host LIKE ?1",
            )?;
            let matched: Vec<SessionConfig> = stmt
                .query_map([&like], row_to_session)?
                .collect::<Result<_, _>>()?;

            // 需要保留的文件夹：名称匹配项 + 匹配会话的祖先文件夹
            let mut keep: BTreeSet<String> = all_folders
                .iter()
                .filter(|fd| fd.name.to_lowercase().contains(&f.to_lowercase()))
                .map(|fd| fd.id.clone())
                .collect();
            let by_id: std::collections::HashMap<&str, &SessionFolder> = all_folders
                .iter()
                .map(|fd| (fd.id.as_str(), fd))
                .collect();
            for session in &matched {
                let mut current = session.folder_id.as_deref();
                while let Some(fid) = current {
                    if !keep.insert(fid.to_string()) {
                        break; // 已包含，祖先链必然也已包含
                    }
                    current = by_id.get(fid).and_then(|fd| fd.parent_id.as_deref());
                }
            }

            for fd in &all_folders {
                if keep.contains(&fd.id) {
                    nodes.push(SessionNode::Folder(fd.clone()));
                }
            }
            nodes.extend(matched.into_iter().map(SessionNode::Session));
        } else {
            for fd in &all_folders {
                nodes.push(SessionNode::Folder(fd.clone()));
            }
            let mut stmt = conn.prepare_cached(
                "SELECT id, name, folder_id, host, port, username, auth_type,
                        encoding, color, keepalive_interval, profile_id, session_type,
                        serial_port, baud_rate, description, updated_at
                 FROM sessions ORDER BY name",
            )?;
            let sessions: Vec<SessionConfig> = stmt
                .query_map([], row_to_session)?
                .collect::<Result<_, _>>()?;
            nodes.extend(sessions.into_iter().map(SessionNode::Session));
        }

        Ok(nodes)
    }

    /// 按 id 读取单个会话配置；不存在返回 Ok(None)
    pub fn get_session(&self, id: &str) -> Result<Option<SessionConfig>, AppError> {
        let conn = self.lock();
        let mut stmt = conn.prepare_cached(
            "SELECT id, name, folder_id, host, port, username, auth_type,
                    encoding, color, keepalive_interval, profile_id, session_type,
                    serial_port, baud_rate, description, updated_at
             FROM sessions WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map([id], row_to_session)?;
        Ok(rows.next().transpose()?)
    }

    /// 保存会话（按 id upsert；新会话的 id 由 commands 层生成后传入）。
    ///
    /// `updated_at` 由 Rust 侧每次保存时取当前 Unix 秒覆盖（前端不传，保证修改时间真实）。
    pub fn save_session(&self, config: &SessionConfig) -> Result<(), AppError> {
        let conn = self.lock();
        let auth_json = serde_json::to_string(&config.auth_type)?;
        let updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        conn.execute(
            "INSERT INTO sessions (id, name, folder_id, host, port, username, auth_type,
                                   encoding, color, keepalive_interval, profile_id, session_type,
                                   serial_port, baud_rate, description, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name,
                 folder_id = excluded.folder_id,
                 host = excluded.host,
                 port = excluded.port,
                 username = excluded.username,
                 auth_type = excluded.auth_type,
                 encoding = excluded.encoding,
                 color = excluded.color,
                 keepalive_interval = excluded.keepalive_interval,
                 profile_id = excluded.profile_id,
                 session_type = excluded.session_type,
                 serial_port = excluded.serial_port,
                 baud_rate = excluded.baud_rate,
                 description = excluded.description,
                 updated_at = excluded.updated_at",
            params![
                config.id,
                config.name,
                config.folder_id,
                config.host,
                config.port as i64,
                config.username,
                auth_json,
                config.encoding,
                config.color,
                config.keepalive_interval as i64,
                config.profile_id,
                config.session_type,
                config.serial_port,
                config.baud_rate,
                config.description,
                updated_at,
            ],
        )?;
        Ok(())
    }

    /// 删除单个会话
    pub fn delete_session(&self, id: &str) -> Result<(), AppError> {
        let conn = self.lock();
        conn.execute("DELETE FROM sessions WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---------- 文件夹 CRUD ----------

    /// 保存文件夹（按 id upsert）
    pub fn save_folder(&self, folder: &SessionFolder) -> Result<(), AppError> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO folders (id, name, parent_id) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name,
                 parent_id = excluded.parent_id",
            params![folder.id, folder.name, folder.parent_id],
        )?;
        Ok(())
    }

    /// 删除文件夹及其全部内容（递归收集子文件夹，删内部会话与子文件夹）
    pub fn delete_folder(&self, id: &str) -> Result<(), AppError> {
        let conn = self.lock();

        // BFS 收集以 id 为根的全部后代文件夹
        let mut to_delete = vec![id.to_string()];
        let mut idx = 0;
        while idx < to_delete.len() {
            let mut stmt = conn.prepare("SELECT id FROM folders WHERE parent_id = ?1")?;
            let children: Vec<String> = stmt
                .query_map([&to_delete[idx]], |row| row.get(0))?
                .collect::<Result<_, _>>()?;
            drop(stmt);
            for child in children {
                to_delete.push(child);
            }
            idx += 1;
        }

        // 删除文件夹内的会话
        let mut stmt = conn.prepare("DELETE FROM sessions WHERE folder_id = ?1")?;
        for fid in &to_delete {
            stmt.execute([fid])?;
        }
        drop(stmt);

        // 删除文件夹本身
        let mut stmt = conn.prepare("DELETE FROM folders WHERE id = ?1")?;
        for fid in &to_delete {
            stmt.execute([fid])?;
        }
        Ok(())
    }

    // ---------- 主密码（P0 预留接口） ----------

    /// 是否已设置主密码
    pub fn has_master_password(&self) -> Result<bool, AppError> {
        let conn = self.lock();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM meta WHERE key = 'master_password'",
            [],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// 设置主密码（P0 预留：幂等覆盖；正式版应要求先验证旧密码）
    pub fn set_master_password(&self, password: &str) -> Result<(), AppError> {
        if password.is_empty() {
            return Err(AppError::general("主密码不能为空"));
        }
        // 盐来源：uuid v4（Cargo.toml 已声明 uuid，无 rand 依赖）
        let salt = uuid::Uuid::new_v4().simple().to_string();
        let hash = hash_password(password, &salt);
        let conn = self.lock();
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('master_password', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [format!("{salt}${hash}")],
        )?;
        Ok(())
    }

    /// 验证主密码；尚未设置时返回 Ok(false)
    pub fn verify_master_password(&self, password: &str) -> Result<bool, AppError> {
        let conn = self.lock();
        let stored: Option<String> = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'master_password'",
                [],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        let Some(stored) = &stored else {
            return Ok(false);
        };
        let (salt, expected) = stored
            .split_once('$')
            .ok_or_else(|| AppError::general("主密码哈希数据损坏"))?;
        Ok(hash_password(password, salt) == expected)
    }
}

/// 行 -> SessionConfig 的公共映射函数（auth_type 为 JSON 字符串）
fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionConfig> {
        // serde_json::Error 需手动转入 rusqlite::Error（query_map 闭包内无法提前用 ?）
        let auth_json: String = row.get("auth_type")?;
        let auth_type: crate::models::session::AuthType =
            serde_json::from_str(&auth_json).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
        Ok(SessionConfig {
            id: row.get("id")?,
            name: row.get("name")?,
            folder_id: row.get("folder_id")?,
            host: row.get("host")?,
            port: {
                let p: i64 = row.get("port")?;
                p as u16
            },
            username: row.get("username")?,
            auth_type,
            encoding: row.get("encoding")?,
            color: row.get("color")?,
            keepalive_interval: {
                let k: i64 = row.get("keepalive_interval")?;
                k as u32
            },
            profile_id: row.get("profile_id")?,
            session_type: row.get("session_type")?,
            serial_port: row.get("serial_port")?,
            baud_rate: row.get("baud_rate")?,
            description: row.get("description")?,
            // 迁移加列前已存在的行该列为 NULL，直接读 i64 会报 InvalidColumnType
            updated_at: row.get::<_, Option<i64>>("updated_at")?.unwrap_or(0),
        })
}

/// 口令哈希：FNV-1a 64 位迭代拉伸 + 盐。P0 简单实现，正式版换 argon2。
fn hash_password(password: &str, salt: &str) -> String {
    // FNV-1a offset basis
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut input = format!("{salt}:{password}");
    for _ in 0..MASTER_HASH_ROUNDS {
        for b in input.bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3); // FNV prime
        }
        // 每轮输出回填输入，实现迭代拉伸
        input = format!("{hash:016x}:{salt}");
    }
    format!("{hash:016x}")
}
