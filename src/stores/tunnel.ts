/**
 * SSH 隧道规则 Pinia store（契约 §5.2：commands/tunnel.rs ↔ api/tunnel.ts）
 *
 * 职责（对照 docs/ipc-contracts.md §5.1 TunnelRule / §5.2 SSH 隧道）：
 * - 规则列表加载（tunnel_list 快照）、保存（tunnel_save）、删除并停止（tunnel_delete）
 * - 启动 / 停止监听（tunnel_start / tunnel_stop）
 * - 按会话过滤：契约的 session_id 为 Option（不传即全量），此处不透传参数，
 *   由 filteredRules getter 在前端本地过滤，避免与 api 层签名强耦合
 *
 * 隧道转发逻辑在后端实现，本 store 只管理规则与状态展示。
 * 注意：本 store 不直接 invoke，统一走 @/api/tunnel 封装层。
 */
import { defineStore } from 'pinia'
import { tunnelDelete, tunnelList, tunnelSave, tunnelStart, tunnelStop } from '@/api/tunnel'
import type { TunnelKind, TunnelRule, TunnelStatus } from '@/api/types'

/** 对外复用契约类型（单一事实来源在 @/api/types） */
export type { TunnelKind, TunnelStatus, TunnelRule }

/** 归一化类型：兼容 "local"/"Local" 等大小写差异 */
function normalizeKind(raw: unknown): TunnelKind {
  const k = String(raw ?? '').toLowerCase()
  if (k === 'remote') return 'Remote'
  if (k === 'socks') return 'Socks'
  return 'Local'
}

/** 归一化状态：兼容 "stopped"/"listening" 等大小写差异 */
function normalizeStatus(raw: unknown): TunnelStatus {
  const s = String(raw ?? '').toLowerCase()
  if (s === 'listening') return 'Listening'
  if (s === 'error') return 'Error'
  return 'Stopped'
}

/** 任意来源的规则数据 → TunnelRule（容忍字段缺失/大小写差异） */
function normalizeRule(raw: unknown): TunnelRule {
  const r = (raw ?? {}) as Record<string, unknown>
  return {
    id: String(r.id ?? ''),
    session_id: String(r.session_id ?? ''),
    kind: normalizeKind(r.kind),
    listen_host: String(r.listen_host ?? '127.0.0.1'),
    listen_port: Number(r.listen_port ?? 0),
    target_host: String(r.target_host ?? ''),
    target_port: Number(r.target_port ?? 0),
    enabled: Boolean(r.enabled),
    status: normalizeStatus(r.status),
    error: r.error == null ? null : String(r.error),
  }
}

export const useTunnelStore = defineStore('tunnel', {
  state: () => ({
    rules: [] as TunnelRule[],
    /** 全量加载中标志（v-loading 类提示用） */
    loading: false,
    /** 按会话过滤：null = 全部会话 */
    filterSessionId: null as string | null,
  }),

  getters: {
    /** 当前过滤后的规则列表（filterSessionId 为 null 时返回全量） */
    filteredRules: (state): TunnelRule[] => {
      if (!state.filterSessionId) return state.rules
      return state.rules.filter((r) => r.session_id === state.filterSessionId)
    },
    /** 监听中的规则数，可用于角标 */
    listeningCount: (state): number =>
      state.rules.filter((r) => r.status === 'Listening').length,
  },

  actions: {
    /** 拉取后端规则快照（含最新 status/error） */
    async refresh(): Promise<void> {
      const list = (await tunnelList()) as unknown[]
      this.rules = (list ?? []).map(normalizeRule)
    },

    /**
     * 新建/编辑规则；新规则传 id 为空字符串，Rust 侧生成 id。
     * 保存后拉取快照以同步后端归一化后的状态字段
     */
    async save(rule: TunnelRule): Promise<TunnelRule> {
      const saved = (await tunnelSave(rule)) as unknown
      await this.refresh()
      // 返回后端归一化后的规则（可能带新 id）
      return (saved as Record<string, unknown>)?.id
        ? normalizeRule(saved)
        : this.rules.find((r) => r.id === rule.id) ?? normalizeRule(saved)
    },

    /** 删除规则（后端同步停止监听） */
    async remove(id: string): Promise<void> {
      await tunnelDelete(id)
      this.rules = this.rules.filter((r) => r.id !== id)
      // 拉一次快照兜底（后端可能在删除前有状态变更）
      try {
        await this.refresh()
      } catch {
        /* 快照失败不影响删除结果 */
      }
    },

    /** 启动监听；失败时以后端快照中的 error 为准 */
    async start(id: string): Promise<void> {
      await tunnelStart(id)
      await this.refresh()
    },

    /** 停止监听 */
    async stop(id: string): Promise<void> {
      await tunnelStop(id)
      await this.refresh()
    },

    /** 设置会话过滤（null=全部） */
    setFilter(sessionId: string | null): void {
      this.filterSessionId = sessionId
    },
  },
})
