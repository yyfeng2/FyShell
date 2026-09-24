//! SFTP 业务逻辑：目录列表 / mkdir / 删除（递归）/ 重命名 / 上传下载核心。
//!
//! 基于 services/ssh 的 `SshSessionHandle` 打开 russh-sftp channel。
//! 公开 API 形态：`pub async fn list(state: &AppState, id: &str, path: &str) -> Result<Vec<FileEntry>, AppError>` 等。

use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, OpenFlags, StatusCode};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};

use crate::error::AppError;
use crate::models::transfer::FileEntry;
use crate::services::ssh::SshSessionHandle;
use crate::state::AppState;

/// 每批读写字节数（架构红线：Rust 侧 4KB 批量读取）
const CHUNK_SIZE: usize = 4096;

/// 取消标记信息（队列侧以 cancelled_transfers 命中与否判定 Cancelled，此文案仅供展示）
const CANCELLED_MSG: &str = "传输已取消";

/// 从 AppState 取出会话句柄克隆（避免跨 await 持有锁）
fn get_handle(state: &AppState, id: &str) -> Result<SshSessionHandle, AppError> {
    state
        .ssh_sessions
        .lock()
        .expect("ssh_sessions 锁被污染")
        .get(id)
        .cloned()
        .ok_or_else(|| AppError::general(format!("会话 {id} 不存在或未连接")))
}

/// 打开 SFTP 子系统 channel，返回 russh-sftp 会话
async fn open_sftp(state: &AppState, id: &str) -> Result<SftpSession, AppError> {
    get_handle(state, id)?.open_sftp().await
}

/// russh-sftp 错误 → AppError（转字符串归入 Ssh 变体，与 error.rs 的归类注释一致）
fn sftp_err(err: impl std::fmt::Display) -> AppError {
    AppError::Ssh(err.to_string())
}

