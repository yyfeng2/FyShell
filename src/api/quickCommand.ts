/**
 * 快捷命令命令封装（契约 5.2：commands/quick_command.rs ↔ api/quickCommand.ts）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 发送到会话复用既有 ssh_write（api/ssh.ts），本模块无对应命令。
 */
import { invoke } from '@tauri-apps/api/core';
import type {
  QuickCommand,
  QuickCommandFolder,
  QuickCommandNode,
} from './types';

/** 获取快捷命令树（文件夹+命令混合节点） */
export function qcList(): Promise<QuickCommandNode[]> {
  return invoke<QuickCommandNode[]>('qc_list');
}

/** 保存快捷命令；新命令传 id 为空字符串，Rust 侧生成 uuid 并返回完整对象 */
export function qcSaveCommand(cmd: QuickCommand): Promise<QuickCommand> {
  return invoke<QuickCommand>('qc_save_command', { cmd });
}

/** 保存快捷命令文件夹；新文件夹传 id 为空字符串，Rust 侧生成 uuid 并返回完整对象 */
export function qcSaveFolder(
  folder: QuickCommandFolder,
): Promise<QuickCommandFolder> {
  return invoke<QuickCommandFolder>('qc_save_folder', { folder });
}

/** 删除快捷命令或文件夹（isFolder=true 时按文件夹删除，含其子节点） */
export function qcDelete(id: string, isFolder: boolean): Promise<void> {
  return invoke<void>('qc_delete', { id, isFolder });
}
