/**
 * MySQL 数据库对象命令封装（契约：commands/mysql_objects.rs ↔ api/mysqlObjects.ts）
 *
 * 覆盖视图/函数/存储过程/触发器/事件五类对象的增删查（DDL 查看/保存）。
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 错误处理：Rust 侧 AppError 以字符串形式 reject，由调用方捕获处理。
 *
 * 命名约定：结构体字段一律 snake_case（serde 按 Rust 侧字段名反序列化，
 * 禁 rename_all="camelCase"）；invoke 顶层参数沿用 camelCase（Tauri 自动转换）。
 */
import { invoke } from '@tauri-apps/api/core';
import type { MySqlObjectDdl, MySqlObjectInfo } from '@/bindings';

/** 数据库对象类型（对应 Rust enum ObjectKind；OBJ 参数位保持窄类型） */
export type MySqlObjectKind = 'view' | 'function' | 'procedure' | 'trigger' | 'event';

// 对象条目/DDL 与后端 Rust 契约同源（bindings.ts 自动生成）：re-export 消除契约漂移源
export type { MySqlObjectDdl, MySqlObjectInfo }

/** 列出当前库指定类型的全部对象 */
export function mysqlObjectList(connId: string, kind: MySqlObjectKind): Promise<MySqlObjectInfo[]> {
  return invoke<MySqlObjectInfo[]>('mysql_object_list', { connId, kind });
}

/** 查看指定对象的 DDL（CREATE 语句） */
export function mysqlObjectDdl(
  connId: string,
  kind: MySqlObjectKind,
  name: string,
): Promise<MySqlObjectDdl> {
  return invoke<MySqlObjectDdl>('mysql_object_ddl', { connId, kind, name });
}

/** 保存对象：后端先 DROP 旧对象再执行新 CREATE */
export function mysqlObjectSave(
  connId: string,
  kind: MySqlObjectKind,
  name: string,
  createSql: string,
): Promise<void> {
  return invoke<void>('mysql_object_save', { connId, kind, name, createSql });
}

/** 删除对象（DROP） */
export function mysqlObjectDrop(connId: string, kind: MySqlObjectKind, name: string): Promise<void> {
  return invoke<void>('mysql_object_drop', { connId, kind, name });
}
