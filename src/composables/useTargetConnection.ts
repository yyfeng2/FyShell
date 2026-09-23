/**
 * useTargetConnection —— 数据库工具对话框的目标连接管理
 *
 * 数据传输/数据同步/结构同步三个对话框共用：目标连接下拉 =「当前连接」
 * （connId 直用，不建新连接）+ 已保存连接（选中后用 raw mysqlConnect 建立
 * **独立**目标连接——禁用 store.connect，那会断开当前连接），目标库下拉随连接
 * 联动加载；关闭对话框时 dispose 断开建立的独立连接。
 */
import { computed, ref } from 'vue'
import { mysqlConnect, mysqlDisconnect } from '@/api/mysql'
import { mysqlDbList } from '@/api/mysqlDb'
import { useMysqlStore } from '@/stores/mysql'
import { useUiStore } from '@/stores/ui'

import { friendlyError as errText } from '@/utils/errors'

/** 错误归一化：Rust 侧 AppError 以字符串 reject，转发共享友好化工具（errText 别名） */
export { errText }

/** 目标连接下拉选中值：'__current__' = 当前连接，否则为已保存连接 id */
export const TARGET_CURRENT = '__current__'

export function useTargetConnection() {
  const store = useMysqlStore()
  const ui = useUiStore()

  /** 目标连接下拉选中值 */
  const targetKey = ref<string>(TARGET_CURRENT)
  /** 目标连接实际使用的后端连接 ID（当前连接 = store.connId；已保存连接 = mysqlConnect 建立的独立连接） */
  const targetConnId = ref<string | null>(null)
  /** 目标连接的库列表（mysql_db_list，联动库下拉） */
  const targetDbs = ref<string[]>([])
  /** 目标库下拉选中值 */
  const targetDb = ref('')
  /** 目标连接建立/库列表加载中 */
  const targetLoading = ref(false)

  /** mysqlConnect 建立的独立连接 ID（dispose 时断开；当前连接直用时为 null） */
  let established: string | null = null

  /** 目标连接下拉候选：当前连接 + 已保存连接 */
  const targetItems = computed<{ key: string; label: string }[]>(() => {
    const out: { key: string; label: string }[] = []
    if (store.connId) {
      out.push({ key: TARGET_CURRENT, label: `当前连接（${store.connLabel}）` })
    }
    for (const c of store.savedConnections) {
      out.push({ key: c.id, label: `${c.name}（${c.host}:${c.port}）` })
    }
    return out
  })

  /** 加载目标连接的库列表 */
  async function loadTargetDbs(): Promise<void> {
    if (!targetConnId.value) return
    targetLoading.value = true
    try {
      const result = await mysqlDbList(targetConnId.value)
      targetDbs.value = result.databases
    } catch (err) {
      ui.toast(errText(err), 'error')
    } finally {
      targetLoading.value = false
    }
  }

  /** 建立目标连接并加载库列表（重复调用先清理旧连接；'__current__' 直用当前连接） */
  async function connectTarget(): Promise<void> {
    targetDbs.value = []
    targetDb.value = ''
    targetConnId.value = null
    if (established) {
      void mysqlDisconnect(established).catch(() => undefined)
      established = null
    }
    if (targetKey.value === TARGET_CURRENT) {
      // 当前连接直用（不建新连接，关闭时也不断开）
      if (!store.connId) {
        ui.toast('当前 MySQL 连接已断开，请重新连接', 'error')
        return
      }
      targetConnId.value = store.connId
      await loadTargetDbs()
      return
    }
    const saved = store.savedConnections.find((c) => c.id === targetKey.value)
    if (!saved) {
      ui.toast('目标连接不存在或已删除', 'error')
      return
    }
    targetLoading.value = true
    try {
      // 独立连接：与工作台当前连接并存（schema 传 null，库上下文由工具 SQL 限定）
      established = await mysqlConnect({
        host: saved.host,
        port: saved.port,
        username: saved.username,
        password: saved.password,
        schema: null,
      })
      targetConnId.value = established
      await loadTargetDbs()
    } catch (err) {
      ui.toast(errText(err), 'error')
    } finally {
      targetLoading.value = false
    }
  }

  /** 切换目标连接（下拉 @update:model-value 入口） */
  async function selectTarget(key: string): Promise<void> {
    if (key === targetKey.value) return
    targetKey.value = key
    await connectTarget()
  }

  /** 对话框打开：复位选中态为当前连接并加载 */
  function reset(): void {
    targetKey.value = TARGET_CURRENT
    targetDbs.value = []
    targetDb.value = ''
    targetConnId.value = null
    if (established) {
      void mysqlDisconnect(established).catch(() => undefined)
      established = null
    }
    void connectTarget()
  }

  /** 对话框关闭：断开建立的独立连接（'__current__' 直用的连接不断开） */
  function dispose(): void {
    if (established) {
      void mysqlDisconnect(established).catch(() => undefined)
      established = null
    }
  }

  return {
    store,
    targetKey,
    targetConnId,
    targetDbs,
    targetDb,
    targetLoading,
    targetItems,
    selectTarget,
    reset,
    dispose,
  }
}
