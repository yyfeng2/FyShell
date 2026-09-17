/**
 * MySQL 表设计器命令封装（契约：commands/mysql.rs ↔ api/mysqlDesign.ts）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 错误处理：Rust 侧 AppError 以字符串形式 reject，由调用方捕获处理。
 * 类型命名：一律 snake_case，与 Rust 契约保持一致（禁 camelCase rename）。
 */
import { invoke } from '@tauri-apps/api/core';

/** 表设计中单个字段的信息（读取结果） */
export interface MySqlColumnInfo {
  name: string;
  data_type: string;
  is_nullable: boolean;
  default_value: string | null;
  comment: string;
  /** AUTO_INCREMENT 等附加属性 */
  extra: string;
  /** 主键/唯一键标记（来自 information_schema，仅展示用） */
  key_type: string | null;
  character_set: string | null;
  collation: string | null;
}

/** 表设计中单个索引的信息 */
export interface MySqlIndexInfo {
  name: string;
  columns: string[];
  is_unique: boolean;
  is_primary: boolean;
  index_type: string;
}

/** 表设计中单个外键的信息 */
export interface MySqlForeignKeyInfo {
  name: string;
  columns: string[];
  ref_table: string;
  ref_columns: string[];
  on_delete: string;
  on_update: string;
}

/** 表结构读取结果 */
export interface MySqlTableDesign {
  table: string;
  columns: MySqlColumnInfo[];
  indexes: MySqlIndexInfo[];
  foreign_keys: MySqlForeignKeyInfo[];
  engine: string;
  charset: string;
  comment: string;
}

/** 字段变更动作：add = 新增，modify = 修改，drop = 删除 */
export type MySqlColumnAction = 'add' | 'modify' | 'drop';

/** 表设计变更载荷中的字段定义 */
export interface MySqlColumnChange {
  action: MySqlColumnAction;
  name: string;
  data_type: string;
  is_nullable: boolean;
  default_value: string | null;
  comment: string;
  extra: string;
}

/** 表设计变更载荷（保存时提交给后端） */
export interface MySqlDesignChange {
  table: string;
  is_new: boolean;
  columns: MySqlColumnChange[];
  indexes: MySqlIndexInfo[];
  dropped_indexes: string[];
  foreign_keys: MySqlForeignKeyInfo[];
  dropped_foreign_keys: string[];
  engine: string | null;
  charset: string | null;
  comment: string | null;
}

/** 拉取指定表的设计信息（字段/索引/外键/表属性） */
export function mysqlTableDesignGet(connId: string, table: string): Promise<MySqlTableDesign> {
  return invoke<MySqlTableDesign>('mysql_table_design_get', { connId, table });
}

/** 保存表设计变更，返回后端生成的真实 DDL 文本 */
export function mysqlTableDesignSave(
  connId: string,
  change: MySqlDesignChange,
): Promise<string> {
  return invoke<string>('mysql_table_design_save', { connId, change });
}
