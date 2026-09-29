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

/**
 * 读取远程文件内容为 UTF-8 文本（文本编辑用）。
 * 超过 2MB 或非 UTF-8（二进制/含 NUL/其它编码）时后端报错，前端提示改用下载。
 */
export function sftpReadText(id: string, path: string): Promise<string> {
  return invoke<string>('sftp_read_text', { id, path });
}

/** 写回远程文件内容（全量 TRUNCATE 覆盖，UTF-8）——文本编辑保存 */
export function sftpWriteText(
  id: string,
  path: string,
  content: string,
): Promise<void> {
  return invoke<void>('sftp_write_text', { id, path, content });
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
): Promise<TransferTask> {
  const onProgressChannel = createChannel<TransferTask>(onProgress);
  return invoke<TransferTask>('sftp_upload', {
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
): Promise<TransferTask> {
  const onProgressChannel = createChannel<TransferTask>(onProgress);
  return invoke<TransferTask>('sftp_download', {
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

/** 本地文件系统起点列表（内置文件选择器初始视图）：Windows 各盘符 / 其他平台根 `/` */
export function localListRoots(): Promise<FileEntry[]> {
  return invoke<FileEntry[]>('local_list_roots');
}

/**
 * 自研 Windows 原生文件对话框（资源管理器选择器）。
 * send=选文件 / recv=选目录；用户取消返回 null。
 * 不用 @tauri-apps/plugin-dialog（rz/sz 场景曾导致鼠标指针不显示）。
 */
export interface LocalDialogOptions {
  /** 对话框标题 */
  title?: string
  /** 扩展名过滤组名（如「配色方案」） */
  filterName?: string
  /** 扩展名列表（如 ['json']） */
  extensions?: string[]
}

export function localPickDialog(
  mode: 'send' | 'recv',
  opts?: LocalDialogOptions,
): Promise<string | null> {
  return invoke<string | null>('local_pick_dialog', {
    mode,
    title: opts?.title ?? null,
    filterName: opts?.filterName ?? null,
    filterExtensions: opts?.extensions ?? null,
  })
}

/** 自研 Windows 原生保存对话框（IFileSaveDialog，带覆盖确认）；取消返回 null */
export function localSaveDialog(
  defaultPath: string,
  opts?: LocalDialogOptions,
): Promise<string | null> {
  return invoke<string | null>('local_save_dialog', {
    defaultPath,
    title: opts?.title ?? null,
    filterName: opts?.filterName ?? null,
    filterExtensions: opts?.extensions ?? null,
  })
}
