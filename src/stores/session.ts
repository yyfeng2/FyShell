import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { sessionDelete, sessionList, sessionSave, folderSave } from '@/api/session'

/**
 * 与 IPC 契约（docs/ipc-contracts.md §1）同名同构的数据模型。
 * AuthType 为可辨识联合，type 字段值为 "password" | "publicKey" | "interactive" | "noAuth" | "jump"。
 * 若 @/api/session 后续也导出同名类型，二者结构一致，可统一为单一来源。
 */
export type AuthType =
  | { type: 'password'; password: string }
  | { type: 'publicKey'; private_key_path: string; passphrase: string | null }
  | { type: 'interactive'; password: string }
  | { type: 'noAuth' }
  | { type: 'jump'; jump_session_id: string }

export interface SessionConfig {
  id: string
  name: string
  folder_id: string | null
  host: string
  port: number
  username: string
  auth_type: AuthType
  encoding: string
  color: string | null
  keepalive_interval: number
  /** 会话类型："mysql" = 数据库会话，"telnet"/"rlogin" = Telnet 兼容，"serial" = 串口；缺省/"ssh" = SSH（与后端 bindings 对齐，string 联动） */
  session_type?: string | null
  /** 串口会话：端口名（session_type 为 "serial" 时生效） */
  serial_port?: string | null
  /** 串口会话：波特率（默认 115200） */
  baud_rate?: number | null
}

export interface SessionFolder {
  id: string
  name: string
  parent_id: string | null
}

/** session_list 返回的树形节点（契约 §2：文件夹+会话混合节点） */
export interface SessionNode {
  kind: 'folder' | 'session'
  id: string
  name: string
  parent_id: string | null
  children: SessionNode[]
  config: SessionConfig | null
}

type UnknownRecord = Record<string, unknown>

/** 容错判断：节点是否为文件夹（兼容 kind / is_folder / 有无 children 三种返回形态） */
export function isFolderNode(node: SessionNode): boolean {
  const raw = node as unknown as UnknownRecord
  return node.kind === 'folder' || raw.is_folder === true || Array.isArray(raw.children)
}

/** 取节点的子节点（仅文件夹有，安全返回空数组） */
export function nodeChildren(node: SessionNode): SessionNode[] {
  const children = (node as unknown as UnknownRecord).children
  return Array.isArray(children) ? (children as SessionNode[]) : []
}

/** 取节点上的会话配置：兼容 config 挂载或节点本身即 SessionConfig（平铺）两种形态 */
export function nodeConfig(node: SessionNode): SessionConfig | null {
  const raw = node as unknown as UnknownRecord
  if (raw.config) return raw.config as SessionConfig
  return isFolderNode(node) ? null : (node as unknown as SessionConfig)
}

/** session_list 返回形态归一化：无 kind 时按有无 children 推断 */
function normalizeNode(raw: unknown): SessionNode | null {
  if (!raw || typeof raw !== 'object') return null
  const r = raw as UnknownRecord
  const id = String(r.id ?? '')
  if (!id) return null
  const kind: SessionNode['kind'] =
    r.kind === 'folder' || r.is_folder === true || Array.isArray(r.children) ? 'folder' : 'session'
  const children: SessionNode[] = []
  if (kind === 'folder' && Array.isArray(r.children)) {
    for (const child of r.children) {
      const normalized = normalizeNode(child)
      if (normalized) children.push(normalized)
    }
  }
  const config =
    kind === 'session'
      ? (r.config as SessionConfig | undefined) ?? ({ ...r } as unknown as SessionConfig)
      : null
  return {
    kind,
    id,
    name: String(r.name ?? ''),
    parent_id: (r.parent_id as string | null) ?? null,
    children,
    config,
  }
}

/** 按关键字递归过滤：文件夹保留「子树含匹配项或自身匹配」的分支 */
function matchNodes(nodes: SessionNode[], keyword: string): SessionNode[] {
  const walk = (list: SessionNode[]): SessionNode[] => {
    const out: SessionNode[] = []
    for (const node of list) {
      if (isFolderNode(node)) {
        const children = walk(nodeChildren(node))
        if (children.length > 0 || node.name.toLowerCase().includes(keyword)) {
          out.push({ ...node, children })
        }
      } else {
        const cfg = nodeConfig(node)
        const hit =
          node.name.toLowerCase().includes(keyword) ||
          (!!cfg && (cfg.host.toLowerCase().includes(keyword) || cfg.username.toLowerCase().includes(keyword)))
        if (hit) out.push(node)
      }
    }
    return out
  }
  return walk(nodes)
}

