/**
 * 会话日志状态 store
 *
 * 负责：
 * - 各会话日志开关状态（session_id -> enabled；契约仅提供开关命令无状态查询，开关状态以前端记录为准）
 * - 各会话日志日期列表缓存
 * - 日志内容读取缓存（同一日期不重复走 IPC）
 *
 * 对应契约（docs/ipc-contracts.md §5.2）：session_log_toggle / session_log_list / session_log_read
 */
import { defineStore } from 'pinia'
import { ref } from 'vue'
import { sessionLogList, sessionLogRead, sessionLogToggle } from '@/api/sessionLog'

export const useLogStore = defineStore('log', () => {
  // ---------- state ----------
  /** 日志开关状态：session_id -> enabled（默认关闭） */
  const enabledMap = ref(new Map<string, boolean>())
  /** 各会话日志日期列表缓存：session_id -> 日期数组（如 "2026-09-17"） */
  const datesMap = ref(new Map<string, string[]>())
  /** 日志内容读取缓存：`sessionId::date` -> 内容 */
  const contentCache = ref(new Map<string, string>())
  const loading = ref(false)

  // ---------- actions ----------
  /** 开关会话日志记录（session_log_toggle），成功后回写本地开关状态 */
  async function toggle(sessionId: string, enabled: boolean): Promise<void> {
    await sessionLogToggle(sessionId, enabled)
    enabledMap.value = new Map(enabledMap.value).set(sessionId, enabled)
  }

  /** 查询某会话日志开关状态（前端记录值，无后端查询命令） */
  function isEnabled(sessionId: string): boolean {
    return enabledMap.value.get(sessionId) ?? false
  }

  /** 加载某会话的日志日期列表（session_log_list），结果写入缓存 */
  async function loadDates(sessionId: string): Promise<string[]> {
    loading.value = true
    try {
      const list = await sessionLogList(sessionId)
      const dates = Array.isArray(list) ? list : []
      datesMap.value = new Map(datesMap.value).set(sessionId, dates)
      return dates
    } finally {
      loading.value = false
    }
  }

  /** 某会话缓存的日期列表（未加载过返回空数组） */
  function cachedDates(sessionId: string): string[] {
    return datesMap.value.get(sessionId) ?? []
  }

  /** 读取指定日期日志内容（session_log_read），优先命中读取缓存 */
  async function read(sessionId: string, date: string): Promise<string> {
    const key = `${sessionId}::${date}`
    const hit = contentCache.value.get(key)
    if (hit !== undefined) return hit
    const content = await sessionLogRead(sessionId, date)
    contentCache.value.set(key, content)
    return content
  }

  /** 清空某会话的全部日志缓存（日期列表 + 内容；切会话/刷新场景使用） */
  function invalidate(sessionId: string): void {
    const dates = new Map(datesMap.value)
    dates.delete(sessionId)
    datesMap.value = dates
    const cache = new Map<string, string>()
    for (const [key, value] of contentCache.value) {
      if (!key.startsWith(`${sessionId}::`)) cache.set(key, value)
    }
    contentCache.value = cache
  }

  return {
    enabledMap,
    datesMap,
    contentCache,
    loading,
    toggle,
    isEnabled,
    loadDates,
    cachedDates,
    read,
    invalidate,
  }
})
