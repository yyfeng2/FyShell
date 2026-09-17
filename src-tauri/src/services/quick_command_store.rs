//! 快捷命令持久化：Rust 侧 SQLite（契约第 5 节 P1 扩展）
//!
//! - 数据库与 config_store 共用 `app_data_dir()/fyshell.db`，但持有独立 `Connection`
//!   （SQLite bundled 支持多连接同库读写，WAL/锁由 SQLite 内部仲裁）
//! - 模块级注册表模式：主会话在 lib.rs setup 中调用 `init(app_data_dir)`，
//!   命令层直接调用本模块的同步函数，不经过 AppState
//! - 建表迁移：quick_commands / quick_folders 两张表

use std::path::Path;
use std::sync::{Mutex, OnceLock};

use rusqlite::{params, Connection};

use crate::error::AppError;
use crate::models::quick_command::{QuickCommand, QuickCommandFolder, QuickCommandNode};

/// 内部连接注册表：`OnceLock<Mutex<Connection>>`，应用启动时经 `init` 注入
static CONN: OnceLock<Mutex<Connection>> = OnceLock::new();

/// 初始化快捷命令存储（目录不存在则自动创建，并完成建表迁移）
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
    CONN.get().expect("quick_command_store 未初始化：请在 lib.rs setup 中调用 init")
}

/// 锁住连接（Mutex 中毒视为不可恢复错误）
fn lock() -> std::sync::MutexGuard<'static, Connection> {
    registry().lock().expect("quick_command_store 内部连接锁中毒")
}

/// 建表迁移：quick_commands / quick_folders 两张表
fn init_schema(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        "BEGIN;
         CREATE TABLE IF NOT EXISTS quick_folders (
             id        TEXT PRIMARY KEY,
             name      TEXT NOT NULL,
             parent_id TEXT
         );
         CREATE TABLE IF NOT EXISTS quick_commands (
             id           TEXT PRIMARY KEY,
             name         TEXT NOT NULL,
             command_text TEXT NOT NULL,
             group_id     TEXT
         );
         COMMIT;",
    )?;
    Ok(())
}

/// 快捷命令/文件夹树（扁平全量：文件夹在前、命令在后，前端按 group_id/parent_id 组装层级）
pub fn list_nodes() -> Result<Vec<QuickCommandNode>, AppError> {
    let conn = lock();

    let mut nodes = Vec::new();

    // 文件夹在前
    let mut stmt = conn.prepare_cached("SELECT id, name, parent_id FROM quick_folders ORDER BY name")?;
    let folders: Vec<QuickCommandFolder> = stmt
        .query_map([], |row| {
            Ok(QuickCommandFolder {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    for folder in folders {
        nodes.push(QuickCommandNode::Folder(folder));
    }

    // 命令在后
    let mut stmt =
        conn.prepare_cached("SELECT id, name, command_text, group_id FROM quick_commands ORDER BY name")?;
    let commands = stmt
        .query_map([], |row| {
            Ok(QuickCommand {
                id: row.get(0)?,
                name: row.get(1)?,
                command_text: row.get(2)?,
                group_id: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for command in commands {
        nodes.push(QuickCommandNode::Command(command));
    }

    Ok(nodes)
}

/// 保存快捷命令（按 id upsert；新命令的 id 由 commands 层生成后传入）
pub fn save_command(cmd: &QuickCommand) -> Result<(), AppError> {
    let conn = lock();
    conn.execute(
        "INSERT INTO quick_commands (id, name, command_text, group_id)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET
             name = excluded.name,
             command_text = excluded.command_text,
             group_id = excluded.group_id",
        params![cmd.id, cmd.name, cmd.command_text, cmd.group_id],
    )?;
    Ok(())
}

/// 保存快捷命令文件夹（按 id upsert）
pub fn save_folder(folder: &QuickCommandFolder) -> Result<(), AppError> {
    let conn = lock();
    conn.execute(
        "INSERT INTO quick_folders (id, name, parent_id) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET
             name = excluded.name,
             parent_id = excluded.parent_id",
        params![folder.id, folder.name, folder.parent_id],
    )?;
    Ok(())
}

/// 删除单个快捷命令
pub fn delete_command(id: &str) -> Result<(), AppError> {
    let conn = lock();
    conn.execute("DELETE FROM quick_commands WHERE id = ?1", [id])?;
    Ok(())
}

/// 删除文件夹及其全部内容（递归收集子文件夹，删内部命令与子文件夹）
pub fn delete_folder(id: &str) -> Result<(), AppError> {
    let conn = lock();

    // BFS 收集以 id 为根的全部后代文件夹
    let mut to_delete = vec![id.to_string()];
    let mut idx = 0;
    while idx < to_delete.len() {
        let mut stmt = conn.prepare("SELECT id FROM quick_folders WHERE parent_id = ?1")?;
        let children: Vec<String> = stmt
            .query_map([&to_delete[idx]], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        drop(stmt);
        for child in children {
            to_delete.push(child);
        }
        idx += 1;
    }

    // 删除文件夹内的快捷命令
    let mut stmt = conn.prepare("DELETE FROM quick_commands WHERE group_id = ?1")?;
    for fid in &to_delete {
        stmt.execute([fid])?;
    }
    drop(stmt);

    // 删除文件夹本身
    let mut stmt = conn.prepare("DELETE FROM quick_folders WHERE id = ?1")?;
    for fid in &to_delete {
        stmt.execute([fid])?;
    }
    Ok(())
}
