/**
 * 错误信息友好化：Rust 侧 AppError 以字符串形式 reject（"SSH 错误: ..." 等），
 * 底层 russh / rusqlite / mysql_async 错误多为英文原文，直接展示不友好。
 *
 * friendlyError 按规则表把常见错误族映射为中文结论，并附原文（截断）保留调试细节；
 * 未命中规则的错误原样返回（AppError 已带中文前缀，不重复加工）。
 */

/** 原文附加长度上限（超长截断，避免 SQL 语法错误等整段刷屏） */
const RAW_DETAIL_MAX = 160

/** 规则表：正则命中即返回对应文案（更具体的规则在前，命中即停） */
const RULES: { pattern: RegExp; message: string }[] = [
  { pattern: /ECONNREFUSED|Connection refused/i, message: '连接被拒绝：目标端口未监听或被防火墙拦截' },
  { pattern: /ETIMEDOUT|timed out|timeout/i, message: '连接超时：请检查网络与端口' },
  { pattern: /ENOTFOUND|no such host|getaddrinfo/i, message: '域名解析失败：请检查主机地址' },
  { pattern: /ENETUNREACH/i, message: '网络不可达：请检查网络连接' },
  { pattern: /Authentication failed|Access denied|permission denied/i, message: '认证失败：请检查用户名、密码或密钥' },
  { pattern: /connect error|connection not available|connect failed/i, message: '连接失败：请检查主机、端口与网络' },
  { pattern: /Connection reset|broken pipe|Connection closed/i, message: '连接已断开，请重试' },
  { pattern: /Unknown database/i, message: '数据库不存在' },
  { pattern: /ER_NO_SUCH_TABLE|doesn't exist/i, message: '表不存在' },
  { pattern: /Duplicate entry|ER_DUP_ENTRY/i, message: '唯一键冲突：记录已存在' },
  { pattern: /cannot be null|ER_BAD_NULL_ERROR/i, message: '字段不能为空（NULL）' },
  { pattern: /syntax error|\b1064\b/i, message: 'SQL 语法错误' },
  { pattern: /UNIQUE constraint failed/i, message: '记录已存在（唯一约束）' },
  { pattern: /FOREIGN KEY constraint failed/i, message: '存在关联数据，无法操作（外键约束）' },
  { pattern: /No such file|ENOENT/i, message: '文件或路径不存在' },
  { pattern: /Error \d+ \(\d{5}\)/, message: 'MySQL 报错' },
]

/** 未知错误 → 用户可读的友好文案（命中规则附原文，未命中原样返回） */
export function friendlyError(err: unknown): string {
  const raw = normalize(err)
  for (const rule of RULES) {
    if (rule.pattern.test(raw)) {
      return `${rule.message}（${truncate(raw)}）`
    }
  }
  return raw
}

/** 错误对象归一化：AppError reject 为字符串，其余取 message / String */
function normalize(err: unknown): string {
  if (typeof err === 'string') return err
  if (err instanceof Error) return err.message
  return String(err)
}

/** 原文截断到展示上限 */
function truncate(s: string): string {
  return s.length > RAW_DETAIL_MAX ? `${s.slice(0, RAW_DETAIL_MAX)}…` : s
}
