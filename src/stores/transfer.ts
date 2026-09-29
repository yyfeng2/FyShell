/**
 * 传输队列 Pinia store
 *
 * 职责（对照 docs/ipc-contracts.md §2 SFTP 传输）：
 * - 上传/下载入队：sftpUpload/sftpDownload 的进度接收（api 层内部用 Channel
 *   包装，此处传回调实时更新队列中对应任务）
 * - transfer-status 事件监听：start()/stop() 由视图在 mounted/unmounted 调用，
 *   带引用计数，多视图共用时最后一个卸载才 unlisten
 * - transfer_cancel / transfer_clear、transfer_list 队列快照
 * - 任务内速度估算（指数滑动平均）；断点续传由后端实现，前端仅展示进度与状态
 *
 * 注意：本 store 不直接 invoke，统一走 @/api/sftp 封装层。
 */
import { defineStore } from 'pinia'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import {
  sftpUpload,
  sftpDownload,
  transferList,
  transferCancel,
  transferClear,
} from '@/api/sftp'
import type { TransferKind, TransferStatus, TransferTask } from '@/api/types'

/** 对外复用契约类型 */
export type { TransferKind, TransferStatus, TransferTask }

/** 归一化后端状态：兼容 "Queued"/"running" 等大小写差异 */
function normalizeStatus(raw: unknown): TransferStatus {
  const s = String(raw ?? '').toLowerCase()
  switch (s) {
    case 'running':
      return 'Running'
    case 'completed':
      return 'Completed'
    case 'failed':
      return 'Failed'
    case 'cancelled':
      return 'Cancelled'
    default:
      return 'Queued'
  }
}

/** 归一化传输方向 */
function normalizeKind(raw: unknown): TransferKind {
  return String(raw ?? '').toLowerCase() === 'download' ? 'Download' : 'Upload'
}

/** 任意来源的任务数据 → TransferTask（容忍字段缺失/大小写差异） */
function normalizeTask(raw: unknown): TransferTask {
  const t = (raw ?? {}) as Record<string, unknown>
  return {
    id: String(t.id ?? ''),
    session_id: String(t.session_id ?? ''),
    kind: normalizeKind(t.kind),
    local_path: String(t.local_path ?? ''),
    remote_path: String(t.remote_path ?? ''),
    total_bytes: Number(t.total_bytes ?? 0),
    transferred_bytes: Number(t.transferred_bytes ?? 0),
    status: normalizeStatus(t.status),
    error: t.error == null ? null : String(t.error),
  }
}

function isDone(status: TransferStatus): boolean {
  return status === 'Completed' || status === 'Failed' || status === 'Cancelled'
}

function isActive(status: TransferStatus): boolean {
  return status === 'Queued' || status === 'Running'
}

/** 任务完成通知方（批量合并后刷新对侧文件列表等），返回注销函数 */
const doneListeners: ((task: TransferTask) => void)[] = []

export function onSessionTransfersDone(fn: (task: TransferTask) => void): () => void {
  doneListeners.push(fn)
  return () => {
    const i = doneListeners.indexOf(fn)
    if (i >= 0) doneListeners.splice(i, 1)
  }
}

/** 速度采样记录：上次进度字节数 + 时间戳 + 平滑后的速度 */
interface SpeedSample {
  bytes: number
  time: number
  speed: number // 字节/秒
}

