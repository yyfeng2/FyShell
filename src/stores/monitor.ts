/**
 * 服务器监控 Pinia store（契约第 5 节：commands/monitor.rs ↔ api/monitor.ts）
 *
 * 职责：
 * - startMonitor(sessionId, intervalSecs)：monitor_start 的 Channel 进度接收；
 *   样本写入非响应式环形缓冲（模块级 Map/数组，不经 Pinia 深层代理），
 *   仅 CPU/内存/网络聚合值暴露为响应式 ref，保证高频路径不卡
 * - stopMonitor(sessionId)：带引用计数，最后一个卸载的组件才真正调 monitor_stop
 * - samplesOf(sessionId)：最近 N 个样本快照（供 sparkline 趋势图）
 * - docker_list 加载与 docker_operate 操作
 *
 * 注意：本 store 不直接 invoke，统一走 @/api/monitor 封装层。
 */
import { ref } from 'vue'
import { defineStore } from 'pinia'
import { monitorStart, monitorStop, dockerList, dockerOperate } from '@/api/monitor'
import type { MonitorSample, DockerContainer } from '@/api/types'

/** 对外复用契约类型（与 api/types.ts 同名同构） */
export type { MonitorSample, DockerContainer }

/** 暴露给组件的最新聚合值（不携带原始样本） */
export interface MonitorStats {
  cpuPercent: number
  memPercent: number
  memUsedMb: number
  memTotalMb: number
  netRxRate: number // KB/s（累计值差分）
  netTxRate: number // KB/s
  updatedAt: number // 最近一次采样时间戳，兼作 sparkline 的响应式 tick
}

/** 环形缓冲上限：120 个样本（1s 间隔约 2 分钟趋势） */
const MAX_SAMPLES = 120

/** 非响应式环形缓冲：会话 id → 原始样本队列（高频路径，不进响应式） */
const buffers = new Map<string, MonitorSample[]>()

/** 引用计数：多组件共用同一会话监控时，最后一个卸载才 monitor_stop */
const refCounts = new Map<string, number>()

/** 采集运行状态（非响应式） */
interface ChannelState {
  intervalSecs: number
  lastSample: MonitorSample | null
  lastTime: number
}

/** 非响应式采集状态表：会话 id → 运行状态 */
const running = new Map<string, ChannelState>()

/** 归一化 Docker 容器（容忍字段缺失） */
function normalizeContainer(raw: unknown): DockerContainer {
  const c = (raw ?? {}) as Record<string, unknown>
  return {
    id: String(c.id ?? ''),
    names: String(c.names ?? ''),
    image: String(c.image ?? ''),
    state: String(c.state ?? ''),
    status: String(c.status ?? ''),
  }
}

