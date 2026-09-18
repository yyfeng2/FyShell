/**
 * Redis 基础命令封装（契约：commands/redis.rs ↔ api/redis.ts）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 错误处理：Rust 侧 AppError 以字符串形式 reject，由调用方捕获处理。
 */
import { invoke } from '@tauri-apps/api/core';
import type {
  RedisConnection,
  RedisExecResult,
} from './types';

/** 建立 Redis 连接，返回 Rust 侧生成的 connection_id（config.db 作为默认库随连接选择） */
export function redisConnect(config: RedisConnection): Promise<string> {
  return invoke<string>('redis_connect', { config });
}

/** 关闭连接（connId 为 redisConnect 返回的连接 ID） */
export function redisDisconnect(connId: string): Promise<void> {
  return invoke<void>('redis_disconnect', { connId });
}

/** 测试 Redis 连接（建连即断，内部自断），返回服务器版本摘要 */
export function redisTest(config: RedisConnection): Promise<string> {
  return invoke<string>('redis_test', { config });
}

/** 拉取当前连接的服务器信息（INFO 命令聚合为键值对） */
export function redisInfo(connId: string): Promise<[string, string][]> {
  return invoke<[string, string][]>('redis_info', { connId });
}

/** 切换当前默认库（select n） */
export function redisSelectDb(connId: string, db: number): Promise<void> {
  return invoke<void>('redis_select_db', { connId, db });
}

/** 列出匹配 pattern 的键（KEYS pattern；空 pattern 传 "*"） */
export function redisKeys(connId: string, pattern: string): Promise<string[]> {
  return invoke<string[]>('redis_keys', { connId, pattern });
}

/** 任意 Redis 命令执行（args 为命令 + 参数，如 ["GET", "foo"]），结果按 kind 由调用方渲染 */
export function redisExec(connId: string, args: string[]): Promise<RedisExecResult> {
  return invoke<RedisExecResult>('redis_exec', { connId, args });
}