export const useTransferStore = defineStore('transfer', {
  state: () => ({
    tasks: [] as TransferTask[],
    samples: {} as Record<string, SpeedSample>,
    unlisten: null as UnlistenFn | null,
    /** 引用计数：多视图同时监听时按计数启停 */
    listenerCount: 0,
    /** listen await 在途标志：防 start() 重入（unlisten 未赋值前第二个 start 会复用 await） */
    starting: false,
  }),

  getters: {
    /** 进行中（含排队）的任务数，可用于角标 */
    activeCount: (state) => state.tasks.filter((t) => isActive(t.status)).length,
    /** 是否存在可清除的已完成/失败/取消记录 */
    hasFinished: (state) => state.tasks.some((t) => isDone(t.status)),
  },

  actions: {
    /** 按任务 id 更新或插入（进度回调与 transfer-status 事件共用入口） */
    upsert(raw: unknown) {
      const task = normalizeTask(raw)
      if (!task.id) return
      const idx = this.tasks.findIndex((t) => t.id === task.id)
      if (idx >= 0) {
        this.tasks[idx] = task
      } else {
        this.tasks.push(task)
      }
      this.sampleSpeed(task)

      // 任务完成且该会话无剩余活跃任务 → 通知（批量合并，供视图刷新对侧列表）
      if (isDone(task.status)) {
        const remaining = this.tasks.some((t) => t.session_id === task.session_id && isActive(t.status))
        if (!remaining) {
          for (const fn of doneListeners) {
            try {
              fn(task)
            } catch {
              /* 通知方异常不影响队列 */
            }
          }
        }
      }
    },

    /** 增量速度估算（EWMA 平滑，避免速度跳变） */
    sampleSpeed(task: TransferTask) {
      const now = Date.now()
      const prev = this.samples[task.id]
      let speed = prev?.speed ?? 0
      if (prev && now > prev.time && task.transferred_bytes >= prev.bytes) {
        const instant = ((task.transferred_bytes - prev.bytes) * 1000) / (now - prev.time)
        speed = speed > 0 ? speed * 0.7 + instant * 0.3 : instant
      }
      this.samples[task.id] = { bytes: task.transferred_bytes, time: now, speed }
    },

    /** 某任务当前估算速度（字节/秒） */
    speedOf(taskId: string): number {
      return this.samples[taskId]?.speed ?? 0
    },

    /** 拉取后端队列快照 */
    async refresh(): Promise<void> {
      const list = (await transferList()) as unknown[]
      this.tasks = (list ?? []).map(normalizeTask)
    },

    /** 入队上传（进度经 api 层 Channel 实时回调 upsert） */
    async enqueueUpload(sessionId: string, localPath: string, remotePath: string): Promise<void> {
      // 入队返回后端生成的实时任务快照（含真实任务 id）立即入列；
      // Channel 回调用 upsert 继续驱动进度；随后再拉一次权威快照对齐后端终态
      //（实时推送偶发不可达时的兜底——小文件可能瞬间完成，队列 Tab 未打开时
      //  事件/Channel 有丢失风险，主动 refresh 保证 UI 立即反映真实状态）
      const task = await sftpUpload(sessionId, localPath, remotePath, (t) => this.upsert(t))
      this.upsert(task)
      await this.refresh()
    },

    /** 入队下载（同上传） */
    async enqueueDownload(
      sessionId: string,
      remotePath: string,
      localPath: string,
    ): Promise<void> {
      const task = await sftpDownload(sessionId, remotePath, localPath, (t) => this.upsert(t))
      this.upsert(task)
      await this.refresh()
    },

    /** 取消任务（乐观更新状态，最终以后端广播为准） */
    async cancel(taskId: string): Promise<void> {
      const task = this.tasks.find((t) => t.id === taskId)
      if (task && isActive(task.status)) {
        task.status = 'Cancelled'
      }
      await transferCancel(taskId)
      await this.refresh()
    },

    /** 清除已完成/失败/取消记录 */
    async clear(): Promise<void> {
      await transferClear()
      this.tasks = this.tasks.filter((t) => !isDone(t.status))
    },

    /** 开始监听 transfer-status（幂等，引用计数） */
    async start(): Promise<void> {
      this.listenerCount++
      if (this.unlisten || this.starting) return
      this.starting = true
      try {
        this.unlisten = await listen<unknown>('transfer-status', (event) => {
          const payload = event.payload as { task?: unknown } | undefined
          if (payload?.task) this.upsert(payload.task)
        })
      } finally {
        this.starting = false
      }
      // await listen 期间若已被 stop() 把计数归零（unlisten 尚未建立、stop 无事可反注册），
      // 需在此补一次注销，否则监听器永久泄漏（此后计数为 0 且无人再调用 stop）
      if (this.listenerCount === 0) {
        this.unlisten()
        this.unlisten = null
      }
    },

    /** 停止监听：组件卸载时调用，计数归零才真正 unlisten */
    stop(): void {
      if (this.listenerCount > 0) this.listenerCount--
      if (this.listenerCount === 0 && this.unlisten) {
        this.unlisten()
        this.unlisten = null
      }
    },
  },
})
