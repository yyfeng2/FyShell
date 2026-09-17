/**
 * 主密码命令封装（commands/master_password.rs ↔ api/masterPassword.ts）
 *
 * P0 预留接口接线：主密码哈希存 Rust 侧 SQLite meta 表（凭据不进 WebView），
 * 前端仅做设置/校验入口，用于后续凭据加密。
 */
import { invoke } from '@tauri-apps/api/core';

/** 是否已设置主密码（前端据此决定首次设置 / 修改模式） */
export function masterPasswordStatus(): Promise<boolean> {
  return invoke<boolean>('master_password_status');
}

/** 设置主密码（Rust 侧幂等覆盖；修改前建议先调 masterPasswordVerify 校验旧密码） */
export function masterPasswordSet(password: string): Promise<void> {
  return invoke<void>('master_password_set', { password });
}

/** 验证主密码；尚未设置时返回 false */
export function masterPasswordVerify(password: string): Promise<boolean> {
  return invoke<boolean>('master_password_verify', { password });
}
