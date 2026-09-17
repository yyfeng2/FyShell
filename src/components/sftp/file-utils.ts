/**
 * SFTP 双栏组件共享工具
 *
 * 职责：路径拼接与上级推导（本地 Windows / 远程 POSIX 双语义）、
 * 大小与时间格式化、显示名提取。
 * FilePane 与 DualPane 共用，避免在组件间复制逻辑。
 */
import type { FileEntry } from '@/api/types'

export type PaneSide = 'local' | 'remote'

/** FileEntry 统一复用契约类型（api/types） */
export type { FileEntry }

/** 各侧默认路径：本地为 Windows 盘符根，远程为 POSIX 根 */
export const DEFAULT_PATH: Record<PaneSide, string> = {
  local: 'C:\\',
  remote: '/',
}

/**
 * 拼接子路径
 * - remote：POSIX 风格，`/a` + `b` → `/a/b`
 * - local：Windows 风格，`C:\Users` + `b` → `C:\Users\b`
 */
export function joinPath(side: PaneSide, base: string, name: string): string {
  if (side === 'remote') {
    return base.endsWith('/') ? base + name : `${base}/${name}`
  }
  return /[\\/]$/.test(base) ? base + name : `${base}\\${name}`
}

/**
 * 计算上级路径；已在根目录时返回 null（用于禁用"上级"按钮）
 * - remote：`/home/foo` → `/home`，`/home` → `/`，`/` → null
 * - local：`C:\Users\foo` → `C:\Users`，`C:\Users` → `C:\`，`C:\` → null
 */
export function parentPath(side: PaneSide, path: string): string | null {
  if (side === 'remote') {
    if (!path || path === '/') return null
    const stripped = path.replace(/\/+$/, '')
    if (stripped === '') return '/'
    const idx = stripped.lastIndexOf('/')
    if (idx <= 0) return '/'
    return stripped.slice(0, idx)
  }
  // Windows 本地路径（兼容正反斜杠）
  const stripped = path.replace(/[\\/]+$/, '')
  if (!stripped) return null
  const idx = Math.max(stripped.lastIndexOf('\\'), stripped.lastIndexOf('/'))
  if (idx < 0) return null
  const parent = stripped.slice(0, idx)
  if (!parent) return null
  // 上级是盘符（如 C:）时归一为 C:\
  if (parent.length <= 2 && /^[A-Za-z]:$/.test(parent)) return `${parent}\\`
  return parent
}

/** 是否已处于根目录（此时"上级"按钮应禁用） */
export function isRootPath(side: PaneSide, path: string): boolean {
  return parentPath(side, path) === null
}

/** 字节数格式化为人类可读大小；目录等无效值显示 — */
export function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '—'
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = bytes / 1024
  let i = 0
  while (i < units.length - 1 && value >= 1024) {
    value /= 1024
    i++
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)} ${units[i]}`
}

/** Unix 秒 → 本地时间字符串（YYYY-MM-DD HH:mm） */
export function formatTime(unixSec: number): string {
  if (!Number.isFinite(unixSec) || unixSec <= 0) return '—'
  const d = new Date(unixSec * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** 取路径最后一段作为显示文件名（兼容正反斜杠） */
export function baseName(path: string): string {
  return path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() ?? path
}
