/**
 * SSH 隧道命令封装（契约 5.2：commands/tunnel.rs ↔ api/tunnel.ts）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 */
import { invoke } from '@tauri-apps/api/core';
import type { TunnelRule, TunnelStartReport } from './types';

/** 列出隧道规则；不传 sessionId 返回全部规则 */
export function tunnelList(sessionId?: string): Promise<TunnelRule[]> {
  return invoke<TunnelRule[]>('tunnel_list', { sessionId: sessionId ?? null });
}

/** 保存隧道规则；新规则传 id 为空字符串，Rust 侧生成 uuid 并返回带 id 的完整规则 */
export function tunnelSave(rule: TunnelRule): Promise<TunnelRule> {
  return invoke<TunnelRule>('tunnel_save', { rule });
}

/** 删除隧道规则（Rust 侧先停止监听再删除） */
export function tunnelDelete(id: string): Promise<void> {
  return invoke<void>('tunnel_delete', { id });
}

/** 启动隧道监听 */
export function tunnelStart(id: string): Promise<void> {
  return invoke<void>('tunnel_start', { id });
}

/** 停止隧道监听 */
export function tunnelStop(id: string): Promise<void> {
  return invoke<void>('tunnel_stop', { id });
}

/** 一键全启：遍历全部规则逐个启动（已在运行中的跳过），返回成功/失败计数 */
export function tunnelStartAll(): Promise<TunnelStartReport> {
  return invoke<TunnelStartReport>('tunnel_start_all');
}
