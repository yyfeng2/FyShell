/**
 * MySQL 数据编辑命令封装（P2 阶段：commands/mysql_edit.rs ↔ api/mysqlEdit.ts）
 *
 * 「预览 -> 确认 -> 执行」管道的 API 层：
 * - mysql_edit_preview：生成 UPDATE 预览并用 COUNT(*) 估算影响行数
 * - mysql_update_row / mysql_update_rows / mysql_delete_row：确认后执行
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 错误处理：Rust 侧 AppError 以字符串形式 reject，由调用方捕获处理。
 *
 * 命名约定：结构体字段一律 snake_case（serde 按 Rust 侧字段名反序列化，
 * 禁 rename_all="camelCase"）；invoke 顶层参数沿用 camelCase（Tauri 自动转换）。
 */
import { invoke } from '@tauri-apps/api/core';

/** 单元格更新（对应 Rust 侧 models/mysql_edit.rs::MySqlRowUpdate） */
export interface MySqlRowUpdate {
  /** 目标表名 */
  table: string;
  /** 主键列名 */
  pk_column: string;
  /** 主键值（null = 主键为 NULL，按 pk IS NULL 定位） */
  pk_value: string | null;
  /** 目标列名 */
  column: string;
  /** 新值（is_null=true 时忽略） */
  value: string | null;
  /** true = 显式写 NULL（区分空串与 NULL） */
  is_null: boolean;
}

/** 批量更新（对应 Rust 侧 MySqlRowUpdateBatch；隐式事务包裹，失败整体回滚） */
export interface MySqlRowUpdateBatch {
  table: string;
  updates: MySqlRowUpdate[];
}

/** 编辑预览（对应 Rust 侧 MySqlEditPreview） */
export interface MySqlEditPreview {
  /** 将执行的 UPDATE 语句（值已转义内联） */
  sql: string;
  /** COUNT(*) 估算的受影响行数 */
  affected_estimate: number;
  /** 命中危险 SQL 词法特征（管道完整性保留） */
  danger: boolean;
  danger_reason: string | null;
}

/**
 * 表设计快照的子集类型：与 design 模块（@/api/mysqlDesign.ts）的 MySqlTableDesign
 * 同源，均来自 Rust 侧 commands/mysql_design.rs 的 mysql_table_design_get 返回。
 * 此处仅声明本模块所需字段（表名 + 列的 name/key_type），避免跨模块耦合。
 */
export interface MySqlTableDesignSnapshot {
  table: string;
  columns: {
    name: string;
    /** COLUMN_KEY：'PRI' 为主键 */
    key_type: string | null;
  }[];
}

/** mysql_edit_preview：生成 UPDATE 预览并估算影响行数 */
export function mysqlEditPreview(connId: string, update: MySqlRowUpdate): Promise<MySqlEditPreview> {
  return invoke<MySqlEditPreview>('mysql_edit_preview', { connId, update });
}

/** 单行更新：按主键定位写入新值，返回受影响行数 */
export function mysqlUpdateRow(
  connId: string,
  update: MySqlRowUpdate,
  confirmed?: boolean,
): Promise<number> {
  return invoke<number>('mysql_update_row', { connId, update, confirmed: confirmed ?? null });
}

/** 批量更新：逐条执行（隐式事务包裹，失败整体回滚），返回总受影响行数 */
export function mysqlUpdateRows(
  connId: string,
  batch: MySqlRowUpdateBatch,
  confirmed?: boolean,
): Promise<number> {
  return invoke<number>('mysql_update_rows', { connId, batch, confirmed: confirmed ?? null });
}

/** 单行删除：按主键定位删除，返回受影响行数（pkValue 传 null 表示 pk IS NULL） */
export function mysqlDeleteRow(
  connId: string,
  table: string,
  pkColumn: string,
  pkValue: string | null,
  confirmed?: boolean,
): Promise<number> {
  return invoke<number>('mysql_delete_row', {
    connId,
    table,
    pkColumn,
    pkValue,
    confirmed: confirmed ?? null,
  });
}

/**
 * 原生插入：逐行参数化 INSERT（事务包裹，失败整体回滚），返回插入行数。
 * columns 为空表示逐行插入全默认值行（后端生成 `INSERT INTO t () VALUES ()`）；
 * rows 每行值个数须与 columns 一致（null = NULL）。
 */
export function mysqlInsertRows(
  connId: string,
  table: string,
  columns: string[],
  rows: (string | null)[][],
  confirmed?: boolean,
): Promise<number> {
  return invoke<number>('mysql_insert_rows', {
    connId,
    table,
    columns,
    rows,
    confirmed: confirmed ?? null,
  });
}

/**
 * mysql_table_design_get：拉取指定表的设计快照，用于解析主键列。
 * 与 design 模块（@/api/mysqlDesign.ts）同源——同一后端命令，此处独立封装
 * 一个最小子集类型，避免数据编辑模块依赖表设计器模块的完整 API。
 */
export function mysqlTableDesignGet(connId: string, table: string): Promise<MySqlTableDesignSnapshot> {
  return invoke<MySqlTableDesignSnapshot>('mysql_table_design_get', { connId, table });
}
