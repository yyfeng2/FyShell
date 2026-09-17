/**
 * SFTP 传输命令封装（契约第 2 节：commands/sftp.rs ↔ api/sftp.ts）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 注意：Windows 本地路径（反斜杠）原样传递，不做任何转换。
 */
import { invoke } from '@tauri-apps/api/core';
import { createChannel } from './channels';
import type { FileEntry, SftpFavorite, TransferTask } from './types';

/** 列出远程目录（path 为远程绝对路径） */
export function sftpList(id: string, path: string): Promise<FileEntry[]> {
  return invoke<FileEntry[]>('sftp_list', { id, path });
}

/** 在远程创建目录 */
export function sftpMkdir(id: string, path: string): Promise<void> {
  return invoke<void>('sftp_mkdir', { id, path });
}

/** 删除远程文件/目录（isDir=true 时按目录删除；调用方需先做二次确认） */
export function sftpDelete(
  id: string,
  path: string,
  isDir: boolean,
): Promise<void> {
  return invoke<void>('sftp_delete', { id, path, isDir });
}

/** 远程重命名 */
export function sftpRename(
  id: string,
  oldPath: string,
  newPath: string,
): Promise<void> {
  return invoke<void>('sftp_rename', { id, oldPath, newPath });
}

/** 设置远程文件/目录权限（mode 为八进制语义数值，如 0o644 = 420） */
export function sftpChmod(id: string, path: string, mode: number): Promise<void> {
  return invoke<void>('sftp_chmod', { id, path, mode });
}

/** 收藏路径列表（按收藏时间倒序） */
export function sftpFavoriteList(): Promise<SftpFavorite[]> {
  return invoke<SftpFavorite[]>('sftp_favorite_list');
}

/** 收藏路径（按 side+path 幂等；返回含 id 的收藏记录） */
export function sftpFavoriteAdd(side: string, path: string): Promise<SftpFavorite> {
  return invoke<SftpFavorite>('sftp_favorite_add', { side, path });
}

/** 取消收藏（按 side+path 删除，未收藏时幂等） */
export function sftpFavoriteRemove(side: string, path: string): Promise<void> {
  return invoke<void>('sftp_favorite_remove', { side, path });
}

/**
 * 入队上传任务，进度走 Channel。
 *
 * @param id 会话 ID
 * @param localPath 本地路径（Windows 反斜杠原样传递）
 * @param remotePath 远程目标路径
 * @param onProgress 进度回调（每个任务状态变化推送一次完整 TransferTask）
 */
export function sftpUpload(
  id: string,
  localPath: string,
  remotePath: string,
  onProgress: (task: TransferTask) => void,
): Promise<void> {
  const onProgressChannel = createChannel<TransferTask>(onProgress);
  return invoke<void>('sftp_upload', {
    id,
    localPath,
    remotePath,
    onProgress: onProgressChannel,
  });
}

/**
 * 入队下载任务，进度走 Channel。
 *
 * @param id 会话 ID
 * @param remotePath 远程源路径
 * @param localPath 本地目标路径（Windows 反斜杠原样传递）
 * @param onProgress 进度回调
 */
export function sftpDownload(
  id: string,
  remotePath: string,
  localPath: string,
  onProgress: (task: TransferTask) => void,
): Promise<void> {
  const onProgressChannel = createChannel<TransferTask>(onProgress);
  return invoke<void>('sftp_download', {
    id,
    remotePath,
    localPath,
    onProgress: onProgressChannel,
  });
}

/** 获取传输队列快照 */
export function transferList(): Promise<TransferTask[]> {
  return invoke<TransferTask[]>('transfer_list');
}

/** 取消指定任务（taskId 与契约命令参数 task_id 一致） */
export function transferCancel(taskId: string): Promise<void> {
  return invoke<void>('transfer_cancel', { taskId });
}

/** 清除已完成/失败的传输记录 */
export function transferClear(): Promise<void> {
  return invoke<void>('transfer_clear');
}

/** 列出本地目录（供双栏左侧使用，`..` 返回上级；Windows 路径原样传递） */
export function localList(path: string): Promise<FileEntry[]> {
  return invoke<FileEntry[]>('local_list', { path });
}
