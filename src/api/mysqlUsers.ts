/**
 * MySQL 用户管理命令封装
 *
 * 契约：commands/mysql_users.rs ↔ api/mysqlUsers.ts
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 错误处理：Rust 侧 AppError 以字符串形式 reject，由调用方捕获处理。
 *
 * 命名约定：结构体字段一律 snake_case（serde 按 Rust 侧字段名反序列化，
 * 禁 rename_all="camelCase"）；invoke 顶层参数沿用 camelCase（Tauri 自动转换）。
 */
import { invoke } from '@tauri-apps/api/core';
import type { MySqlGrantItem, MySqlUserInfo } from '@/bindings';

// 模型类型与后端 Rust 契约同源（bindings.ts 自动生成）：re-export 消除契约漂移源
// （MySqlUserGrant 为后端 MySqlGrantItem 的别名保留，调用方命名不变）
export type { MySqlUserInfo }
export type MySqlUserGrant = MySqlGrantItem

/** 列出当前库可见的全部用户（权限不足时后端 reject，由调用方捕获展示） */
export function mysqlUserList(connId: string): Promise<MySqlUserInfo[]> {
  return invoke<MySqlUserInfo[]>('mysql_user_list', { connId });
}

/** 创建用户（CREATE USER ... IDENTIFIED BY ...） */
export function mysqlUserCreate(
  connId: string,
  user: string,
  host: string,
  password: string,
): Promise<void> {
  return invoke<void>('mysql_user_create', { connId, user, host, password });
}

/** 删除用户（DROP USER 'user'@'host'） */
export function mysqlUserDrop(connId: string, user: string, host: string): Promise<void> {
  return invoke<void>('mysql_user_drop', { connId, user, host });
}

/** 查看用户权限（SHOW GRANTS FOR 'user'@'host'） */
export function mysqlUserGrants(connId: string, user: string, host: string): Promise<MySqlUserGrant[]> {
  return invoke<MySqlUserGrant[]>('mysql_user_grants', { connId, user, host });
}
