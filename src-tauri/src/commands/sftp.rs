//! SFTP 传输命令层：契约第 2 节的 10 个命令，薄层转发（参数校验 + 调用 services）。

use tauri::ipc::Channel;
use tauri::State;

use crate::error::AppError;
use crate::models::transfer::{FileEntry, TransferKind, TransferStatus, TransferTask};
use crate::services::{sftp as sftp_service, sftp_store, transfer as transfer_service};
use crate::state::AppState;

/// 目录列表：`Vec<FileEntry>`
#[tauri::command]
#[specta::specta]
pub async fn sftp_list(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> Result<Vec<FileEntry>, AppError> {
    sftp_service::list(&state, &id, &path).await
}

/// 创建目录
#[tauri::command]
#[specta::specta]
pub async fn sftp_mkdir(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> Result<(), AppError> {
    sftp_service::mkdir(&state, &id, &path).await
}

/// 删除（前端二次确认；目录递归删除）
#[tauri::command]
#[specta::specta]
pub async fn sftp_delete(
    state: State<'_, AppState>,
    id: String,
    path: String,
    is_dir: bool,
) -> Result<(), AppError> {
    sftp_service::delete(&state, &id, &path, is_dir).await
}

/// 重命名
#[tauri::command]
#[specta::specta]
pub async fn sftp_rename(
    state: State<'_, AppState>,
    id: String,
    old_path: String,
    new_path: String,
) -> Result<(), AppError> {
    sftp_service::rename(&state, &id, &old_path, &new_path).await
}

/// 设置文件/目录权限（chmod）：`mode` 为八进制语义数值（如 0o644 = 420）
#[tauri::command]
#[specta::specta]
pub async fn sftp_chmod(
    state: State<'_, AppState>,
    id: String,
    path: String,
    mode: u32,
) -> Result<(), AppError> {
    sftp_service::chmod(&state, &id, &path, mode).await
}

/// 读取远程文件内容为 UTF-8 文本（文本编辑用；≤2MB，非 UTF-8/二进制报错）
#[tauri::command]
#[specta::specta]
pub async fn sftp_read_text(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> Result<String, AppError> {
    sftp_service::read_text(&state, &id, &path).await
}

/// 写回远程文件内容（全量 TRUNCATE 覆盖，UTF-8）——文本编辑保存
#[tauri::command]
#[specta::specta]
pub async fn sftp_write_text(
    state: State<'_, AppState>,
    id: String,
    path: String,
    content: String,
) -> Result<(), AppError> {
    sftp_service::write_text(&state, &id, &path, &content).await
}

/// SFTP 收藏路径列表（按收藏时间倒序）
#[tauri::command]
#[specta::specta]
pub fn sftp_favorite_list() -> Result<Vec<sftp_store::SftpFavorite>, AppError> {
    sftp_store::list()
}

/// 收藏路径：按 (side, path) 幂等，返回含 id 的收藏记录
#[tauri::command]
#[specta::specta]
pub fn sftp_favorite_add(side: String, path: String) -> Result<sftp_store::SftpFavorite, AppError> {
    sftp_store::add(&side, &path)
}

/// 取消收藏：按侧 + 路径删除（未收藏时幂等）
#[tauri::command]
#[specta::specta]
pub fn sftp_favorite_remove(side: String, path: String) -> Result<(), AppError> {
    sftp_store::remove(&side, &path)
}

/// 入队上传，进度走 Channel（返回含 id 的任务快照，前端可凭其取消）
#[tauri::command]
#[specta::specta]
pub async fn sftp_upload(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    local_path: String,
    remote_path: String,
    on_progress: Channel<TransferTask>,
) -> Result<TransferTask, AppError> {
    transfer_service::enqueue(
        &app,
        &state,
        TransferKind::Upload,
        &id,
        local_path,
        remote_path,
        on_progress,
    )
}

/// 入队下载，进度走 Channel
#[tauri::command]
#[specta::specta]
pub async fn sftp_download(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    remote_path: String,
    local_path: String,
    on_progress: Channel<TransferTask>,
) -> Result<TransferTask, AppError> {
    transfer_service::enqueue(
        &app,
        &state,
        TransferKind::Download,
        &id,
        local_path,
        remote_path,
        on_progress,
    )
}

/// 传输队列快照：`Vec<TransferTask>`
#[tauri::command]
#[specta::specta]
pub fn transfer_list(state: State<'_, AppState>) -> Vec<TransferTask> {
    state
        .transfer_tasks
        .lock()
        .expect("transfer_tasks 锁被污染")
        .clone()
}

/// 取消任务：写入取消集合，传输循环每批检查命中即中止
#[tauri::command]
#[specta::specta]
pub fn transfer_cancel(state: State<'_, AppState>, task_id: String) -> Result<(), AppError> {
    state
        .cancelled_transfers
        .lock()
        .expect("cancelled_transfers 锁被污染")
        .insert(task_id);
    Ok(())
}

/// 清除已完成 / 失败 / 已取消记录（Queued / Running 保留）
#[tauri::command]
#[specta::specta]
pub fn transfer_clear(state: State<'_, AppState>) -> Result<(), AppError> {
    let removed: Vec<String> = {
        let tasks = state.transfer_tasks.lock().expect("transfer_tasks 锁被污染");
        let removed = tasks
            .iter()
            .filter(|t| {
                !matches!(
                    t.status,
                    TransferStatus::Queued | TransferStatus::Running
                )
            })
            .map(|t| t.id.clone())
            .collect();
        removed
    };
    {
        let mut tasks = state.transfer_tasks.lock().expect("transfer_tasks 锁被污染");
        tasks.retain(|t| matches!(t.status, TransferStatus::Queued | TransferStatus::Running));
    }
    transfer_service::remove_channels(&removed);
    Ok(())
}

/// 本地目录列表（供双栏左侧使用）；路径为 ".." 时返回上级目录列表
#[tauri::command]
#[specta::specta]
pub fn local_list(path: String) -> Result<Vec<FileEntry>, AppError> {
    // ".." 返回上级：基于进程当前目录解析
    let target = if path == ".." {
        std::env::current_dir()?
            .parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| AppError::general("已位于根目录"))?
    } else {
        std::path::PathBuf::from(&path)
    };

    let mut entries = Vec::new();

    // 非根目录时提供 ".." 条目（is_dir: true，供双栏左侧返回上级）
    if target.parent().is_some() {
        entries.push(FileEntry {
            name: "..".to_string(),
            is_dir: true,
            size: 0,
            modified_at: 0,
            permissions: String::new(),
        });
    }

    let read_dir = std::fs::read_dir(&target)?;
    for entry in read_dir {
        let entry = entry?;
        let meta = entry.metadata()?;
        let name = entry.file_name();
        // 防御性处理：read_dir 不产生 "." 与 ".."
        if name == "." || name == ".." {
            continue;
        }
        entries.push(FileEntry {
            is_dir: meta.is_dir(),
            size: meta.len(),
            modified_at: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            // Windows 本地无 POSIX 权限位，返回空串
            permissions: String::new(),
            name: name.to_string_lossy().into_owned(),
        });
    }

    // 目录优先 + 名称不区分大小写排序
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

/// 本地新建文件夹（fe-sftp 双栏本地侧）
#[tauri::command]
#[specta::specta]
pub fn local_mkdir(path: String) -> Result<(), AppError> {
    std::fs::create_dir_all(&path)?;
    Ok(())
}

/// 本地重命名
#[tauri::command]
#[specta::specta]
pub fn local_rename(old_path: String, new_path: String) -> Result<(), AppError> {
    std::fs::rename(&old_path, &new_path)?;
    Ok(())
}

/// 本地删除：is_dir 为 true 时递归删除目录，否则删除文件
#[tauri::command]
#[specta::specta]
pub fn local_delete(path: String, is_dir: bool) -> Result<(), AppError> {
    if is_dir {
        std::fs::remove_dir_all(&path)?;
    } else {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}
