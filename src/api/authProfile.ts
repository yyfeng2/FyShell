/**
 * 认证配置文件命令封装（契约 5.2：commands/auth_profile.rs ↔ api/authProfile.ts）
 *
 * 一处改全局生效：SessionForm 选择配置文件后 auth_type 从配置文件解析。
 */
import { invoke } from '@tauri-apps/api/core';
import type { AuthProfile } from './types';

/** 获取全部认证配置文件 */
export function authProfileList(): Promise<AuthProfile[]> {
  return invoke<AuthProfile[]>('auth_profile_list');
}

/** 保存认证配置文件；新配置文件传 id 为空字符串，Rust 侧生成 uuid 并返回完整对象 */
export function authProfileSave(profile: AuthProfile): Promise<AuthProfile> {
  return invoke<AuthProfile>('auth_profile_save', { profile });
}

/** 删除认证配置文件 */
export function authProfileDelete(id: string): Promise<void> {
  return invoke<void>('auth_profile_delete', { id });
}
