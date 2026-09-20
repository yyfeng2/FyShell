/**
 * 设置命令封装（commands/settings.rs ↔ api/settings.ts）
 *
 * 高功能设置：设置项以 key-value 形式存 Rust 侧 SQLite settings 表，
 * 前端 Pinia store 启动时加载、变更时写回（IPC 字段一律 snake_case）。
 */
import { invoke } from '@tauri-apps/api/core';

/** 设置项映射：key -> value（value 统一为字符串，读取方自行解析） */
export type SettingsMap = Record<string, string>;

/** 全量读取设置项 */
export function settingsGetAll(): Promise<SettingsMap> {
  return invoke<SettingsMap>('settings_get_all');
}

/** 读取单个设置项；未设置时返回 null */
export function settingsGet(key: string): Promise<string | null> {
  return invoke<string | null>('settings_get', { key });
}

/** 写入单个设置项（幂等覆盖） */
export function settingsSet(key: string, value: string): Promise<void> {
  return invoke<void>('settings_set', { key, value });
}

/** 设置关闭到托盘行为（运行时状态 + 托盘菜单勾选态同步） */
export function traySetCloseToTray(enabled: boolean): Promise<void> {
  return invoke<void>('tray_set_close_to_tray', { enabled });
}