export const useMonitorStore = defineStore('monitor', () => {
  /** 采样间隔（秒），全局 UI 偏好；变更时对正在采集的会话重启 */
  const intervalSecs = ref(3)

  /** 各会话最新聚合值：仅聚合值走响应式，样本存非响应式环形缓冲 */
  const latest = ref<Record<string, MonitorStats>>({})

  /** Docker 容器列表：会话 id → 容器数组 */
  const containers = ref<Record<string, DockerContainer[]>>({})
  const dockerLoading = ref(false)
  const dockerError = ref<string | null>(null)
  /** docker 操作进行中标记：container id → action */
  const operating = ref<Record<string, string>>({})
  /** 最近一次监控启动/采集错误信息 */
  const monitorError = ref<string | null>(null)

  /** 样本到达：更新环形缓冲与聚合值（Channel 回调入口） */
  function onSample(sessionId: string, raw: MonitorSample): void {
    const st = running.get(sessionId)
    if (!st) return
    const now = Date.now()

    // 环形缓冲：超过上限丢弃最旧样本
    const buf = buffers.get(sessionId) ?? []
    buf.push(raw)
    if (buf.length > MAX_SAMPLES) {
      buf.splice(0, buf.length - MAX_SAMPLES)
    }
    buffers.set(sessionId, buf)

    // 网络速率 = 累计值差分 / 实际间隔秒（计数器回绕取 0）
    let rxRate = 0
    let txRate = 0
    if (st.lastSample && now > st.lastTime) {
      const dt = (now - st.lastTime) / 1000
      rxRate = Math.max(0, (raw.net_rx_kb - st.lastSample.net_rx_kb) / dt)
      txRate = Math.max(0, (raw.net_tx_kb - st.lastSample.net_tx_kb) / dt)
    }
    st.lastSample = raw
    st.lastTime = now

    // 仅聚合值进响应式（整体替换触发依赖更新）
    latest.value = {
      ...latest.value,
      [sessionId]: {
        cpuPercent: raw.cpu_percent,
        memPercent: raw.mem_total_mb > 0 ? (raw.mem_used_mb / raw.mem_total_mb) * 100 : 0,
        memUsedMb: raw.mem_used_mb,
        memTotalMb: raw.mem_total_mb,
        netRxRate: rxRate,
        netTxRate: txRate,
        updatedAt: Date.now(),
      },
    }
  }

  /** 开始采集（引用计数；已在采集时仅在间隔变化时重启） */
  async function startMonitor(sessionId: string, interval?: number): Promise<void> {
    if (!sessionId) return
    const secs = Math.min(10, Math.max(1, Math.round(interval ?? intervalSecs.value)))
    refCounts.set(sessionId, (refCounts.get(sessionId) ?? 0) + 1)
    const st = running.get(sessionId)
    if (st) {
      if (st.intervalSecs !== secs) {
        st.intervalSecs = secs
        await restartMonitor(sessionId)
      }
      return
    }
    intervalSecs.value = secs
    await beginChannel(sessionId, secs)
  }

  /** 启动一个采集 Channel（内部先幂等 stop，避免后端重复采集） */
  async function beginChannel(sessionId: string, secs: number): Promise<void> {
    try {
      await monitorStop(sessionId)
    } catch {
      // 忽略停止失败（可能本来就未启动）
    }
    running.set(sessionId, { intervalSecs: secs, lastSample: null, lastTime: 0 })
    monitorError.value = null
    try {
      await monitorStart(sessionId, secs, (sample) => onSample(sessionId, sample))
    } catch (e) {
      running.delete(sessionId)
      buffers.delete(sessionId)
      monitorError.value = e instanceof Error ? e.message : String(e)
    }
  }

  /** 以新间隔重启采集（保留引用计数） */
  async function restartMonitor(sessionId: string): Promise<void> {
    const st = running.get(sessionId)
    const secs = st?.intervalSecs ?? intervalSecs.value
    running.delete(sessionId)
    await beginChannel(sessionId, secs)
  }

  /** 停止采集：引用计数归零才真正 monitor_stop，并清理聚合值与缓冲 */
  function stopMonitor(sessionId: string | null): void {
    if (!sessionId) return
    const count = refCounts.get(sessionId) ?? 0
    if (count > 1) {
      refCounts.set(sessionId, count - 1)
      return
    }
    refCounts.delete(sessionId)
    if (running.has(sessionId)) {
      running.delete(sessionId)
      void monitorStop(sessionId).catch(() => {
        // 后端已停止或会话已关闭，忽略
      })
    }
    if (latest.value[sessionId]) {
      const next = { ...latest.value }
      delete next[sessionId]
      latest.value = next
    }
    buffers.delete(sessionId)
  }

  /** 某会话最近 N 个原始样本快照（供 sparkline；非响应式） */
  function samplesOf(sessionId: string | null): MonitorSample[] {
    return (sessionId ? buffers.get(sessionId) : undefined)?.slice() ?? []
  }

  /** 某会话最新聚合值（供迷你条） */
  function statsOf(sessionId: string | null): MonitorStats | null {
    return (sessionId ? latest.value[sessionId] : undefined) ?? null
  }

  /** 设置采样间隔（1/3/5/10 秒），正在采集的会话自动重启 */
  async function setIntervalSecs(secs: number): Promise<void> {
    const value = Math.min(10, Math.max(1, Math.round(Number(secs) || 3)))
    if (intervalSecs.value === value) return
    intervalSecs.value = value
    for (const sid of Array.from(refCounts.keys())) {
      if ((refCounts.get(sid) ?? 0) > 0 && running.has(sid)) {
        await restartMonitor(sid)
      }
    }
  }

  /** 拉取 Docker 容器列表 */
  async function loadDocker(sessionId: string): Promise<void> {
    if (!sessionId) return
    dockerLoading.value = true
    dockerError.value = null
    try {
      const list = (await dockerList(sessionId)) as unknown[]
      containers.value = {
        ...containers.value,
        [sessionId]: (list ?? []).map(normalizeContainer),
      }
    } catch (e) {
      dockerError.value = e instanceof Error ? e.message : String(e)
    } finally {
      dockerLoading.value = false
    }
  }

  /** Docker 容器操作（action: start/stop/restart），完成后刷新列表 */
  async function operateDocker(
    sessionId: string,
    containerId: string,
    action: 'start' | 'stop' | 'restart',
  ): Promise<void> {
    if (!sessionId || !containerId) return
    operating.value = { ...operating.value, [containerId]: action }
    try {
      await dockerOperate(sessionId, containerId, action)
      await loadDocker(sessionId)
    } catch (e) {
      dockerError.value = e instanceof Error ? e.message : String(e)
    } finally {
      const next = { ...operating.value }
      delete next[containerId]
      operating.value = next
    }
  }

  return {
    intervalSecs,
    latest,
    containers,
    dockerLoading,
    dockerError,
    operating,
    error: monitorError,
    startMonitor,
    stopMonitor,
    samplesOf,
    statsOf,
    setIntervalSecs,
    loadDocker,
    operateDocker,
  }
})
