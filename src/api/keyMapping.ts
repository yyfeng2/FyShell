/**
 * 键位映射命令封装（commands/key_mapping.rs ↔ api/keyMapping.ts）
 *
 * 键位映射存 Rust 侧 SQLite key_mappings 表，前端 Pinia store 启动时加载、
 * 变更时写回（IPC 字段一律 snake_case）。
 */
import { invoke } from '@tauri-apps/api/core';
import type { KeyMapping as KeyMappingContract } from '@/bindings';

/** 动作类型：发送字符串 / 执行菜单命令（参数位与判定保持窄类型） */
export type KeyMappingActionType = 'send_string' | 'menu_command';

/**
 * 键位映射（字段同 bindings 契约，action_type 收窄为本地字面量以保 UI 判定类型安全；
 * specta 对 Rust 简单枚举导出为 string，本地复合类型在契约字段变化时仍同步）
 */
export type KeyMapping = Omit<KeyMappingContract, 'action_type'> & {
  action_type: KeyMappingActionType
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
