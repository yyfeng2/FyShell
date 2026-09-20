/**
 * MySQL 基础命令封装（契约 5.2：commands/mysql.rs ↔ api/mysql.ts）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 错误处理：Rust 侧 AppError 以字符串形式 reject，由调用方捕获处理。
 */
import { invoke } from '@tauri-apps/api/core';
import type {
  MySqlConnection,
  MySqlQueryResult,
  MySqlTableInfo,
  MySqlDbFindHit,
} from './types';

/** 建立 MySQL 连接，返回 Rust 侧生成的 connection_id */
export function mysqlConnect(config: MySqlConnection): Promise<string> {
  return invoke<string>('mysql_connect', { config });
}

/** 关闭连接（connId 为 mysqlConnect 返回的连接 ID） */
export function mysqlDisconnect(connId: string): Promise<void> {
  return invoke<void>('mysql_disconnect', { connId });
}

/** 列出当前库的全部表信息 */
export function mysqlListTables(connId: string): Promise<MySqlTableInfo[]> {
  return invoke<MySqlTableInfo[]>('mysql_list_tables', { connId });
}

/** 分页查询：SELECT 类语句，返回列名/行数据/总数/分页信息 */
export function mysqlQuery(
  connId: string,
  sql: string,
  page: number,
  pageSize: number,
): Promise<MySqlQueryResult> {
  return invoke<MySqlQueryResult>('mysql_query', {
    connId,
    sql,
    page,
    pageSize,
  });
}

/** 执行写操作（INSERT/UPDATE/DELETE/DDL），返回受影响行数；危险 SQL 需传 confirmed=true */
export function mysqlExecute(
  connId: string,
  sql: string,
  confirmed?: boolean,
): Promise<number> {
  return invoke<number>('mysql_execute', { connId, sql, confirmed: confirmed ?? null });
}

/** 开启事务 */
export function mysqlBegin(connId: string): Promise<void> {
  return invoke<void>('mysql_begin', { connId });
}

/** 提交事务 */
export function mysqlCommit(connId: string): Promise<void> {
  return invoke<void>('mysql_commit', { connId });
}

/** 回滚事务 */
export function mysqlRollback(connId: string): Promise<void> {
  return invoke<void>('mysql_rollback', { connId });
}

/** 编辑数据库：修改默认字符集/排序规则（ALTER DATABASE），collation 可省略 */
export function mysqlDbEdit(
  connId: string,
  name: string,
  charset: string,
  collation?: string,
): Promise<void> {
  return invoke<void>('mysql_db_edit', {
    connId,
    name,
    charset,
    collation: collation ?? null,
  });
}

/** 全库表数据按关键字 LIKE 搜索（仅字符串列），按表分组返回命中行 */
export function mysqlDbFind(
  connId: string,
  name: string,
  keyword: string,
  maxPerTable?: number,
): Promise<MySqlDbFindHit[]> {
  return invoke<MySqlDbFindHit[]>('mysql_db_find', {
    connId,
    name,
    keyword,
    maxPerTable: maxPerTable ?? null,
  });
}