/// 目录列表：返回契约的 FileEntry；SFTP 服务器返回的 "." 与 ".." 不对前端暴露
pub async fn list(state: &AppState, id: &str, path: &str) -> Result<Vec<FileEntry>, AppError> {
    let sftp = open_sftp(state, id).await?;
    let read_dir = sftp.read_dir(path).await.map_err(sftp_err)?;

    let mut entries = Vec::new();
    for entry in read_dir {
        let name = entry.file_name();
        // 路径 "." 与 ".." 处理：跳过，不进入返回列表
        if name == "." || name == ".." {
            continue;
        }
        let meta = entry.metadata();
        entries.push(FileEntry {
            is_dir: entry.file_type().is_dir(),
            size: meta.size.unwrap_or(0),
            modified_at: meta.mtime.map(|t| t as i64).unwrap_or(0),
            // 八进制权限串，仅保留 rwx 权限位（不含类型位）
            permissions: meta
                .permissions
                .map(|p| format!("{:o}", p & 0o777))
                .unwrap_or_default(),
            name,
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

/// 创建目录
pub async fn mkdir(state: &AppState, id: &str, path: &str) -> Result<(), AppError> {
    let sftp = open_sftp(state, id).await?;
    sftp.create_dir(path).await.map_err(sftp_err)
}

/// 删除：文件直接删，目录递归删除
pub async fn delete(state: &AppState, id: &str, path: &str, is_dir: bool) -> Result<(), AppError> {
    // 安全护栏：拒绝删除根目录与特殊路径项
    if matches!(path, "" | "/" | "." | "..") {
        return Err(AppError::general(format!("非法删除路径: {path}")));
    }
    let sftp = open_sftp(state, id).await?;
    if is_dir {
        delete_dir_recursive(&sftp, path).await
    } else {
        sftp.remove_file(path).await.map_err(sftp_err)
    }
}

/// 递归删除目录：先清空全部子项，再删除目录本身
async fn delete_dir_recursive(sftp: &SftpSession, dir_path: &str) -> Result<(), AppError> {
    let read_dir = sftp.read_dir(dir_path).await.map_err(sftp_err)?;
    for entry in read_dir {
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let child = format!("{}/{}", dir_path.trim_end_matches('/'), name);
        if entry.file_type().is_dir() {
            // 异步递归需要装箱
            Box::pin(delete_dir_recursive(sftp, &child)).await?;
        } else {
            sftp.remove_file(&child).await.map_err(sftp_err)?;
        }
    }
    sftp.remove_dir(dir_path).await.map_err(sftp_err)
}

/// 重命名（SFTP v3 语义：目标已存在时可能报错）
pub async fn rename(
    state: &AppState,
    id: &str,
    old_path: &str,
    new_path: &str,
) -> Result<(), AppError> {
    let sftp = open_sftp(state, id).await?;
    sftp.rename(old_path, new_path).await.map_err(sftp_err)
}

/// 修改权限（chmod）：`mode` 为八进制语义的 u32（如 0o644）。
/// 仅设置权限位：size/uid/gid/时间均为 None，序列化时 attrs flags 只含 PERMISSIONS，
/// 不会误改文件大小、属主与时间戳。
pub async fn chmod(state: &AppState, id: &str, path: &str, mode: u32) -> Result<(), AppError> {
    let sftp = open_sftp(state, id).await?;
    let mut metadata = FileAttributes::default();
    // 保留 setuid/setgid/sticky 位（0o7777 全量掩码），与 chmod 语义一致
    metadata.permissions = Some(mode & 0o7777);
    sftp.set_metadata(path, metadata).await.map_err(sftp_err)
}

/// 续传截短（SETSTAT SIZE）降级：设置远端文件大小的调用失败时，若服务端明确
/// 不支持 size 属性（SSH_FX_OP_UNSUPPORTED），降级为仅 seek 续传（打 warning、
/// 不中断作业）——后续写入从已确认偏移起覆盖写至 EOF，最终远端大小仍正确；
/// 其余错误（IO/权限/超时等）如实报错中止。correctness-first 权衡：只有明确
/// Unsupported 才降级，其余如实报错。
async fn truncate_remote(
    remote: &russh_sftp::client::fs::File,
    size: u64,
    context: &str,
) -> Result<(), AppError> {
    let mut trunc = FileAttributes::default();
    trunc.size = Some(size);
    match remote.set_metadata(trunc).await {
        Ok(()) => Ok(()),
        Err(russh_sftp::client::error::Error::Status(status))
            if status.status_code == StatusCode::OpUnsupported =>
        {
            eprintln!(
                "[sftp] {context}: 服务端不支持 SETSTAT(SIZE) 截断（OP_UNSUPPORTED），\
                 降级为仅 seek 续传"
            );
            Ok(())
        }
        Err(err) => Err(sftp_err(err)),
    }
}

/// 上传核心逻辑：4KB 批量读写、每块更新进度、断点续传。
///
/// `progress(transferred, total)` 每写完一批调用一次，
/// 返回 false 表示任务已被取消，核心循环立即中止。
pub async fn upload_file<F>(
    state: &AppState,
    id: &str,
    local_path: &str,
    remote_path: &str,
    progress: &mut F,
) -> Result<(), AppError>
where
    F: FnMut(u64, u64) -> bool,
{
    let sftp = open_sftp(state, id).await?;

    // 断点续传：检测远端已存在文件大小，作为续传偏移。
    // 注意：russh-sftp 的 metadata() 对不存在的文件返回 Err(Error::Status(NoSuchFile))
    // 而非默认值——首次上传（远端无此文件）必须按 offset 0 处理，否则必然失败
    let resume_offset = match sftp.metadata(remote_path).await {
        Ok(meta) => meta.size.unwrap_or(0),
        Err(russh_sftp::client::error::Error::Status(status))
            if status.status_code == StatusCode::NoSuchFile =>
        {
            0
        }
        Err(err) => return Err(sftp_err(err)),
    };

    let mut local = tokio::fs::File::open(local_path).await?; // io::Error 经 From 归入 AppError::Io
    let local_size = local.metadata().await?.len();

    // 远端已不小于本地。恰相等视为传完；远端更大（本地文件曾缩小/被替换）时
    // 截断远端到本地大小，否则重传后残留 stale tail，文件大小与本地不一致；
    // 服务端不支持 size 属性时按 truncate_remote 降级（跳过截短、按完成处理）
    if resume_offset >= local_size {
        if resume_offset > local_size {
            let remote = sftp
                .open_with_flags(remote_path, OpenFlags::WRITE)
                .await
                .map_err(sftp_err)?;
            truncate_remote(&remote, local_size, "远端大于本地（截短至本地大小）").await?;
        }
        progress(local_size, local_size);
        return Ok(());
    }

    // 写 + 创建；非续传时清空旧内容，续传时保留已传部分
    let flags = if resume_offset > 0 {
        OpenFlags::WRITE | OpenFlags::CREATE
    } else {
        OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE
    };
    let mut remote = sftp
        .open_with_flags(remote_path, flags)
        .await
        .map_err(sftp_err)?;
    if resume_offset > 0 {
        // 续传先截断再续：把服务端文件截断到已确认的续传偏移（SETSTAT SIZE），
        // 再 seek 到该偏移写入——防止此前中断的传输在已确认长度之后残留多余数据，
        // 确保完成后服务端文件恰好等于 local_size；服务端不支持 size 属性时按
        // truncate_remote 降级为仅 seek 续写（从该偏移覆盖写至 EOF，大小仍正确）
        truncate_remote(&remote, resume_offset, "续传截断").await?;
        // 从已传偏移继续写
        remote.seek(SeekFrom::Start(resume_offset)).await.map_err(sftp_err)?;
    }

    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut transferred = resume_offset;
    loop {
        let n = local.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        remote.write_all(&buf[..n]).await.map_err(sftp_err)?;
        transferred += n as u64;
        // 每块更新进度；命中取消即中止
        if !progress(transferred, local_size) {
            return Err(AppError::general(CANCELLED_MSG));
        }
    }
    let _ = remote.sync_all().await; // fsync 失败不视为失败（部分服务器不支持）
    local.flush().await?;
    progress(transferred, transferred);
    Ok(())
}

/// 下载核心逻辑：4KB 批量读写、每块更新进度、断点续传。
///
/// `progress(transferred, total)` 每读到一批调用一次，返回 false 表示任务已被取消。
pub async fn download_file<F>(
    state: &AppState,
    id: &str,
    remote_path: &str,
    local_path: &str,
    progress: &mut F,
) -> Result<(), AppError>
where
    F: FnMut(u64, u64) -> bool,
{
    let sftp = open_sftp(state, id).await?;

    // 总字节数取自远端元数据
    let total = sftp
        .metadata(remote_path)
        .await
        .map_err(sftp_err)?
        .size
        .ok_or_else(|| AppError::general(format!("远端无文件大小信息: {remote_path}")))?;

    // 断点续传：本地已有字节数作为起始偏移
    let local_size = std::fs::metadata(local_path)
        .map(|m| m.len())
        .unwrap_or(0);
    // 本地已有文件大于远端总字节数（本地文件曾比远端大/上次未完成的下载残留）：
    // 先截断本地到已确认的完整长度（== total），否则 min(local, total) 续传会
    // 残留 stale tail 且仍返回成功，导致本地文件大小与远端不一致
    if local_size > total {
        std::fs::OpenOptions::new()
            .write(true)
            .open(local_path)?
            .set_len(total)?;
    }
    let resume_offset = local_size.min(total);

    let mut remote = sftp.open(remote_path).await.map_err(sftp_err)?;
    if resume_offset > 0 {
        // 远端 seek 到已传偏移
        remote.seek(SeekFrom::Start(resume_offset)).await.map_err(sftp_err)?;
    }

    // 本地以追加模式打开：从已传偏移继续写入
    let mut local = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(local_path)
        .await?;

    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut transferred = resume_offset;
    loop {
        let n = remote.read(&mut buf).await.map_err(sftp_err)?;
        if n == 0 {
            break;
        }
        local.write_all(&buf[..n]).await?;
        transferred += n as u64;
        // 每块更新进度；命中取消即中止
        if !progress(transferred, total) {
            return Err(AppError::general(CANCELLED_MSG));
        }
        if transferred >= total {
            break;
        }
    }
    local.flush().await?;
    local.sync_all().await?;
    progress(transferred, total);
    Ok(())
}
