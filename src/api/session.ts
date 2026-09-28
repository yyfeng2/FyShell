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

/** 会话克隆：读原配置 → 新 uuid → 名称加"副本"后缀 → 自动保存并返回新配置（不建立连接） */
export function sessionClone(id: string): Promise<SessionConfig> {
  return invoke<SessionConfig>('session_clone', { id });
}

/** 保存文件夹；新文件夹传 id 为空字符串，Rust 侧生成 uuid 并返回完整对象 */
export function folderSave(folder: SessionFolder): Promise<SessionFolder> {
  return invoke<SessionFolder>('folder_save', { folder });
}

/** 导航树批量重排载荷（拖拽归类/排序）：kind 区分文件夹/会话，parent_id 为新父级（None=根级），order 为同类顺序号 */
export interface NodeReorderItem {
  kind: 'folder' | 'session';
  id: string;
  parent_id: string | null;
  order: number;
}

/** 导航树批量重排（拖拽归类/排序，Rust 侧单事务持久化，父级与顺序一次更新） */
export function sessionReorder(items: NodeReorderItem[]): Promise<void> {
  return invoke<void>('session_reorder', { items });
}
