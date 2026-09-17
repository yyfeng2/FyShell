/**
 * SQL 控制台 P2 命令封装（查询历史 + 执行计划 + 多结果集）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 错误处理：Rust 侧 AppError 以字符串形式 reject，由调用方捕获处理。
 */
import { invoke } from '@tauri-apps/api/core';
import type { MySqlQueryResult } from './types';

/** 查询历史条目（每次执行 SQL 后由 Rust 侧落库） */
export interface MySqlQueryHistoryItem {
  id: number;
  conn_id: string;
  sql: string;
  created_at: string;
  /** 执行耗时（毫秒）；null 表示未记录 */
  duration_ms: number | null;
  success: boolean;
}

/** EXPLAIN 结果：rows 表格 / tree 文本树 */
export interface MySqlExplainResult {
  format: 'rows' | 'tree';
  columns: string[];
  rows: (string | null)[][];
  /** 文本树（format="tree" 时非空） */
  tree: string | null;
}

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

/** 已保存查询条目（用户命名，区别于查询历史） */
export interface MySqlSavedQueryItem {
  id: number;
  /** 查询名称（后端 UNIQUE 约束，同名保存走覆盖语义） */
  name: string;
  /** 绑定的连接标识（null = 不绑定连接，全局可见） */
  conn_id: string | null;
  sql: string;
  created_at: string;
}

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

/** 多结果集查询：一次执行多条 SELECT，逐条返回结果 */
export function mysqlQueryMulti(connId: string, sql: string): Promise<MySqlQueryResult[]> {
  return invoke<MySqlQueryResult[]>('mysql_query_multi', { connId, sql });
}
