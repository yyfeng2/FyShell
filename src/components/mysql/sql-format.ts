/**
 * SQL 格式化工具（SQL 编辑器右键菜单用）
 *
 * - formatSql：基于 sql-formatter 的 MySQL 方言格式化（关键字大小写可选）
 * - compressSql：压缩为单行（引号字面量内的空白保持原样，不做词法级压缩）
 *
 * 仅做纯文本变换，不调用 invoke、不触碰后端。
 */
import { format } from 'sql-formatter'

/** 关键字大小写模式 */
export type SqlKeywordCase = 'upper' | 'lower'

/**
 * 格式化 SQL（MySQL 方言）。
 * 空串或格式化失败时返回原文（右键菜单场景下保证内容不丢失）。
 */
export function formatSql(sql: string, keywordCase?: SqlKeywordCase): string {
  const text = sql.trim()
  if (!text) return sql
  try {
    return format(text, {
      language: 'mysql',
      // 未指定时保留用户原有大小写习惯
      keywordCase: keywordCase ?? 'preserve',
    })
  } catch {
    return sql
  }
}

/**
 * 压缩 SQL 为单行：引号外的连续空白折叠为单个空格。
 * 引号字面量（'...'、"..."、`...`）内的空白保持原样，避免破坏字符串内容。
 */
export function compressSql(sql: string): string {
  let out = ''
  let quote: string | null = null
  for (let i = 0; i < sql.length; i++) {
    const ch = sql[i]
    if (quote) {
      out += ch
      // 引号内的成对引号为转义，跳出转义字符后仍在引号内
      if (ch === quote && sql[i + 1] === quote) {
        out += sql[i + 1]
        i++
      } else if (ch === quote) {
        quote = null
      }
      continue
    }
    if (ch === "'" || ch === '"' || ch === '`') {
      quote = ch
      out += ch
      continue
    }
    if (/\s/.test(ch)) {
      // 折叠引号外空白：跳过后续空白，补一个空格（行首尾不加）
      while (i + 1 < sql.length && /\s/.test(sql[i + 1])) i++
      if (out && i + 1 < sql.length) out += ' '
      continue
    }
    out += ch
  }
  return out
}

/**
 * 按分号拆分多条语句（引号字面量内的分号不拆分），
 * 去除各语句首尾空白与结尾分号，空语句被过滤。
 */
export function splitSqlStatements(sql: string): string[] {
  const parts: string[] = []
  let current = ''
  let quote: string | null = null
  for (let i = 0; i < sql.length; i++) {
    const ch = sql[i]
    if (quote) {
      current += ch
      // 引号内的成对引号为转义，跳出转义字符后仍在引号内
      if (ch === quote && sql[i + 1] === quote) {
        current += sql[i + 1]
        i++
      } else if (ch === quote) {
        quote = null
      }
      continue
    }
    if (ch === "'" || ch === '"' || ch === '`') {
      quote = ch
      current += ch
      continue
    }
    if (ch === ';') {
      parts.push(current)
      current = ''
      continue
    }
    current += ch
  }
  parts.push(current)
  return parts.map((p) => p.trim()).filter((p) => p.length > 0)
}
