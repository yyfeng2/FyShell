/**
 * MySQL 数据库级管理命令封装（commands/mysql_db.rs ↔ api/mysqlDb.ts）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 错误处理：Rust 侧 AppError 以字符串形式 reject，由调用方捕获处理。
 *
 * 命名约定：结构体字段一律 snake_case（serde 按 Rust 侧字段名反序列化，
 * 禁 rename_all="camelCase"）；invoke 顶层参数沿用 camelCase（Tauri 自动转换）。
 */
import { invoke } from '@tauri-apps/api/core';
import type { MySqlDatabaseList, MySqlTableDdl } from '@/bindings';

// 模型类型与后端 Rust 契约同源（bindings.ts 自动生成）：re-export 消除契约漂移源
export type { MySqlDatabaseList, MySqlTableDdl }

/** 列出该连接可见的全部数据库（含连接 host 与当前默认库） */
export function mysqlDbList(connId: string): Promise<MySqlDatabaseList> {
  return invoke<MySqlDatabaseList>('mysql_db_list', { connId });
}

/** 新建数据库（CREATE DATABASE） */
export function mysqlDbCreate(connId: string, name: string): Promise<void> {
  return invoke<void>('mysql_db_create', { connId, name });
}

/**
 * 删除数据库（DROP DATABASE，强确认）。
 * 首次调用不传 confirmed，后端返回带提示的错误；用户确认后带 confirmed=true 重发。
 */
export function mysqlDbDrop(connId: string, name: string, confirmed?: boolean): Promise<void> {
  return invoke<void>('mysql_db_drop', { connId, name, confirmed: confirmed ?? null });
}

/**
 * 切换默认数据库（重建连接池），返回新 connection_id。
 * 调用方以新 connId 替换 store 后刷新对象树与数据网格。
 */
export function mysqlDbSwitch(connId: string, name: string): Promise<string> {
  return invoke<string>('mysql_db_switch', { connId, name });
}

/** 取表的完整建表语句（SHOW CREATE TABLE） */
export function mysqlTableShowCreate(connId: string, table: string): Promise<MySqlTableDdl> {
  return invoke<MySqlTableDdl>('mysql_table_show_create', { connId, table });
}

/** 优化表空间（OPTIMIZE TABLE），执行时附带的结果集由后端消费 */
export function mysqlTableOptimize(connId: string, table: string): Promise<void> {
  return invoke<void>('mysql_table_optimize', { connId, table });
}

/** 重命名表（RENAME TABLE 旧表 TO 新表） */
export function mysqlTableRename(
  connId: string,
  oldName: string,
  newName: string,
): Promise<void> {
  return invoke<void>('mysql_table_rename', { connId, oldName, newName });
}
