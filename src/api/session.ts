/**
 * 会话管理命令封装（契约第 2 节：commands/session.rs ↔ api/session.ts）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 错误处理：Rust 侧 AppError 以字符串形式 reject，由调用方捕获处理。
 */
import { invoke } from '@tauri-apps/api/core';
import type {
  SessionConfig,
  SessionFolder,
  SessionNode,
  SessionTestResult,
} from './types';

/** 获取会话树（文件夹+会话混合节点） */
export function sessionList(): Promise<SessionNode[]> {
  return invoke<SessionNode[]>('session_list');
}

/** 保存会话配置；新会话传 id 为空字符串，Rust 侧生成 uuid 并返回带 id 的完整配置 */
export function sessionSave(config: SessionConfig): Promise<SessionConfig> {
  return invoke<SessionConfig>('session_save', { config });
}

/** 删除会话或文件夹（isFolder=true 时按文件夹删除，含其子节点） */
export function sessionDelete(id: string, isFolder: boolean): Promise<void> {
  return invoke<void>('session_delete', { id, isFolder });
}

/** 测试连接（不建立持久会话，仅验证认证参数） */
export function sessionTest(config: SessionConfig): Promise<SessionTestResult> {
  return invoke<SessionTestResult>('session_test', { config });
}

/** 保存文件夹；新文件夹传 id 为空字符串，Rust 侧生成 uuid 并返回完整对象 */
export function folderSave(folder: SessionFolder): Promise<SessionFolder> {
  return invoke<SessionFolder>('folder_save', { folder });
}
