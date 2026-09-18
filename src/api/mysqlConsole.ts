/**
 * SQL 控制台 P2 命令封装（查询历史 + 执行计划 + 多结果集）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 错误处理：Rust 侧 AppError 以字符串形式 reject，由调用方捕获处理。
 */
import { invoke } from '@tauri-apps/api/core';
import type { MySqlExplainResult, MySqlQueryHistoryItem, MySqlSavedQueryItem } from '@/bindings';

// 模型类型与后端 Rust 契约同源（bindings.ts 自动生成）：re-export 消除契约漂移源
export type { MySqlExplainResult, MySqlQueryHistoryItem, MySqlSavedQueryItem }

/** 列出查询历史（默认 100 条，按时间倒序） */
export function mysqlHistoryList(limit: number): Promise<MySqlQueryHistoryItem[]> {
  return invoke<MySqlQueryHistoryItem[]>('mysql_history_list', { limit });
}

/** 按关键词搜索查询历史 */
export function mysqlHistorySearch(
  keyword: string,
  limit: number,
): Promise<MySqlQueryHistoryItem[]> {
  return invoke<MySqlQueryHistoryItem[]>('mysql_history_search', { keyword, limit });
}

/** 清空查询历史（组件调用前需经 uiStore.confirm 二次确认） */
export function mysqlHistoryClear(): Promise<void> {
  return invoke<void>('mysql_history_clear');
}

// ---------------------------------------------------------------------------
// 已保存查询（命名保存，区别于自动记录的查询历史）
// ---------------------------------------------------------------------------

/** 列出已保存查询（按名称排序） */
export function mysqlSavedQueryList(): Promise<MySqlSavedQueryItem[]> {
  return invoke<MySqlSavedQueryItem[]>('mysql_saved_query_list');
}

/**
 * 保存查询（新增或覆盖），返回 true = 覆盖了同名记录。
 * 同名且未带 overwrite=true 时 reject（由调用方覆盖确认后重调）。
 */
export function mysqlSavedQuerySave(
  name: string,
  connId: string | null,
  sql: string,
  overwrite?: boolean,
): Promise<boolean> {
  return invoke<boolean>('mysql_saved_query_save', {
    name,
    connId,
    sql,
    overwrite: overwrite ?? null,
  });
}

/** 重命名已保存查询（按 id 定位；新名称与其它条目冲突时 reject） */
export function mysqlSavedQueryRename(id: number, name: string): Promise<void> {
  return invoke<void>('mysql_saved_query_rename', { id, name });
}

/** 删除已保存查询（按 id 定位） */
export function mysqlSavedQueryDelete(id: number): Promise<void> {
  return invoke<void>('mysql_saved_query_delete', { id });
}

/** 执行计划（EXPLAIN）；analyze=true 时执行 EXPLAIN ANALYZE */
export function mysqlExplain(connId: string, sql: string, analyze?: boolean): Promise<MySqlExplainResult> {
  return invoke<MySqlExplainResult>('mysql_explain', { connId, sql, analyze: analyze ?? null });
}