/** 星标置顶：星标会话 → 文件夹 → 普通会话，同层级按名称排序 */
function sortByStar(nodes: SessionNode[], starred: string[]): SessionNode[] {
  const rank = (n: SessionNode): number => (starred.includes(n.id) ? 0 : isFolderNode(n) ? 1 : 2)
  return [...nodes].sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name, 'zh'))
}

const STARRED_KEY = 'fyshell.starred-sessions'

function loadStarred(): string[] {
  try {
    const raw = localStorage.getItem(STARRED_KEY)
    return raw ? (JSON.parse(raw) as string[]) : []
  } catch {
    return []
  }
}

function persistStarred(ids: string[]): void {
  try {
    localStorage.setItem(STARRED_KEY, JSON.stringify(ids))
  } catch (err) {
    console.error('[session] 星标持久化失败:', err)
  }
}

function uuid(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) return crypto.randomUUID()
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0
    return (c === 'x' ? r : (r & 0x3) | 0x8).toString(16)
  })
}

export const useSessionStore = defineStore('session', () => {
  // ---------- state ----------
  const nodes = ref<SessionNode[]>([])
  const loading = ref(false)
  /** 导航过滤栏关键字（实时过滤） */
  const search = ref('')
  /** 星标会话 id（契约数据模型无此字段，暂存前端本地存储） */
  const starredIds = ref<string[]>(loadStarred())

  // ---------- getters ----------
  /** 过滤 + 星标置顶后的树，供会话树渲染 */
  const filteredNodes = computed<SessionNode[]>(() => {
    const keyword = (search.value ?? '').trim().toLowerCase()
    const matched = keyword ? matchNodes(nodes.value, keyword) : nodes.value
    return sortByStar(matched, starredIds.value)
  })

  /** 全部会话配置的平铺列表（供跳板机下拉等场景） */
  const allConfigs = computed<SessionConfig[]>(() => {
    const out: SessionConfig[] = []
    const walk = (list: SessionNode[]): void => {
      for (const node of list) {
        const cfg = nodeConfig(node)
        if (cfg) out.push(cfg)
        walk(nodeChildren(node))
      }
    }
    walk(nodes.value)
    return out
  })

  /** 按 id 取会话配置 */
  function getSessionById(id: string): SessionConfig | undefined {
    return findConfig(nodes.value, id)
  }

  function findConfig(list: SessionNode[], id: string): SessionConfig | undefined {
    for (const node of list) {
      if (node.id === id) {
        const cfg = nodeConfig(node)
        if (cfg) return cfg
      }
      const hit = findConfig(nodeChildren(node), id)
      if (hit) return hit
    }
    return undefined
  }

  // ---------- actions ----------
  /** 加载会话树（session_list） */
  async function load(): Promise<void> {
    loading.value = true
    try {
      const list = await sessionList()
      nodes.value = (list ?? []).map(normalizeNode).filter((n): n is SessionNode => n !== null)
    } catch (err) {
      console.error('[session] 加载会话树失败:', err)
    } finally {
      loading.value = false
    }
  }

  /** 保存会话（session_save），成功后刷新树 */
  async function save(config: SessionConfig): Promise<SessionConfig> {
    const saved = await sessionSave(config)
    await load()
    return saved
  }

  /** 删除会话或文件夹（session_delete），成功后刷新树 */
  async function remove(id: string, isFolder: boolean): Promise<void> {
    await sessionDelete(id, isFolder)
    await load()
  }

  /** 保存文件夹（folder_save），成功后刷新树 */
  async function saveFolder(folder: SessionFolder): Promise<SessionFolder> {
    const saved = await folderSave(folder)
    await load()
    return saved
  }

  function toggleStar(id: string): void {
    const idx = starredIds.value.indexOf(id)
    if (idx >= 0) starredIds.value.splice(idx, 1)
    else starredIds.value.push(id)
    persistStarred(starredIds.value)
  }

  function isStarred(id: string): boolean {
    return starredIds.value.includes(id)
  }

  return {
    nodes,
    loading,
    search,
    starredIds,
    filteredNodes,
    allConfigs,
    getSessionById,
    load,
    save,
    remove,
    saveFolder,
    toggleStar,
    isStarred,
  }
})
