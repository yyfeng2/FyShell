/**
 * MySQL 导入导出命令封装（契约 P2 阶段：commands/mysql_io.rs ↔ api/mysqlIo.ts）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 错误处理：Rust 侧 AppError 以字符串形式 reject，由调用方捕获处理。
 *
 * ⚠️ 格式枚举为内部 tag 对象形式（Rust 侧 #[serde(tag = "format")]），
 * TS 侧对应字段为 { format: "csv" } 等，不是纯字符串。
 */
import { invoke } from '@tauri-apps/api/core';

/** 导出格式字面量（对应 Rust 侧 MySqlExportFormat） */
export type MySqlExportFormat = 'csv' | 'json' | 'sql';

/** 导入格式字面量（对应 Rust 侧 MySqlImportFormat；导入不支持 JSON） */
export type MySqlImportFormat = 'csv' | 'sql';

/** 导出选项（format 为内部 tag 对象形式） */
export interface MySqlExportOptions {
  /** 导出的查询 SQL（SELECT） */
  sql: string;
  /** 导出格式：{ format: "csv" | "json" | "sql" } */
  format: { format: MySqlExportFormat };
  /** 目标文件路径（前端保存对话框已让用户确认，后端直接覆盖写入） */
  file_path: string;
  /** SQL 格式时附带 DROP + CREATE TABLE（表结构） */
  include_create_table: boolean;
}

/** 导入选项（format 为内部 tag 对象形式） */
export interface MySqlImportOptions {
  /** 源文件路径 */
  file_path: string;
  /** 目标表（CSV 格式必填；SQL 格式的语句自带表名，此字段仅作展示） */
  table: string;
  /** 导入格式：{ format: "csv" | "sql" } */
  format: { format: MySqlImportFormat };
  /** 批量插入每批行数（默认 500，后端 clamp 1-1000） */
  batch_size: number;
  /** true = REPLACE INTO 覆盖已有主键（需 confirmed=true 重调） */
  replace: boolean;
}

/** 导入导出执行结果 */
export interface MySqlIoResult {
  /** 导出行数 / 导入行数 */
  rows_total: number;
  /** 耗时（毫秒） */
  duration_ms: number;
}

/**
 * 导出查询结果到文件。
 * options.replace 场景不适用；后端直接覆盖写入 file_path。
 */
export function mysqlExport(
  connId: string,
  options: MySqlExportOptions,
): Promise<MySqlIoResult> {
  return invoke<MySqlIoResult>('mysql_export', { connId, options });
}

/**
 * 从文件导入数据到目标表。
 * replace=true 且未确认时后端返回带提示的错误，前端确认后带 confirmed 重调。
 */
export function mysqlImport(
  connId: string,
  options: MySqlImportOptions,
  confirmed?: boolean,
): Promise<MySqlIoResult> {
  return invoke<MySqlIoResult>('mysql_import', {
    connId,
    options,
    confirmed: confirmed ?? null,
  });
}
