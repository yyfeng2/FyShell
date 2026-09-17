/**
 * 键位映射命令封装（commands/key_mapping.rs ↔ api/keyMapping.ts）
 *
 * 键位映射存 Rust 侧 SQLite key_mappings 表，前端 Pinia store 启动时加载、
 * 变更时写回（IPC 字段一律 snake_case）。
 */
import { invoke } from '@tauri-apps/api/core';

/** 动作类型：发送字符串 / 执行菜单命令 */
export type KeyMappingActionType = 'send_string' | 'menu_command';

/** 键位映射条目 */
export interface KeyMapping {
  /** uuid v4 */
  id: string;
  /** 归一化键位组合（如 "ctrl+shift+x" / "alt+f1"） */
  key_combo: string;
  /** 动作类型（send_string | menu_command） */
  action_type: KeyMappingActionType;
  /** 动作载荷：发送的字符串 或 菜单命令 action 名 */
  payload: string;
}

/** 键位映射列表 */
export function keyMappingList(): Promise<KeyMapping[]> {
  return invoke<KeyMapping[]>('key_mapping_list');
}

/** 保存键位映射（新映射 id 传空，Rust 侧生成） */
export function keyMappingSave(mapping: KeyMapping): Promise<KeyMapping> {
  return invoke<KeyMapping>('key_mapping_save', { mapping });
}

/** 删除键位映射 */
export function keyMappingDelete(id: string): Promise<void> {
  return invoke<void>('key_mapping_delete', { id });
}
