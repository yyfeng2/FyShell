/**
 * 凭据保险库命令封装（commands/vault.rs ↔ api/vault.ts）
 *
 * 已存连接密码的加解密完全在后端完成（DEK 从不过桥到前端）。前端流程：
 * - vaultStatus：判断 has_master_password / unlocked，决定 明文直读 / 解锁 / 加密保存
 * - vaultUnlock：以主密码解锁（解包 DEK 入 Rust 内存），此后 encrypt/decrypt 可用
 * - vaultLock：锁定（丢弃内存 DEK）
 * - vaultEncrypt/vaultDecrypt：保存/读取路径对明文凭据加解密
 */
import { invoke } from '@tauri-apps/api/core'
import type { VaultStatus } from '@/bindings'

/** 保险库状态（has_master_password / unlocked） */
export type { VaultStatus }

export function vaultStatus(): Promise<VaultStatus> {
  return invoke<VaultStatus>('vault_status')
}

export function vaultUnlock(password: string): Promise<void> {
  return invoke<void>('vault_unlock', { password })
}

export function vaultLock(): Promise<void> {
  return invoke<void>('vault_lock')
}

/** 加密明文凭据（保存路径；未解锁时 reject，由调用方引导解锁） */
export function vaultEncrypt(plain: string): Promise<string> {
  return invoke<string>('vault_encrypt', { plain })
}

/** 解密凭据密文（读取路径；未解锁时 reject） */
export function vaultDecrypt(cipher: string): Promise<string> {
  return invoke<string>('vault_decrypt', { cipher })
}
