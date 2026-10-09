<script setup lang="ts">
/**
 * WorkspaceView —— 应用主工作区
 *
 * 整体布局（参考 Xshell，架构 §3.1）：
 * - 左导航树（可停靠 / 自动隐藏）+ 右多 Tab 工作区 + 底部状态栏
 * - Tab 按连接着色（会话 color 字段）
 * - 工作区按 Tab 类型渲染：终端 Tab 渲染终端区、传输 Tab 渲染传输队列视图
 *
 * 全局快捷键（可修改，settings 快捷键组）：新标签/关闭/切换默认 Ctrl+T/W/Tab、Alt+1~9 直达
 */
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import FlexTabs, { type FlexTabItem } from '@/components/common/FlexTabs.vue'
import EmptyState from '@/components/common/EmptyState.vue'
import StatusBar from '@/components/common/StatusBar.vue'
import GlobalDialog from '@/components/common/GlobalDialog.vue'
import MenuBar from '@/components/common/MenuBar.vue'
import MasterPasswordDialog from '@/components/common/MasterPasswordDialog.vue'
import ShortcutListDialog from '@/components/common/ShortcutListDialog.vue'
import SettingsDialog from '@/components/common/SettingsDialog.vue'
import ColorSchemeDialog from '@/components/common/ColorSchemeDialog.vue'
import SshOptionsDialog from '@/components/ssh/options/SshOptionsDialog.vue'
import ToolBar from '@/components/common/ToolBar.vue'
import AddressBar from '@/components/common/AddressBar.vue'
import QuickCommandBar from '@/components/common/QuickCommandBar.vue'
import SessionForm from '@/components/ssh/session/SessionForm.vue'
import HostkeyDialog from '@/components/ssh/terminal/HostkeyDialog.vue'
import RzszDialog from '@/components/ssh/terminal/RzszDialog.vue'
import TerminalPane from '@/components/ssh/terminal/TerminalPane.vue'
import ComposeBar from '@/components/ssh/terminal/ComposeBar.vue'
import DualPane from '@/components/sftp/DualPane.vue'
import TransferQueueView from '@/views/transfer/TransferQueueView.vue'
import TunnelView from '@/views/tunnel/TunnelView.vue'
import ComposePane from '@/components/common/quickcommand/ComposePane.vue'
import LogViewer from '@/components/ssh/log/LogViewer.vue'
import MysqlDbWorkspace from '@/components/mysql/MysqlDbWorkspace.vue'
import ImportExportDialog from '@/components/mysql/ImportExportDialog.vue'
import DataTransferDialog from '@/components/mysql/DataTransferDialog.vue'
import DataGenerateDialog from '@/components/mysql/DataGenerateDialog.vue'
import DbSyncDialog from '@/components/mysql/DbSyncDialog.vue'
import StructureSyncDialog from '@/components/mysql/StructureSyncDialog.vue'
import EditDatabaseDialog from '@/components/mysql/EditDatabaseDialog.vue'
import NewDatabaseDialog from '@/components/mysql/NewDatabaseDialog.vue'
import ErModelDialog from '@/components/mysql/ErModelDialog.vue'
import FindInDbDialog from '@/components/mysql/FindInDbDialog.vue'
import MysqlCliConsole from '@/components/mysql/MysqlCliConsole.vue'
import MysqlQueryTab from '@/components/mysql/MysqlQueryTab.vue'
import RedisDbWorkspace from '@/components/redis/RedisDbWorkspace.vue'
import SessionListDialog from '@/components/ssh/session/SessionListDialog.vue'
import { useMysqlStore } from '@/stores/mysql'
import { friendlyError } from '@/utils/errors'
import { useRedisStore } from '@/stores/redis'
import { mysqlDbList, mysqlDbDrop } from '@/api/mysqlDb'
import { mysqlListTables, mysqlQuery } from '@/api/mysql'
import { sessionList, sessionClone, sessionReorder, type NodeReorderItem } from '@/api/session'
import { transferList } from '@/api/sftp'
import { useUiStore } from '@/stores/ui'
import { useSettingsStore, type FontFamilyStyle } from '@/stores/settings'
import { useSessionStore, type SessionConfig } from '@/stores/session'
import type { RedisConnection, MySqlTableInfo, MySqlQueryResult } from '@/api/types'
import { useSshOptionsStore } from '@/stores/sshOptions'
import { useTerminalStore } from '@/stores/terminal'
import { debugLog } from '@/api/channels'
import { useDragOutWindow } from '@/composables/useDragOutWindow'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { check_update, install_update } from '@/api/updater'

const ui = useUiStore()
const sessionStore = useSessionStore()
/** 高功能设置（主题模式/终端项/SFTP 目录），持久化到后端 SQLite settings 表 */
const settings = useSettingsStore()
/** SSH 会话级选项（默认标签色等设置兜底，见 openTerminal） */
const sshOpts = useSshOptionsStore()
/** 终端连接管理：openTerminal/closeBySessionId 由 store 统一管理连接生命周期 */
const terminalStore = useTerminalStore()
/** 标签拖出新窗口（P1）：创建 WebviewWindow 展示会话终端 */
const { openSessionWindow } = useDragOutWindow()

// ---------------- 数据模型 ----------------

/** session_list 返回的树形节点（文件夹 + 会话混合节点，字段按契约容错处理） */
interface SessionNode {
  id: string
  name: string
  is_folder: boolean
  children?: SessionNode[]
  config?: {
    color?: string | null
    host?: string
    port?: number
    username?: string
    encoding?: string
    session_type?: string | null
  } & Record<string, unknown>
  /** 手工排序序号（后端持久化：0=未手工排序按名称兜底；拖拽改序后按它重排展示） */
  sort_order?: number
}

/** 工作区 Tab（终端 / 传输 / SFTP 双栏 / 隧道 / 快捷命令 / MySQL / Redis / 命令列界面 / 查询九类） */
interface WorkTab {
  id: string
  type: 'terminal' | 'transfer' | 'tunnel' | 'sftp' | 'compose' | 'mysql' | 'redis' | 'mysqlcli' | 'query'
  title: string
  /** 终端 Tab 对应的会话配置 ID（状态栏主机/编码、SFTP 配置查询） */
  sessionId?: string
  /** 连接路由键（Xshell 多标签：每标签一条独立连接，输出/输入/resize 按它路由） */
  connId?: string
  color?: string | null
}

/** 左导航扁平化节点（带层级深度） */
interface FlatNode {
  id: string
  name: string
  depth: number
  isFolder: boolean
  color: string | null
  /** 视觉展开态（搜索强制展开时也为 true） */
  isOpen: boolean
  /** 会话副行信息：user@host:port（Xshell 会话树风格；文件夹为空） */
  hostLabel: string
  /** 会话编码（副行展示用） */
  encoding: string | null
  /** 数据库会话（session_type === 'mysql' 或 'redis'，树图标与连接路由区分） */
  isMysql: boolean
  /** 会话类型（config.session_type，"ssh" 缺省；mysql/telnet/rlogin/serial 用于树图标与路由） */
  sessionType?: string | null
  /** 数据库子节点（已连接 MySQL 会话的库，Navicat 风格） */
  isDbLeaf?: boolean
  /** 分区头（SSH 服务 / 数据库服务，可收缩） */
  isSection?: boolean
  /** 分区虚线分隔 */
  isSeparator?: boolean
  /** 已保存连接常驻节点（数据库服务分区，未连接也显示主机名） */
  isSavedConn?: boolean
  /** 已保存连接节点对应的 savedConnections.id */
  savedConnId?: string
  /** 库名（isDbLeaf 节点的目标库，切换时直接使用，免解析 id 前缀） */
  dbName?: string
  /** 数据库 host 节点（已存连接/独立节点/已连接会话）：库列表已挂载（已连接态） */
  hasDbChildren?: boolean
  /** 历史库（断开连接后保留展示，灰色弱化） */
  grey?: boolean
  /** 会话在线状态（SSH/sftp/byte-stream 会话：反查活动 Tab 连接状态；数据库/文件夹/分区头不填） */
  status?: 'connecting' | 'connected' | 'offline'
}

// ---------------- 会话树 ----------------

const nodes = ref<SessionNode[]>([])
const keyword = ref('')
const expanded = ref(new Set<string>())

/** 已连接 MySQL 会话的数据库子节点（Navicat 风格：库作为会话子节点展示）。
    sessionId 为 null 表示树中无 MySQL 会话，库列表挂到数据库服务分区的独立节点下
    （host 为连接主机，作为独立节点显示名）；savedId 为当前活动连接对应的已保存连接 id
    （已保存连接常驻节点优先挂载）；connected=false（断开）时保留历史库以灰色展示 */
const mysqlTreeDbs = ref<{
  sessionId: string | null
  host: string
  databases: string[]
  savedId: string | null
  connected: boolean
} | null>(null)

/** 导航分区展开态（SSH 服务 / 数据库服务，单击分区头收缩） */
const openSections = ref(new Set(['section-ssh', 'section-db', 'section-redis']))

// 连接状态变化：任意入口（会话树/工作台）连接成功后，把数据库子节点挂到
// 匹配的会话节点（按 host 匹配，MySQL 类型与用户名一致者优先）；断开时清空。
// 切库会产生新 conn_id（非 null），库列表随之自动刷新，不会误清
/** 加载当前连接的库列表并挂载到会话树（连接状态变化与树加载后均调用）。
    库列表挂 MySQL 会话节点下；树中无 MySQL 会话时挂数据库服务分区的独立节点 */
async function refreshMysqlTreeDbs(): Promise<void> {
  const store = useMysqlStore()
  const id = store.connId
  if (!id) {
    // 断开不清空：保留历史库以灰色展示（Navicat 风格）
    if (mysqlTreeDbs.value) mysqlTreeDbs.value.connected = false
    return
  }
  try {
    const result = await mysqlDbList(id)
    // 全部库显示（含系统库，对齐 Navicat）
    const databases = result.databases
    // 挂靠目标：与当前连接 host 一致的 MySQL 会话节点（host 相同的 SSH 会话不可作为挂靠点，
    // 否则库列表挂在数据库区不渲染的 SSH 会话上导致分区空白）
    const cfg = store.lastConfig
    const candidates = nodes.value.filter(
      (n) =>
        !n.is_folder &&
        n.config?.session_type === 'mysql' &&
        n.config.host &&
        cfg &&
        n.config.host === cfg.host,
    )
    const match =
      candidates.sort((a, b) => scoreOf(b, cfg) - scoreOf(a, cfg))[0] ??
      nodes.value.find((n) => !n.is_folder && n.config?.session_type === 'mysql')
    // 当前活动连接来自已保存连接（工作台入口）时优先挂到对应常驻节点
    const activeSavedId = store.activeSavedId
    const savedId =
      activeSavedId && store.savedConnections.some((c) => c.id === activeSavedId)
        ? activeSavedId
        : null
    mysqlTreeDbs.value = {
      sessionId: match?.id ?? null,
      host: result.host,
      databases,
      savedId,
      connected: true,
    }
    // 持久化最近一次成功的库列表：重启应用后未连接状态下仍以灰色展示历史库（Navicat 风格）
    localStorage.setItem('mysql_tree_history', JSON.stringify({ host: result.host, databases }))
    debugLog(
      `[mysql-tree] conn=${id} databases=${databases.length} saved=${savedId ?? '-'} target=${match?.id ?? 'standalone'}`,
    )
    if (match) {
      expanded.value = new Set([...expanded.value, match.id])
    }
  } catch {
    if (mysqlTreeDbs.value) mysqlTreeDbs.value.connected = false
  }
}

// 连接状态变化：任意入口（会话树/工作台）连接成功后刷新树中库节点；断开时保留历史库（灰色）。
// 切库会产生新 conn_id（非 null），库列表随之自动刷新，不会误清
/** 库节点「关闭数据库」收起标记（内存态，Navicat 关闭库语义）：收起后库叶子灰色展示，
    重连成功后自动恢复 */
const closedDbNames = ref(new Set<string>())

watch(
  () => useMysqlStore().connId,
  (id) => {
    if (!id) {
      if (mysqlTreeDbs.value) mysqlTreeDbs.value.connected = false
      return
    }
    // 重连后恢复全部关闭的库（收起标记仅在连接存活期间有意义）
    closedDbNames.value = new Set()
    void refreshMysqlTreeDbs()
  },
)

/** 挂靠优先级：MySQL 类型会话 +2，用户名一致 +1 */
function scoreOf(node: SessionNode, cfg: { username?: string } | null): number {
  return (
    (node.config?.session_type === 'mysql' ? 2 : 0) +
    (node.config?.username && cfg?.username === node.config.username ? 1 : 0)
  )
}

async function loadTree(): Promise<void> {
  try {
    const tree = await sessionList()
    nodes.value = normalizeTreeNodes(tree)
    restoreMysqlTreeHistory()
    // 树就绪后刷新库节点挂载（连接已建立时重新匹配会话节点）
    const mysqlStore = useMysqlStore()
    if (mysqlStore.connId) void refreshMysqlTreeDbs()
  } catch (e) {
    ui.toast(`加载会话列表失败：${friendlyError(e)}`, 'error')
  }
}

/** 启动恢复历史库（localStorage）：未连接状态下树中仍以灰色展示上次连接的库列表（Navicat 风格）。
    挂靠分流（三选一，避免重复展示）：
    a) 同 host MySQL 会话节点 → 挂会话节点（sessionId）；
    b) 无会话节点但已存连接同 host → 挂已存连接常驻节点（savedId）；
    c) 都无 → 双 null 走独立节点兜底分支 */
function restoreMysqlTreeHistory(): void {
  if (useMysqlStore().connId || mysqlTreeDbs.value) return
  const raw = localStorage.getItem('mysql_tree_history')
  if (!raw) return
  let history: { host: string; databases: string[] }
  try {
    history = JSON.parse(raw)
  } catch {
    return
  }
  if (!history?.host || !Array.isArray(history.databases)) return
  // a) 同 host MySQL 会话节点优先（与 refreshMysqlTreeDbs 的挂靠规则一致）
  const sessionMatch = nodes.value.find(
    (n) => !n.is_folder && n.config?.session_type === 'mysql' && n.config.host === history.host,
  )
  // b) 无会话节点时落到已存连接常驻节点（savedconn 分支按 host 匹配展示历史库）
  const savedConn = useMysqlStore().savedConnections.find((c) => c.host === history.host) ?? null
  mysqlTreeDbs.value = {
    sessionId: sessionMatch?.id ?? null,
    host: history.host,
    databases: history.databases,
    savedId: savedConn?.id ?? null,
    connected: false,
  }
}

/** 文件夹 id → 父文件夹 id（组树时记录）：重命名文件夹时保留原父级，
    避免「重命名 = 变相新建 + 移到根级」的错误行为。 */
const folderParents = new Map<string, string | null>()

/** 会话树归一化 + 组树：wire 为扁平全量列表——文件夹节点带 parent_id、
    会话节点带 folder_id（bindings SessionNode 契约）——必须按父子关系组装嵌套树，
    否则文件夹层级永远平铺（「文件夹功能没生效」根因）。
    孤儿节点（父不存在）按根级挂载，避免节点丢失。
    同时兼容「config 挂载」与「节点本身即 SessionConfig（Rust enum 平铺序列化）」
    两种 wire 形态——不归一化时 config 恒为 undefined，host 匹配/连接路由/图标全部失效 */
function normalizeTreeNodes(raw: unknown): SessionNode[] {
  folderParents.clear()
  if (!Array.isArray(raw)) return []
  const nodes: SessionNode[] = []
  const byId = new Map<string, SessionNode>()
  for (const item of raw) {
    if (!item || typeof item !== 'object') continue
    const r = item as Record<string, unknown>
    const id = String(r.id ?? '')
    if (!id) continue
    const isFolder =
      r.kind === 'folder' || r.kind === 'Folder' || r.is_folder === true || Array.isArray(r.children)
    if (isFolder) {
      const node: SessionNode = {
        id,
        name: String(r.name ?? ''),
        is_folder: true,
        children: [],
        // 排序号：0=未手工排序（列表仅一次拖拽即可进入受管排序）
        sort_order: Number(r.sort_order) || 0,
      }
      nodes.push(node)
      byId.set(id, node)
      folderParents.set(id, (r.parent_id as string | null) ?? null)
    } else {
      // 会话节点：config 挂载优先，缺失时节点本身即配置（平铺）
      const cfg = (r.config ?? r) as Record<string, unknown>
      const node: SessionNode = {
        id,
        name: String(r.name ?? ''),
        is_folder: false,
        config: cfg as SessionNode['config'],
        sort_order: Number(cfg.sort_order) || 0,
      }
      nodes.push(node)
      byId.set(id, node)
    }
  }
  // 二次遍历按父子关系挂载：文件夹挂父文件夹（parent_id）、会话挂所属文件夹（folder_id）
  const roots: SessionNode[] = []
  for (const item of raw) {
    if (!item || typeof item !== 'object') continue
    const r = item as Record<string, unknown>
    const id = String(r.id ?? '')
    if (!id) continue
    const node = byId.get(id)
    if (!node) continue
    const parentId = (node.is_folder ? r.parent_id : r.folder_id) as string | null
    const parent = parentId ? byId.get(parentId) : undefined
    if (parent && parent.is_folder) {
      parent.children!.push(node)
    } else {
      roots.push(node)
    }
  }
  return roots
}

/** 搜索过滤：保留命中节点及含命中子孙的文件夹 */
function filterTree(list: SessionNode[], kw: string): SessionNode[] {
  if (!kw) return list
  const out: SessionNode[] = []
  for (const n of list) {
    if (n.is_folder) {
      const children = filterTree(n.children ?? [], kw)
      if (n.name.toLowerCase().includes(kw) || children.length > 0) {
        out.push({ ...n, children })
      }
    } else if (
      n.name.toLowerCase().includes(kw) ||
      (n.config?.host ?? '').toLowerCase().includes(kw)
    ) {
      out.push(n)
    }
  }
  return out
}

/** 会话在线状态（批2 树状态点）：SSH/sftp/byte-stream 会话的连接键为每个 Tab 的 connId（非节点 id），
    反查工作台 Tab 匹配该会话来源后读 terminalStore 连接状态。connected 优先返回；任一连接中
    （connecting/主机校验，后端校验 hostkey 期间也属连接中）→ connecting；否则 offline（断开/从未连接）。
    数据库会话用图标语义色表达，不走此函数 */
function sessionNodeStatus(sessionId: string): FlatNode['status'] {
  let connecting = false
  for (const t of tabs.value) {
    if (t.sessionId !== sessionId || !t.connId) continue
    const st = terminalStore.sessionStatus[t.connId]
    if (st === 'connected') return 'connected'
    if (st === 'connecting' || st === 'hostkey-verify') connecting = true
  }
  return connecting ? 'connecting' : 'offline'
}

/** 扁平化渲染列表：搜索时强制展开全部层级 */
const flatNodes = computed<FlatNode[]>(() => {
  const kw = keyword.value.trim().toLowerCase()
  const forceExpand = kw.length > 0
  const out: FlatNode[] = []
  const walk = (list: SessionNode[], depth: number, section: 'ssh' | 'db' | 'redis') => {
    for (const n of list) {
      // mysql 与 redis 数据库会话分别挂数据库区/Redis 区（树图标与连接路由共用判定）
      const isMysqlSession = !n.is_folder && n.config?.session_type === 'mysql'
      const isRedisSession = !n.is_folder && n.config?.session_type === 'redis'
      const isDbSession = isMysqlSession || isRedisSession
      // 分区过滤：SSH 区跳过数据库会话；数据库区只保留 MySQL；Redis 区只保留 Redis
      //（数据库/Redis 区不显示文件夹，扁平列出其中嵌套的会话）
      if (section === 'ssh' && isDbSession) continue
      if ((section === 'db' || section === 'redis') && n.is_folder) {
        walk(n.children ?? [], depth, section)
        continue
      }
      if (section === 'db' && !isMysqlSession) continue
      if (section === 'redis' && !isRedisSession) continue
      // 已连接的 MySQL 会话：数据库作为子节点展示（Navicat 风格）
      //（同 host 的已存连接常驻节点不渲染，避免同一库挂两处）
      const hasDbChildren =
        section === 'db' && !n.is_folder && mysqlTreeDbs.value?.sessionId === n.id
      out.push({
        id: n.id,
        // 主机显示（Navicat 风格单行）：会话节点以主机地址为主，名称兜底
        name: n.is_folder ? n.name : (n.config?.host || n.name),
        depth,
        isFolder: !!n.is_folder,
        color: n.config?.color ?? null,
        isOpen: n.is_folder
          ? forceExpand || expanded.value.has(n.id)
          : hasDbChildren
            ? !collapsedDbHosts.value.has(n.id)
            : false,
        hostLabel: '',
        encoding: (n.config?.encoding as string | undefined) ?? null,
        isMysql: isDbSession,
        sessionType: n.config?.session_type ?? null,
        hasDbChildren,
        // 会话在线状态点：数据库会话用图标语义色表达状态，不填（不显示点）
        status: isDbSession ? undefined : sessionNodeStatus(n.id),
      })
      if (n.is_folder && (forceExpand || expanded.value.has(n.id))) {
        walk(n.children ?? [], depth + 1, section)
      } else if (hasDbChildren && !collapsedDbHosts.value.has(n.id)) {
        for (const dbName of mysqlTreeDbs.value!.databases) {
          out.push({
            id: `db-${n.id}-${dbName}`,
            name: dbName,
            depth: depth + 1,
            isFolder: false,
            color: null,
            isOpen: false,
            hostLabel: '',
            encoding: null,
            isMysql: false,
            isDbLeaf: true,
            dbName,
            grey: !mysqlTreeDbs.value!.connected || closedDbNames.value.has(dbName),
          })
        }
      }
    }
  }

  // 空分区不渲染分区头（无会话/已存连接时空分区头是噪音；按全量树判定，搜索时不闪变）
  const hasSshContent = nodes.value.length > 0
  const hasDbContent =
    nodes.value.some((n) => !n.is_folder && n.config?.session_type === 'mysql') ||
    useMysqlStore().savedConnections.length > 0
  const hasRedisContent =
    nodes.value.some((n) => !n.is_folder && n.config?.session_type === 'redis') ||
    useRedisStore().savedConnections.length > 0

  // 分区头（可收缩：单击折叠/展开）
  const sectionHeader = (id: string, name: string) => {
    out.push({
      id,
      name,
      depth: 0,
      isFolder: false,
      isSection: true,
      isOpen: openSections.value.has(id),
      color: null,
      hostLabel: '',
      encoding: null,
      isMysql: false,
    })
  }

  // SSH 服务分区：文件夹 + SSH 会话（会话/文件夹缩进一级，不与分区头齐平）
  if (hasSshContent) sectionHeader('section-ssh', 'SSH')
  if (hasSshContent && openSections.value.has('section-ssh')) {
    walk(filterTree(nodes.value, kw), 1, 'ssh')
  }

  // 分区虚线分隔（下方分区有内容时才需要，空分区隐藏后虚线不悬空）
  if (hasDbContent || hasRedisContent) {
    out.push({
      id: 'tree-separator',
      name: '',
      depth: 0,
      isFolder: false,
      color: null,
      isOpen: false,
      hostLabel: '',
      encoding: null,
      isMysql: false,
      isSeparator: true,
    })
  }

  // 数据库服务分区：数据库（MySQL/Redis）会话 + 已保存连接常驻节点 + 库列表（会话缩进一级，不与分区头齐平）
  if (hasDbContent) sectionHeader('section-db', '数据库')
  if (hasDbContent && openSections.value.has('section-db')) {
    walk(filterTree(nodes.value, kw), 1, 'db')
    const dbs = mysqlTreeDbs.value
    // 已保存连接常驻节点（Navicat 风格：未连接也显示主机名，点击连接，连接后展开库列表）。
    // 与 session 树中同 host 的 MySQL 会话去重（session 树已有该 host 则不重复出现）
    const sessionHosts = new Set<string>()
    for (const n of nodes.value) {
      if (!n.is_folder && n.config?.session_type === 'mysql' && n.config.host) {
        sessionHosts.add(n.config.host)
      }
    }
    for (const c of useMysqlStore().savedConnections) {
      if (sessionHosts.has(c.host)) continue
      // 搜索结果过滤：常驻节点按 host/名称匹配 keyword（不匹配则整棵下钻（库列表）一并隐藏）
      if (kw && !c.host.toLowerCase().includes(kw) && !c.name.toLowerCase().includes(kw)) continue
      const isActive = dbs?.savedId === c.id
      // 活动连接挂当前库；断开（灰色历史库）时按 host 匹配展示历史库
      const showDbs = !!dbs && (isActive || (!dbs.connected && dbs.host === c.host))
      // 折叠判定必须用完整节点 id（与 toggleDbHost 写入的 node.id 一致）
      const nodeId = `savedconn-${c.id}`
      out.push({
        id: nodeId,
        name: c.host,
        depth: 1,
        isFolder: false,
        color: null,
        isOpen: showDbs && !collapsedDbHosts.value.has(nodeId),
        hostLabel: '',
        encoding: null,
        isMysql: true,
        isSavedConn: true,
        savedConnId: c.id,
        hasDbChildren: showDbs,
      })
      // 该常驻节点是当前活动连接：库列表挂其下（断开后保留历史库以灰色展示，可点击收缩箭头折叠）
      if (showDbs && !collapsedDbHosts.value.has(nodeId)) {
        for (const dbName of dbs.databases) {
          out.push({
            id: `savdb-${c.id}-${dbName}`,
            name: dbName,
            depth: 2,
            isFolder: false,
            color: null,
            isOpen: false,
            hostLabel: '',
            encoding: null,
            isMysql: false,
            isDbLeaf: true,
            dbName,
            grey: !dbs.connected || closedDbNames.value.has(dbName),
          })
        }
      }
    }
    // 兜底：既无已保存连接匹配也无 session 树匹配时的独立节点（连接主机 + 库列表）；
    // 已存连接有同 host 常驻节点时不渲染（避免同一 host 两处展示）
    const standaloneTaken = useMysqlStore().savedConnections.some((c) => c.host === dbs?.host)
    // 搜索结果过滤：兜底独立节点按 host 匹配 keyword
    if (
      dbs &&
      dbs.savedId === null &&
      dbs.sessionId === null &&
      !standaloneTaken &&
      (!kw || dbs.host.toLowerCase().includes(kw))
    ) {
      const standaloneOpen = !collapsedDbHosts.value.has('db-standalone')
      out.push({
        id: 'db-standalone',
        name: dbs.host || 'MySQL 数据库',
        depth: 1,
        isFolder: false,
        color: null,
        isOpen: standaloneOpen,
        hostLabel: '',
        encoding: null,
        isMysql: true,
        isDbLeaf: false,
        hasDbChildren: true,
      })
      if (standaloneOpen) {
        for (const dbName of dbs.databases) {
          out.push({
            id: `db-standalone-${dbName}`,
            name: dbName,
            depth: 2,
            isFolder: false,
            color: null,
            isOpen: false,
            hostLabel: '',
            encoding: null,
            isMysql: false,
            isDbLeaf: true,
            dbName,
            grey: !dbs.connected || closedDbNames.value.has(dbName),
          })
        }
      }
    }
  }

  // Redis 独立分区：Redis 会话 + 已存连接常驻节点（对齐数据库服务的 Navicat 模式）
  if (hasRedisContent) sectionHeader('section-redis', 'Redis')
  if (hasRedisContent && openSections.value.has('section-redis')) {
    walk(filterTree(nodes.value, kw), 1, 'redis')
    // Redis 已存连接常驻节点（与同 host 的 Redis 会话去重；库管理在工作台 Tab 内）
    const redisSessionHosts = new Set<string>()
    for (const n of nodes.value) {
      if (!n.is_folder && n.config?.session_type === 'redis' && n.config.host) {
        redisSessionHosts.add(n.config.host)
      }
    }
    for (const c of useRedisStore().savedConnections) {
      if (redisSessionHosts.has(c.host)) continue
      // 搜索结果过滤：常驻节点按 host/名称匹配 keyword
      if (kw && !c.host.toLowerCase().includes(kw) && !c.name.toLowerCase().includes(kw)) continue
      out.push({
        id: `redisconn-${c.id}`,
        name: c.host,
        depth: 1,
        isFolder: false,
        color: null,
        isOpen: false,
        hostLabel: '',
        encoding: null,
        isMysql: false,
        sessionType: 'redis',
        isSavedConn: true,
        savedConnId: c.id,
      })
    }
  }

  return out
})

const selectedId = ref<string | null>(null)

/** 自动隐藏模式下鼠标是否悬停在导航上（悬停期间导航保持可见） */
const navHover = ref(false)

/** 导航可见性：未折叠（停靠展开）或自动隐藏模式悬停中 */
const navVisible = computed(() => !ui.navCollapsed || navHover.value)

/** 导航右缘拖拽调宽：按下后跟随鼠标横移，松开结束；双击恢复默认 240px */
function startNavResize(e: MouseEvent): void {
  e.preventDefault()
  const startX = e.clientX
  const startW = ui.navWidth
  const onMove = (ev: MouseEvent): void => {
    ui.setNavWidth(startW + (ev.clientX - startX))
  }
  // 集中解绑：mouseup 释放完成；窗口失焦（鼠标在窗口外释放等）兜底清理防监听泄漏
  const onUp = (): void => {
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
    window.removeEventListener('blur', onUp)
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
  window.addEventListener('blur', onUp)
}

function findNode(list: SessionNode[], id: string): SessionNode | null {
  for (const n of list) {
    if (n.id === id) return n
    if (n.children) {
      const hit = findNode(n.children, id)
      if (hit) return hit
    }
  }
  return null
}

/** 在树中查找节点父文件夹（无父 = 根级，返回 null） */
function findParentFolder(list: SessionNode[], id: string): SessionNode | null {
  for (const n of list) {
    if (n.children?.some((c) => c.id === id)) return n
    const hit = findParentFolder(n.children ?? [], id)
    if (hit) return hit
  }
  return null
}

// ---------------- 树拖拽（会话/文件夹归类与同层排序，顺序持久化） ----------------

/** 拖放参与判定：仅会话/文件夹行可拖、可真落点；db 叶子/已存连接/分区头/分隔线/数据库会话不参与 */
function isReorderable(node: FlatNode): boolean {
  if (node.isSection || node.isSeparator || node.isDbLeaf || node.isSavedConn) return false
  // 数据库/Redis 会话行（数据库分区内无文件夹可见，拖拽无归类意义，排除）
  if (!node.isFolder && node.isMysql) return false
  return true
}

/** 拖拽源（{id,kind}，仅会话/文件夹行发起） */
const dragSource = ref<{ id: string; kind: 'folder' | 'session' } | null>(null)
/** 悬停落点：目标行 id + 位置（into=移入文件夹 / before-after=插入其左右） */
const dropTarget = ref<{ id: string; position: 'into' | 'before' | 'after' } | null>(null)
/**
 * 手动指针拖拽基态（HTML5 draggable 在 WebView2 走 OLE 系统拖放集成，OS 级鼠标拖动
 * 不触发 dragstart/drop——树节点真实鼠标拖不动即此根因；弃用 HTML5 DnD，改 pointer
 * 事件 + 6px 阈值 + setPointerCapture 手动实现，与 FlexTabs 标签拖拽先例一致 0386eeb）
 */
const pDrag = ref<{ node: FlatNode; startX: number; startY: number; moved: boolean } | null>(null)
/** 判为「开始拖动」的最小位移（px），低于阈值视为点击不劫持 */
const DRAG_THRESHOLD = 6

/** 行 pointerdown：仅左键参与，记录拖动基态并捕获指针（后续 move/up 均收回到本行） */
function onTreePointerDown(node: FlatNode, e: PointerEvent): void {
  if (!isReorderable(node) || e.button !== 0) return
  dragSource.value = { id: node.id, kind: node.isFolder ? 'folder' : 'session' }
  pDrag.value = { node, startX: e.clientX, startY: e.clientY, moved: false }
  ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
}

/** 树容器 pointermove：越过阈值后进入拖动态，实时计算悬停行与落点位置 */
function onTreePointerMove(e: PointerEvent): void {
  const d = pDrag.value
  if (!d) return
  if (!d.moved && Math.hypot(e.clientX - d.startX, e.clientY - d.startY) < DRAG_THRESHOLD) return
  if (!d.moved) {
    // 首次越过阈值：锁定拖动态（此处不 preventDefault，避免影响后续 click 判定前移）
    d.moved = true
  }
  computeTreeDropTarget(e.clientX, e.clientY)
}

/** 拖拽结束/取消清理（pointerup / pointercancel 均可触发，幂等） */
function clearTreeDrag(): void {
  pDrag.value = null
  dragSource.value = null
  dropTarget.value = null
}

/** 拖拽后抑制随 pointerup 而来的 click（真实按下-抬起同元素会合成 click，防排序完即误开会话） */
const suppressTreeClickUntil = ref(0)

/** 行/容器 pointerup：真实拖过 → 落点执行；未拖过 → 视作普通点击清基态 */
function endTreePointerDrag(e: PointerEvent): void {
  const d = pDrag.value
  if (d?.moved) {
    suppressTreeClickUntil.value = Date.now() + 500
    void applyTreeDrop()
  } else {
    clearTreeDrag()
  }
}

/** 由指针坐标定位悬停行与落点位置（elementFromPoint 穿透 setPointerCapture 的元素遮蔽） */
function computeTreeDropTarget(clientX: number, clientY: number): void {
  if (!dragSource.value) return
  const hit = document.elementFromPoint(clientX, clientY)
  const el = hit?.closest<HTMLElement>('[data-node-id]') ?? null
  const node = el ? flatNodes.value.find((n) => n.id === el.dataset.nodeId) : undefined
  if (!el || !node || !isReorderable(node)) {
    // 悬停空白/不可落处：清空落点（drop 时按「根级追加」兜底处理）
    dropTarget.value = null
    return
  }
  const rect = el.getBoundingClientRect()
  const frac = (clientY - rect.top) / rect.height
  let position: 'into' | 'before' | 'after'
  if (node.isFolder) {
    // 文件夹行：上下 25% 为 before/after（跨文件夹排序），中部移入文件夹
    position = frac < 0.25 ? 'before' : frac > 0.75 ? 'after' : 'into'
  } else {
    // 会话行：上下半区分 before/after（会话为叶子不可移入）
    position = frac < 0.5 ? 'before' : 'after'
  }
  dropTarget.value = { id: node.id, position }
}

/** 某父级下的同类兄弟（父为 null = 根级；按当前树展示顺序返回） */
function siblingsOf(parent: SessionNode | null, kind: 'folder' | 'session'): SessionNode[] {
  const list = parent ? parent.children ?? [] : nodes.value
  return list.filter((n) => n.is_folder === (kind === 'folder'))
}

/**
 * 树容器 drop：按悬停落点解析目标父级与同类插入位 → 防环 → 重建受影响的
 * 父集合（源父 + 目标父，可能不同）各自同类顺序号（1..n）→ session_reorder 持久化 → 刷树。
 * 兼容「文件夹恒在会话上方」惯例：跨类落点自动夹到对应块边缘。
 */
async function applyTreeDrop(): Promise<void> {
  const src = dragSource.value
  if (!src) return
  const target = dropTarget.value ?? null
  clearTreeDrag()

  // 解析目标父级 + 同类插入位
  let targetParent: SessionNode | null = null
  let insertIndex = 0

  if (target && target.position === 'into') {
    const folder = findNode(nodes.value, target.id)
    if (!folder?.children) return
    targetParent = folder
    insertIndex = siblingsOf(folder, src.kind).length // append 与该文件夹内同类块末尾
  } else if (target) {
    const targetNode = findNode(nodes.value, target.id)
    if (!targetNode) return
    targetParent = findParentFolder(nodes.value, target.id)
    const sameKind = targetNode.is_folder === (src.kind === 'folder')
    if (sameKind) {
      const group = siblingsOf(targetParent, src.kind).filter((n) => n.id !== src.id)
      const idx = group.findIndex((n) => n.id === target.id)
      insertIndex = target.position === 'before' ? idx : idx + 1
    } else if (src.kind === 'folder') {
      insertIndex = siblingsOf(targetParent, 'folder').filter((n) => n.id !== src.id).length
    } else {
      insertIndex = 0 // 会话块起始
    }
  } else {
    insertIndex = siblingsOf(null, src.kind).length // 空白落点：根级追加
  }

  // 防环：文件夹不能拖入自身或其后代（沿目标父链上行，遇到源 id 即成环）
  if (src.kind === 'folder') {
    let cur = targetParent
    while (cur) {
      if (cur.id === src.id) {
        ui.toast('不能把文件夹拖入自身或其子文件夹', 'error')
        return
      }
      cur = findParentFolder(nodes.value, cur.id)
    }
  }

  // 持久化：目标父集合重排（含源插入）+ 源父 ≠ 目标父时源父剩余同类重排
  const items: NodeReorderItem[] = []
  const inline = (parent: SessionNode | null): void => {
    const ids = siblingsOf(parent, src.kind)
      .map((n) => n.id)
      .filter((nid) => nid !== src.id)
    if (parent === targetParent) {
      ids.splice(Math.min(insertIndex, ids.length), 0, src.id)
    }
    const parentId = parent ? parent.id : null
    ids.forEach((nid, i) => items.push({ kind: src.kind, id: nid, parent_id: parentId, order: i + 1 }))
  }
  inline(targetParent)
  const srcParent = findParentFolder(nodes.value, src.id)
  if (srcParent !== targetParent) inline(srcParent)

  if (!items.length) return
  try {
    await sessionReorder(items)
    await loadTree()
  } catch (err) {
    ui.toast(friendlyError(err), 'error')
  }
}

function toggleFolder(id: string): void {
  const set = new Set(expanded.value)
  if (set.has(id)) {
    set.delete(id)
  } else {
    set.add(id)
  }
  expanded.value = set
}

/**
 * 树节点图标着色：自定义色优先，未设置按会话类型给 Navicat 式分类色。
 * 返回 Vuetify 语义色名（双主题自适应）；文件头/分区不着色（null）。
 */
/** 数据库 host 节点连接状态（图标/颜色共用）：按实际连接状态判定。
    灰色历史库挂载态（断开/重启恢复）不算连接——hasDbChildren 不能作为连接判据 */
function dbHostConnected(node: FlatNode): boolean {
  if (node.isSavedConn) {
    return node.isMysql
      ? useMysqlStore().activeSavedId === node.savedConnId && !!useMysqlStore().connId
      : useRedisStore().activeSavedId === node.savedConnId && !!useRedisStore().connId
  }
  const stype = findNode(nodes.value, node.id)?.config?.session_type ?? ''
  if (stype === 'mysql') return !!useMysqlStore().connId
  if (stype === 'redis') return !!useRedisStore().connId
  // 兜底独立节点（db-standalone）：连接态随 mysqlTreeDbs.connected
  return !!mysqlTreeDbs.value?.connected
}

/** 树节点图标名（连接状态感知：数据库 host 已连接=数据库，未连接=断开插头，Navicat 风格） */
function treeIcon(node: FlatNode): string {
  if (node.isSection) return node.isOpen ? 'mdi-chevron-down' : 'mdi-chevron-right'
  if (node.isFolder) return node.isOpen ? 'mdi-folder-open' : 'mdi-folder'
  if (node.isDbLeaf) return 'mdi-database-outline'
  if (node.isSavedConn || node.isMysql || node.sessionType === 'redis') {
    return dbHostConnected(node) ? 'mdi-database' : 'mdi-lan-disconnect'
  }
  switch (node.sessionType) {
    case 'telnet':
      return 'mdi-console-network'
    case 'rlogin':
      return 'mdi-send'
    case 'serial':
      return 'mdi-usb-port'
    case 'sftp':
      return 'mdi-folder-swap-outline'
    default:
      return 'mdi-console'
  }
}

function treeIconColor(node: FlatNode): string | null {
  if (node.isSection) return null
  if (node.isFolder) return 'amber'
  // 数据库叶子：已连接 success（绿色，全项目运行状态统一定义），未连接/已关闭灰色（整行 opacity 弱化）
  if (node.isDbLeaf) return node.grey ? null : 'success'
  // 数据库 host 节点：已连接绿色 / 断开（含异常断开）红色 / 未连接灰色
  if (node.isSavedConn || node.isMysql || node.sessionType === 'redis') {
    if (!dbHostConnected(node)) {
      const store = node.isMysql ? useMysqlStore() : useRedisStore()
      return store.dropped ? 'error' : 'grey'
    }
    return 'success'
  }
  if (node.color) return node.color
  switch (node.sessionType) {
    case 'telnet':
      return 'warning'
    case 'rlogin':
      return 'accent'
    case 'serial':
      return 'info'
    default:
      return 'success'
  }
}

/** 数据库 host 节点库列表折叠开关（点击收缩箭头切换，不影响节点本身的连接行为） */
const collapsedDbHosts = ref(new Set<string>())

/** 切换数据库 host 节点的库列表折叠态 */
function toggleDbHost(node: FlatNode): void {
  const set = new Set(collapsedDbHosts.value)
  if (set.has(node.id)) {
    set.delete(node.id)
  } else {
    set.add(node.id)
  }
  collapsedDbHosts.value = set
}

/** 树行收缩箭头分发：文件夹行切换子级展开（expanded），host 行切换库列表（collapsedDbHosts） */
function toggleTreeExpand(node: FlatNode): void {
  if (node.isFolder) {
    toggleFolder(node.id)
    return
  }
  toggleDbHost(node)
}

/** 单击/双击 host 行：打开对应数据库工作台并按节点建立连接（MySQL/Redis）。
    独立兜底节点无 SessionConfig，仅打开 MySQL 工作台 */
function openDbWorkspaceFromNode(node: FlatNode): void {
  if (node.id === 'db-standalone') {
    openMysqlTab()
    return
  }
  if (node.isSavedConn) {
    if (node.isMysql) void connectMysqlSaved(node.savedConnId ?? '')
    else void useRedisStore().connectSaved(node.savedConnId ?? '')
    return
  }
  const target = findNode(nodes.value, node.id)
  if (!target) return
  const stype = target.config?.session_type
  if (stype === 'redis') {
    void connectRedisSession(target)
    return
  }
  if (stype === 'mysql') {
    void connectMysqlSession(target)
    return
  }
  openMysqlTab()
}

function onNodeClick(node: FlatNode): void {
  // pointer 拖拽后的合成 click 抑制（拖拽排序不应误触连接/展开）
  if (suppressTreeClickUntil.value && Date.now() < suppressTreeClickUntil.value) {
    suppressTreeClickUntil.value = 0
    return
  }
  // 分区头：单击折叠/展开
  if (node.isSection) {
    const set = new Set(openSections.value)
    if (set.has(node.id)) {
      set.delete(node.id)
    } else {
      set.add(node.id)
    }
    openSections.value = set
    return
  }
  selectedId.value = node.id
  if (node.isFolder) {
    toggleFolder(node.id)
    return
  }
  // 数据库子节点：单击切换当前库（内部按时间戳去重）
  if (node.isDbLeaf) {
    void switchMysqlDbFromTree(node)
    return
  }
  // 数据库 host 行：单击/双击打开对应数据库工作台并建连（MySQL/Redis）；
  // 库列表展开/收缩只由收缩箭头负责
  if (node.hasDbChildren) {
    openDbWorkspaceFromNode(node)
    return
  }
  // 已保存连接节点：打开对应工作台并按保存配置建连（MySQL/Redis）
  if (node.isSavedConn) {
    if (node.isMysql) {
      void connectMysqlSaved(node.savedConnId ?? '')
    } else {
      openRedisTab()
      void useRedisStore().connectSaved(node.savedConnId ?? '')
    }
    return
  }
  // 会话单击即连接：每次单击都新建独立连接 Tab（不限制多开，同一会话可并存多个终端）
  const target = findNode(nodes.value, node.id)
  if (!target) return
  openSessionByType(target)
}

/** 树节点键盘操作：Enter 打开/连接，Space 选中/折叠 */
function onNodeKeydown(node: FlatNode, e: KeyboardEvent): void {
  if (e.key === 'Enter' || e.key === ' ' || e.key === 'Spacebar') {
    e.preventDefault()
    onNodeClick(node)
  }
}

// ---------------- 树右键菜单（连接 / 重命名 / 删除） ----------------

const treeMenu = reactive({ visible: false, x: 0, y: 0, node: null as FlatNode | null })

function onTreeContextmenu(node: FlatNode, e: MouseEvent): void {
  // 分隔线/MySQL 兜底独立节点无菜单（独立节点无 SessionConfig，操作一律在工作台内）
  if (node.isSeparator || node.id === 'db-standalone') return
  // 分区头（SSH/数据库/Redis）：右键提供批量删除该分区下所有会话；
  // 分区下没有可删除的会话时整个菜单不开（删除是唯一菜单项，空分区直接隐藏）
  if (node.isSection) {
    const section: 'ssh' | 'db' | 'redis' | null =
      node.id === 'section-ssh' ? 'ssh' : node.id === 'section-db' ? 'db' : node.id === 'section-redis' ? 'redis' : null
    if (!section || sectionSessionNodes(section).length === 0) return
    treeMenu.x = e.clientX
    treeMenu.y = e.clientY
    treeMenu.node = node
    treeMenu.visible = true
    return
  }
  selectedId.value = node.id
  treeMenu.x = e.clientX
  treeMenu.y = e.clientY
  treeMenu.node = node
  treeMenu.visible = true
}

/** 右键菜单上下文：按节点类型 + 连接状态派生菜单项（Navicat 风格状态感知，未连接灰化数据库操作） */
const treeMenuCtx = computed(() => {
  const node = treeMenu.node
  if (!node) return null
  const mysqlStore = useMysqlStore()
  const redisStore = useRedisStore()
  // 分区头：仅提供删除项（批量删除该分区下所有会话）
  if (node.isSection) {
    return { kind: 'section' as const, isDb: false, isSessionNode: false, connected: false, connId: null }
  }
  // 库叶子：操作对象即当前活动 MySQL 连接
  if (node.isDbLeaf) {
    const connId = mysqlStore.connId
    return {
      kind: 'db-leaf' as const,
      isDb: true,
      isSessionNode: false,
      connected: !!connId,
      connId,
      // 库打开态：连接存活且未被「关闭数据库」收起（收起的库以灰色展示）
      open: !!connId && !closedDbNames.value.has(node.dbName ?? ''),
    }
  }
  // 已存连接常驻节点：isMysql=true → MySQL，否则 Redis
  if (node.isSavedConn) {
    const connId = node.isMysql ? mysqlStore.connId : redisStore.connId
    return {
      kind: 'conn' as const,
      isDb: node.isMysql,
      isSessionNode: false,
      connected: !!connId,
      connId,
    }
  }
  // 会话节点：数据库类型共享连接菜单，其余（SSH/Telnet 等）通用会话菜单
  const stype = findNode(nodes.value, node.id)?.config?.session_type
  if (stype === 'mysql' || stype === 'redis') {
    const connId = stype === 'mysql' ? mysqlStore.connId : redisStore.connId
    return { kind: 'conn' as const, isDb: stype === 'mysql', isSessionNode: true, connected: !!connId, connId }
  }
  return { kind: 'session' as const, isDb: false, isSessionNode: true, connected: false, connId: null }
})

/** 右键菜单"打开连接"：打开该会话终端（已存连接节点按类型走对应 store 的 connectSaved） */
function menuConnect(): void {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node) return
  // 已存连接常驻节点：树中无 SessionConfig，按类型分流到对应 store
  if (node.isSavedConn) {
    if (node.isMysql) void connectMysqlSaved(node.savedConnId ?? '')
    else void useRedisStore().connectSaved(node.savedConnId ?? '')
    return
  }
  if (!node.isFolder) {
    const target = findNode(nodes.value, node.id)
    if (!target) return
    // 数据库会话切工作台、telnet 系开终端、sftp 开独立 SFTP 双栏 Tab（统一分发）
    openSessionByType(target)
  }
}

/** 连接数据库会话：激活 MySQL 工作台 Tab + 用会话配置建立连接（成功后回填已存连接列表） */
async function connectMysqlSession(target: SessionNode): Promise<void> {
  const mysqlStore = useMysqlStore()
  openMysqlTab()
  // 树节点的 config 即后端 SessionConfig（含 auth_type），按宽松类型取连接字段
  const cfg = (target.config ?? {}) as {
    host?: string
    port?: number
    username?: string
    auth_type?: { type: string; password?: string }
  }
  const auth = cfg.auth_type
  const connectCfg = {
    host: cfg.host ?? '',
    port: cfg.port ?? 3306,
    username: cfg.username ?? '',
    password: auth && (auth.type === 'password' || auth.type === 'interactive') ? (auth.password ?? '') : '',
    schema: null as string | null,
  }
  // 已连接/正在连接同一配置时直接复用（单击导航语义，双击/重复点击不反复重连）
  const last = mysqlStore.lastConfig
  if (
    (mysqlStore.connId || mysqlStore.connecting) &&
    last &&
    last.host === connectCfg.host &&
    last.port === connectCfg.port &&
    last.username === connectCfg.username
  ) {
    return
  }
  try {
    // 已有连接时先断开（切换连接语义，与 connectSaved 一致）
    if (mysqlStore.connId) await mysqlStore.disconnect()
    await mysqlStore.connect(connectCfg)
    // 连接成功回填已存连接列表（按 host+port+username 去重）
    mysqlStore.activeSavedId = mysqlStore.saveConnection(connectCfg)
    // 数据库子节点由 connId watch 统一挂载（覆盖工作台等入口）
  } catch {
    // 连接失败由 MySQL 工作台 v-alert 展示（store.connError）
  }
}

/** 连接 Redis 数据库会话：激活 Redis 工作台 Tab + 用会话配置建立连接（镜像 connectMysqlSession） */
async function connectRedisSession(target: SessionNode): Promise<void> {
  const redisStore = useRedisStore()
  openRedisTab()
  // 树节点的 config 即后端 SessionConfig（含 auth_type），按宽松类型取连接字段
  const cfg = (target.config ?? {}) as {
    host?: string
    port?: number
    username?: string
    auth_type?: { type: string; password?: string }
  }
  const auth = cfg.auth_type
  // RedisConnection：用户名/密码可空（空串归一化为 null）
  const connectCfg: RedisConnection = {
    host: cfg.host ?? '',
    port: cfg.port ?? 6379,
    username: cfg.username?.trim() ? cfg.username.trim() : null,
    password: auth && (auth.type === 'password' || auth.type === 'interactive') ? (auth.password ?? null) : null,
    db: 0,
  }
  // 已连接/正在连接同一配置时直接复用（单击导航语义，双击/重复点击不反复重连）
  const last = redisStore.lastConfig
  if (
    (redisStore.connId || redisStore.connecting) &&
    last &&
    last.host === connectCfg.host &&
    last.port === connectCfg.port &&
    last.username === connectCfg.username
  ) {
    return
  }
  try {
    // 已有连接时先断开（切换连接语义，与 connectSaved 一致）
    if (redisStore.connId) await redisStore.disconnect()
    await redisStore.connect(connectCfg)
    // 连接成功回填已存连接列表（按 host+port+username 去重）
    redisStore.activeSavedId = redisStore.saveConnection(target.name ?? '', connectCfg)
  } catch {
    // 连接失败由 Redis 工作台 v-alert 展示（store.connError）
  }
}

/** 点击已保存连接常驻节点：激活 MySQL 工作台 + 按保存配置建连 */
async function connectMysqlSaved(savedId: string): Promise<void> {
  const mysqlStore = useMysqlStore()
  if (!savedId) return
  // 已在连接同一保存配置时直接复用（双击连发 click 不反复重连）
  if (mysqlStore.activeSavedId === savedId && mysqlStore.connId) return
  openMysqlTab()
  try {
    await mysqlStore.connectSaved(savedId)
  } catch {
    // 连接失败由 MySQL 工作台 v-alert 展示（store.connError）
  }
}

/** 树中最近一次切库（时间戳去重：双击会连发 click+dblclick，800ms 内重复点击同一库直接跳过） */
let lastDbSwitchId = ''
let lastDbSwitchAt = 0

/** 树中双击数据库子节点：切换 MySQL 当前库并刷新表清单（统一走 store.switchDb） */
async function switchMysqlDbFromTree(node: FlatNode): Promise<void> {
  const mysqlStore = useMysqlStore()
  const dbName = node.dbName
  if (!mysqlStore.connId || !dbName) return
  if (dbName === lastDbSwitchId && Date.now() - lastDbSwitchAt < 800) return
  lastDbSwitchId = dbName
  lastDbSwitchAt = Date.now()
  try {
    // 打开 MySQL 工作台展示切库结果（树中切库的可见反馈）
    openMysqlTab()
    // 统一走 store.switchDb：后端重建连接池、内部按连接代际守卫回写
    //（MysqlDbWorkspace 的 connId watch 随之清空会话级状态并刷新库下拉）
    await mysqlStore.switchDb(dbName)
  } catch {
    // 切库失败由 MySQL 工作台 v-alert 展示；清除去重标记允许立即重试
    lastDbSwitchId = ''
  }
}

/** 右键菜单"会话设置"：树节点即会话 id，打开 SSH 会话选项对话框 */
function menuSessionSettings(): void {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node) return
  openSessionSettingsFor({ sessionId: node.id, title: node.name })
}

/** 右键菜单"重命名"：打开会话表单编辑态（文件夹打开名称对话框并预填原名；已存连接按保存配置预填） */
function menuRename(): void {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node) return
  if (node.isFolder) {
    // 进入重命名编辑态：保留原 id 与父级，预填原名（此前误走新建逻辑——
    // 重命名实际再建了一个文件夹，原文件夹纹丝不动）
    editingFolderId.value = node.id
    editingFolderParent.value = folderParents.get(node.id) ?? null
    folderName.value = node.name
    showFolderDialog.value = true
    return
  }
  // 已存连接常驻节点：树中无 SessionConfig，从对应 store 取保存配置预填（保存走 saveHandler 覆盖链路）
  if (node.isSavedConn) {
    const store = node.isMysql ? useMysqlStore() : useRedisStore()
    const saved = store.savedConnections.find((c) => c.id === (node.savedConnId ?? ''))
    if (!saved) return
    editingSavedConn.value = { mysql: !!node.isMysql, id: saved.id }
    editingSession.value = {
      id: '',
      name: saved.name,
      session_type: node.isMysql ? 'mysql' : 'redis',
      host: saved.host,
      port: saved.port,
      username: saved.username ?? '',
      encoding: null,
      color: null,
      keepalive_interval: 30,
      auth_type: { type: 'password', password: saved.password ?? '' },
    } as unknown as SessionConfig
    presetHost.value = ''
    presetSftp.value = false
    showSessionForm.value = true
    return
  }
  const target = findNode(nodes.value, node.id)
  if (!target) return
  editingSession.value = (target.config ?? {}) as unknown as SessionConfig
  presetHost.value = ''
  presetSftp.value = false
  showSessionForm.value = true
}

/** 右键菜单"删除"：二次确认后删除。
    已存连接常驻节点（savedconn-xxx / redisconn-xxx）走对应 store 的 removeConnection
    （真实数据在 settings 表，session_delete 查无此行会静默失败）；
    会话/文件夹走 session_delete。 */
async function menuDelete(): Promise<void> {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node) return
  // 已存连接常驻节点：删的是已保存的连接配置，按类型分流到 mysql/redis store
  if (node.isSavedConn) {
    const id = node.savedConnId ?? ''
    if (!id) return
    const store = node.isMysql ? useMysqlStore() : useRedisStore()
    const ok = await ui.confirm({
      title: '删除确认',
      message: `确定删除连接「${node.name}」吗？此操作不可恢复。`,
      danger: true,
    })
    if (!ok) return
    try {
      const ok = await store.removeConnection(id)
      await loadTree()
      if (ok) {
        ui.toast(`已删除连接「${node.name}」`, 'success')
      } else {
        ui.toast(`已删除连接「${node.name}」，但保存列表落盘失败，重启后可能恢复`, 'warning')
      }
    } catch (e) {
      ui.toast(`删除失败：${friendlyError(e)}`, 'error')
    }
    return
  }
  const label = node.isFolder
    ? `文件夹「${node.name}」（含其中所有会话）`
    : `会话「${node.name}」`
  const ok = await ui.confirm({
    title: '删除确认',
    message: `确定删除${label}吗？此操作不可恢复。`,
    danger: true,
  })
  if (!ok) return
  try {
    await sessionStore.remove(node.id, node.isFolder)
    await loadTree()
    ui.toast(`已删除${label}`, 'success')
  } catch (e) {
    ui.toast(`删除失败：${friendlyError(e)}`, 'error')
  }
}

/** 分区待删除目标：session 会话（session_delete）或已存连接常驻节点（store.removeConnection） */
type SectionTarget = { id: string; name: string; isSaved: boolean; isMysql: boolean }

/** 分区头右键"删除"：收集该分区下所有可删对象（SSH 区为非数据库会话；数据库区/Redis 区
    除 session 会话外，还包括已存连接常驻节点——常驻连接存于 settings 表不在 session 树，
    漏收会导致分区批量删除永远"无可删会话"） */
function sectionSessionNodes(section: 'ssh' | 'db' | 'redis'): SectionTarget[] {
  const out: SectionTarget[] = []
  const walk = (list: SessionNode[]): void => {
    for (const n of list) {
      const stype = n.config?.session_type
      if (n.is_folder) {
        walk(n.children ?? [])
        continue
      }
      if (section === 'ssh' && (stype === 'mysql' || stype === 'redis')) continue
      if (section === 'db' && stype !== 'mysql') continue
      if (section === 'redis' && stype !== 'redis') continue
      out.push({ id: n.id, name: n.name, isSaved: false, isMysql: stype === 'mysql' })
    }
  }
  walk(nodes.value)
  // 已存连接常驻节点：数据库区收集 MySQL 已存连接；Redis 区收集 Redis 已存连接
  //（session 树已有同 host 会话的会在渲染时去重，这里按相同 host 规则同样跳过避免重复计数）
  if (section === 'db' || section === 'redis') {
    const isMysql = section === 'db'
    const store = isMysql ? useMysqlStore() : useRedisStore()
    const hostSet = new Set<string>()
    for (const n of nodes.value) {
      if (!n.is_folder && n.config?.session_type === (isMysql ? 'mysql' : 'redis') && n.config.host) {
        hostSet.add(n.config.host)
      }
    }
    for (const c of store.savedConnections) {
      if (hostSet.has(c.host)) continue
      out.push({ id: c.id, name: c.name, isSaved: true, isMysql })
    }
  }
  return out
}

/** 分区头右键"删除"：确认后批量删除该分区下所有会话 */
async function menuDeleteSection(): Promise<void> {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node || !node.isSection) return
  const section: 'ssh' | 'db' | 'redis' | null =
    node.id === 'section-ssh' ? 'ssh' : node.id === 'section-db' ? 'db' : node.id === 'section-redis' ? 'redis' : null
  if (!section) return
  const targets = sectionSessionNodes(section)
  if (targets.length === 0) {
    ui.toast(`「${node.name}」分区下没有可删除的会话`, 'warning')
    return
  }
  const ok = await ui.confirm({
    title: '删除确认',
    message: `确定删除「${node.name}」分区下的所有会话（共 ${targets.length} 个）吗？此操作不可恢复。`,
    danger: true,
  })
  if (!ok) return
  const failed: string[] = []
  for (const t of targets) {
    try {
      if (t.isSaved) {
        // 已存连接常驻节点：走对应 store 的 removeConnection，落盘失败同样计入失败
        const store = t.isMysql ? useMysqlStore() : useRedisStore()
        const ok = await store.removeConnection(t.id)
        if (!ok) failed.push(`${t.name}（保存列表落盘失败，重启后可能恢复）`)
      } else {
        await sessionStore.remove(t.id, false)
      }
    } catch (e) {
      // 单条删除失败不中断整批：记录后继续删剩余项，最后统一汇总
      failed.push(`${t.name}（${friendlyError(e)}）`)
    }
  }
  await loadTree()
  const removed = targets.length - failed.length
  if (failed.length === 0) {
    ui.toast(`已删除「${node.name}」分区下的 ${removed} 个会话`, 'success')
  } else {
    ui.toast(`已删除 ${removed} 个，${failed.length} 个失败：${failed[0]}`, 'error')
  }
}

/** 右键菜单"关闭连接"：断开该节点对应的数据库连接（MySQL/Redis store 按类型分流） */
async function menuCloseConnection(): Promise<void> {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node) return
  const isMysql = node.isSavedConn
    ? node.isMysql
    : findNode(nodes.value, node.id)?.config?.session_type === 'mysql'
  if (isMysql) {
    await useMysqlStore().disconnect()
  } else {
    await useRedisStore().disconnect()
  }
}

/** 右键菜单"复制连接"：session_clone（自动保存，名称加"副本"后缀）后弹出编辑对话框（复制后改名是高频后续操作，与菜单省略号语义一致） */
async function menuCloneConnection(): Promise<void> {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node || node.isFolder) return
  try {
    const cloned = await sessionClone(node.id)
    ui.toast(`已复制连接「${node.name}」，请确认新名称`, 'success')
    await loadTree()
    // 复制后立即弹出编辑：装载副本配置（新 uuid，保存即更新副本）
    editingSession.value = cloned as unknown as SessionConfig
    presetHost.value = ''
    presetSftp.value = false
    showSessionForm.value = true
  } catch (e) {
    ui.toast(`复制连接失败：${friendlyError(e)}`, 'error')
  }
}

/** 查询 Tab 序号（title 查询-N，Navicat 每次右键新开一个查询窗口） */
let queryTabSeq = 0

/** 新建查询 Tab（独立编辑器 Tab，关闭不影响连接） */
function openQueryTab(): void {
  queryTabSeq += 1
  const tab: WorkTab = { id: genTabId(), type: 'query', title: `查询-${queryTabSeq}` }
  tabs.value.push(tab)
  activeId.value = tab.id
}

/** 右键菜单"新建查询"：打开查询 Tab；库叶子额外切到目标库（Navicat 查询跟随库） */
async function menuNewQuery(): Promise<void> {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node) return
  // 库叶子 / MySQL 会话与已存连接 → 查询 Tab；Redis → Redis 工作台
  if (node.isDbLeaf || (node.isSavedConn && node.isMysql)) {
    if (node.isDbLeaf && !useMysqlStore().connId) {
      ui.toast('请先连接 MySQL 数据库后再新建查询', 'warning')
      return
    }
    openQueryTab()
    // 库叶子：切到右键的库（连接默认库全局生效，工作台 connId watch 自动同步）
    if (node.isDbLeaf && node.dbName) await switchMysqlDb(node.dbName)
    return
  }
  if (node.isSavedConn) {
    openRedisTab()
    return
  }
  const stype = findNode(nodes.value, node.id)?.config?.session_type
  if (stype === 'redis') {
    openRedisTab()
    return
  }
  openQueryTab()
}

/** 切换 MySQL 连接默认库（统一走 store.switchDb：代际守卫，与工作台/树/命令列同型） */
async function switchMysqlDb(name: string): Promise<void> {
  const st = useMysqlStore()
  if (!st.connId || !name) return
  try {
    const applied = await st.switchDb(name)
    // 过期切换（已被其它入口的切库接管）不提示，避免陈旧成功 toast
    if (applied) ui.toast(`已切换到数据库「${name}」`, 'success')
  } catch (e) {
    ui.toast(friendlyError(e), 'error')
  }
}

/** 右键菜单"命令列界面"：新建命令列 Tab（Navicat 每次点击新开一个 mysql> 控制台） */
function menuNewCli(): void {
  treeMenu.visible = false
  const st = useMysqlStore()
  if (!st.connId) {
    ui.toast('请先连接 MySQL 数据库后再打开命令列界面', 'warning')
    return
  }
  const tab: WorkTab = { id: genTabId(), type: 'mysqlcli', title: '命令列界面' }
  tabs.value.push(tab)
  activeId.value = tab.id
}

/** 右键菜单"运行 SQL 文件"：挂载导入导出对话框（作用于当前活动 MySQL 连接） */
function menuRunSqlFile(): void {
  treeMenu.visible = false
  const connId = useMysqlStore().connId
  if (!connId) return
  ioDialogConnId.value = connId
  ioDialogVisible.value = true
}

/** 右键菜单"关闭数据库"：库叶子收起为灰色（closedDbNames 标记，重连后自动恢复） */
function menuCloseDb(): void {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node?.dbName) return
  closedDbNames.value = new Set([...closedDbNames.value, node.dbName])
}

/** 右键菜单"打开数据库"：解除收起标记并打开数据库工作台 Tab */
function menuOpenDb(): void {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node?.dbName) return
  if (closedDbNames.value.has(node.dbName)) {
    const next = new Set([...closedDbNames.value])
    next.delete(node.dbName)
    closedDbNames.value = next
  }
  openMysqlTab()
}

/** 右键菜单"转储 SQL 文件"子菜单：挂载导入导出对话框（导出模式，预设是否包含建表语句） */
function menuDumpSql(includeCreate: boolean): void {
  treeMenu.visible = false
  const connId = useMysqlStore().connId
  if (!connId) return
  ioIncludeCreateTable.value = includeCreate
  ioDialogConnId.value = connId
  ioDialogVisible.value = true
}

/** 右键菜单"编辑数据库..."：打开编辑对话框（修改库默认字符集/排序规则） */
function menuEditDb(): void {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node?.dbName) return
  if (!useMysqlStore().connId) {
    ui.toast('请先连接 MySQL 数据库后再编辑库', 'warning')
    return
  }
  editDbName.value = node.dbName
  showEditDbDialog.value = true
}

/** 右键菜单"打印数据库"：生成全库结构报告并调起 WebView 打印 */
function menuPrintDb(): void {
  treeMenu.visible = false
  const node = treeMenu.node
  const connId = useMysqlStore().connId
  if (!node?.dbName) return
  if (!connId) {
    ui.toast('请先连接 MySQL 数据库后再打印', 'warning')
    return
  }
  const dbName = node.dbName
  void (async () => {
    try {
      const [tables, colResult] = await Promise.all([
        mysqlListTables(connId),
        mysqlQuery(
          connId,
          `SELECT TABLE_NAME, COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_KEY, COLUMN_COMMENT FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = ${sqlStr(dbName)} ORDER BY TABLE_NAME, ORDINAL_POSITION`,
          1,
          1000000,
        ),
      ])
      printHtml(buildStructureReport(dbName, tables, colResult))
    } catch (e) {
      ui.toast(`打印数据库失败：${friendlyError(e)}`, 'error')
    }
  })()
}

/** 右键菜单"逆向数据库到模型..."：打开 ER 图对话框 */
function menuErModel(): void {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node?.dbName) return
  if (!useMysqlStore().connId) {
    ui.toast('请先连接 MySQL 数据库后再生成模型', 'warning')
    return
  }
  erDbName.value = node.dbName
  showErModelDialog.value = true
}

/** 右键菜单"在数据库中查找"：打开全库查找对话框 */
function menuFindInDb(): void {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node?.dbName) return
  if (!useMysqlStore().connId) {
    ui.toast('请先连接 MySQL 数据库后再查找', 'warning')
    return
  }
  findDbName.value = node.dbName
  showFindDialog.value = true
}

/** 右键菜单"新建数据库"：打开新建对话框（常规分区：数据库名称/字符集/排序规则） */
function menuCreateDb(): void {
  treeMenu.visible = false
  if (!useMysqlStore().connId) return
  showNewDbDialog.value = true
}

/** 右键菜单"删除数据库"：二次确认 → mysql_db_drop → 刷新树中库节点。
    未连接 MySQL 时给出提示（库叶子来自已存连接/历史库，未连接也可右键到） */
async function menuDropDb(): Promise<void> {
  treeMenu.visible = false
  const node = treeMenu.node
  const connId = useMysqlStore().connId
  if (!node?.dbName) return
  if (!connId) {
    ui.toast('请先连接 MySQL 数据库后再删除库', 'warning')
    return
  }
  const ok = await ui.confirm({
    title: '删除数据库',
    message: `确定删除数据库「${node.dbName}」吗？库中所有数据将丢失，此操作不可恢复。`,
    danger: true,
  })
  if (!ok) return
  try {
    // UI 强确认通过后带 confirmed=true 重发（后端强确认管道约定，与工作台删除库一致）
    await mysqlDbDrop(connId, node.dbName, true)
    ui.toast(`数据库「${node.dbName}」已删除`, 'success')
    await refreshMysqlTreeDbs()
  } catch (e) {
    ui.toast(`删除数据库失败：${friendlyError(e)}`, 'error')
  }
}

/** 右键菜单"刷新"：重载会话树 + 当前连接的库节点 */
async function menuRefresh(): Promise<void> {
  treeMenu.visible = false
  await loadTree()
  await refreshMysqlTreeDbs()
}

// ---------------- 树右键"新建数据库"对话框 ----------------

const showNewDbDialog = ref(false)

// ---------------- 树右键"编辑数据库/逆向到模型/在库中查找"对话框 ----------------

/** 当前活动 MySQL 连接 ID（对话框 connId 挂当前连接，随 store 响应式更新；未连接为空串） */
const mysqlConnId = computed(() => useMysqlStore().connId ?? '')

const showEditDbDialog = ref(false)
const editDbName = ref('')
const showErModelDialog = ref(false)
const erDbName = ref('')
const showFindDialog = ref(false)
const findDbName = ref('')

/** 全库查找结果「打开表」：切到查找的库并在网格中打开该表（数据可编辑，与树中切库同款语义） */
async function openFoundTable(table: string): Promise<void> {
  showFindDialog.value = false
  const mysqlStore = useMysqlStore()
  if (!mysqlStore.connId || !findDbName.value) return
  openMysqlTab()
  try {
    // 统一走 store.switchDb（切到查找的库；表名按当前库解析，切库保证命中）。
    // 未生效（已被其它入口的切库接管）则不打开表，避免错库错表
    const applied = await mysqlStore.switchDb(findDbName.value)
    if (applied) mysqlStore.pendingOpenTable = table
  } catch (e) {
    ui.toast(`切换数据库失败：${friendlyError(e)}`, 'error')
  }
}

// ---------------- 工具栏数据库工具 4 项（数据传输/数据生成/数据同步/结构同步） ----------------

/** MySQL 已连接时显示 4 个数据库工具按钮（与激活 Tab 无关），断开后隐藏 */
const mysqlDbTools = computed(() => !!useMysqlStore().connId)

const showDbTransfer = ref(false)
const showDataGenerate = ref(false)
const showDbSync = ref(false)
const showStructureSync = ref(false)

/** SQL 字符串字面量（单引号翻倍转义） */
function sqlStr(value: string): string {
  return `'${value.replace(/'/g, "''")}'`
}

/** 拼 HTML 结构报告（打印数据库）：标题=库+生成时间，每表小节=表名/行数/引擎/注释 + 列表格 */
function buildStructureReport(
  dbName: string,
  tables: MySqlTableInfo[],
  colResult: MySqlQueryResult,
): string {
  const colsByTable = new Map<string, string[]>()
  colResult.rows.forEach((row) => {
    const table = row[0] ?? ''
    if (!table) return
    const nullable = row[3] === 'YES' ? 'NULL' : 'NOT NULL'
    const key = row[4] === 'PRI' ? ' PRI' : row[4] === 'UNI' ? ' UK' : row[4] === 'MUL' ? ' MUL' : ''
    const line = `${row[1] ?? ''}  ${row[2] ?? ''}  ${nullable}${key}${row[5] ? `  ${row[5]}` : ''}`
    const list = colsByTable.get(table)
    if (list) list.push(line)
    else colsByTable.set(table, [line])
  })
  const esc = (s: string): string =>
    s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  const sections = tables
    .map((t) => {
      const cols = colsByTable.get(t.name) ?? []
      const colRows = cols.map((c) => `<tr><td>${esc(c)}</td></tr>`).join('')
      return `<section class="tbl"><h2>${esc(t.name)}</h2>
<p class="meta">行数（预估）：${t.rows}　引擎：${esc(t.engine)}${t.comment ? `　注释：${esc(t.comment)}` : ''}</p>
${colRows ? `<table><thead><tr><th>列定义</th></tr></thead><tbody>${colRows}</tbody></table>` : '<p class="meta">（无列信息）</p>'}
</section>`
    })
    .join('\n')
  return `<!DOCTYPE html>
<html lang="zh-CN"><head><meta charset="utf-8"><title>数据库结构 - ${esc(dbName)}</title>
<style>
  body { font-family: "Microsoft YaHei", sans-serif; font-size: 12px; color: #222; margin: 24px; }
  h1 { font-size: 12px; margin: 0 0 4px; }
  .gen { color: #666; margin: 0 0 16px; }
  section.tbl { margin-bottom: 18px; page-break-inside: avoid; }
  h2 { font-size: 12px; margin: 0 0 4px; }
  p.meta { color: #555; margin: 0 0 6px; }
  table { border-collapse: collapse; width: 100%; }
  th, td { border: 1px solid #bbb; padding: 3px 8px; text-align: left; }
  th { background: #f0f0f0; }
</style></head><body>
<h1>数据库结构：${esc(dbName)}</h1>
<p class="gen">生成时间：${new Date().toLocaleString('zh-CN')}　共 ${tables.length} 张表</p>
${sections}
</body></html>`
}

/** 隐藏 iframe 写入 HTML 并调起打印（WebView2 内打印，不弹新窗口）；
 *  打印完成（afterprint）立即移除临时 iframe，避免打印对话框关后被隐藏框架滞留 */
function printHtml(html: string): void {
  const iframe = document.createElement('iframe')
  iframe.style.position = 'fixed'
  iframe.style.width = '0'
  iframe.style.height = '0'
  iframe.style.border = 'none'
  document.body.appendChild(iframe)
  const doc = iframe.contentDocument
  if (!doc) {
    document.body.removeChild(iframe)
    return
  }
  doc.open()
  doc.write(html)
  doc.close()
  iframe.contentWindow?.focus()
  // 打印完成后立即移除（afterprint 前端事件；兜底超时防 60s 滞留，移除幂等）
  const removeFrame = (): void => {
    if (iframe.parentNode === document.body) document.body.removeChild(iframe)
  }
  iframe.contentWindow?.addEventListener('afterprint', removeFrame)
  iframe.contentWindow?.print()
  window.setTimeout(removeFrame, 60_000)
}

/** 导入导出对话框（树右键"运行 SQL 文件"入口；connId 挂当前活动 MySQL 连接） */
const ioDialogVisible = ref(false)
const ioDialogConnId = ref('')
/** 转储 SQL 文件子菜单预选：true=转储结构和数据 / false=仅转储结构 */
const ioIncludeCreateTable = ref(false)

// ---------------- Tab 管理 ----------------

const tabs = ref<WorkTab[]>([])
const activeId = ref<string | null>(null)
const activeTab = computed(() => tabs.value.find((t) => t.id === activeId.value) ?? null)
/** SFTP 双栏远程栏路径（按 Tab id 持有）：打开时注入绑定终端 cwd，导航后继续跟随该 Tab */
const sftpRemotePaths = ref<Record<string, string>>({})

// ---------------- 地址栏 ----------------

/** 会话节点地址文本：user@host:port（Xshell 会话树风格，无 host 时用显示名兜底） */
function sessionAddress(n: SessionNode): string {
  const cfg = n.config
  if (!cfg?.host) return n.name
  const user = cfg.username ? `${cfg.username}@` : ''
  const port = cfg.port ? `:${cfg.port}` : ''
  return `${user}${cfg.host}${port}`
}

/** 当前活动标签的地址文本：会话终端显示地址，其他显示标签标题（Xshell 地址栏惯例） */
const activeAddress = computed(() => {
  const tab = activeTab.value
  if (!tab) return ''
  if (tab.type === 'terminal' && tab.sessionId) {
    const node = findNode(nodes.value, tab.sessionId)
    if (node) return sessionAddress(node)
  }
  return tab.title
})

/** 地址栏下拉会话列表：全量持久会话（不受搜索关键词与分区折叠影响） */
const addressBarSessions = computed<{ id: string; label: string }[]>(() => {
  const out: { id: string; label: string }[] = []
  const walk = (list: SessionNode[]): void => {
    for (const n of list) {
      if (n.is_folder) walk(n.children ?? [])
      else out.push({ id: n.id, label: sessionAddress(n) })
    }
  }
  walk(nodes.value)
  return out
})

/** 地址栏跳转：转扁平节点形状后复用左树连接路由（同会话 Tab 激活/按类型建连） */
function gotoSession(id: string): void {
  const node = findNode(nodes.value, id)
  if (!node) {
    ui.toast(`会话「${id}」不存在或已删除`, 'error')
    return
  }
  onNodeClick({
    id: node.id,
    name: node.config?.host || node.name,
    depth: 0,
    isFolder: false,
    color: node.config?.color ?? null,
    isOpen: false,
    hostLabel: sessionAddress(node),
    encoding: node.config?.encoding ?? null,
    isMysql: node.config?.session_type === 'mysql' || node.config?.session_type === 'redis',
  })
}

/** 解析地址栏输入，支持 Xshell 地址栏的四种格式：user@host:port / host:port / user@host / host */
function parseAddress(addr: string): { user?: string; host: string; port?: number } {
  let rest = addr
  let user: string | undefined
  const at = rest.indexOf('@')
  if (at >= 0) {
    user = rest.slice(0, at)
    rest = rest.slice(at + 1)
  }
  let host = rest
  let port: number | undefined
  const colon = rest.lastIndexOf(':')
  if (colon >= 0) {
    const p = Number(rest.slice(colon + 1))
    if (Number.isInteger(p) && p > 0 && p < 65536) {
      port = p
      host = rest.slice(0, colon)
    }
  }
  return { user, host, port }
}

/** 地址栏回车连接：解析 user@host:port 格式后宽松匹配已有会话（host 必须相同，
    user/port 若输入则须匹配）直接连；未命中（全新地址）打开会话表单预填主机——
    SSH 凭据必须由用户填写，会话表单是凭据入口（认证在连接阶段自动完成，无终端输密码路径） */
function connectAddress(addr: string): void {
  const key = addr.toLowerCase()
  // 完整串精确匹配（会话树副行地址/名称）
  const byLabel = addressBarSessions.value.find((s) => s.label.toLowerCase() === key)
  // 解析后宽松匹配：host 相同即命中，user/port 若输入则须匹配
  const parsed = parseAddress(addr)
  const byParsed = nodes.value.find((n) => {
    if (n.is_folder) return false
    if ((n.config?.host ?? '').toLowerCase() !== parsed.host.toLowerCase()) return false
    if (parsed.port != null && n.config?.port !== parsed.port) return false
    if (
      parsed.user != null &&
      (n.config?.username ?? '').toLowerCase() !== parsed.user.toLowerCase()
    )
      return false
    return true
  })
  const hit = byLabel ?? byParsed
  if (hit) {
    gotoSession(hit.id)
    return
  }
  quickConnect(addr)
}

/** 生成 Tab 唯一 ID（Tab ID 与会话 ID 无关） */
function genTabId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID()
  }
  return `tab-${Date.now()}-${Math.floor(Math.random() * 1e6)}`
}

/** 连接失败的终端 Tab（tabId → 错误原因）：失败后 Tab 保留，终端区就地展示「重试」入口（Xshell 惯例） */
const failedConns = ref(new Map<string, string>())
/** 重试中的 Tab（防重复点击重试） */
const retryingConns = ref(new Set<string>())
/** 失败 Tab 的重连闭包（tabId → connect）：就地重试按原会话参数重连 */
const retryHandlers = new Map<string, () => Promise<unknown>>()

/** 活动终端窗格 ref 表（tabId → TerminalPane，v-show 常驻仅活跃项被菜单操作命中）：
 * 菜单「编辑 → 复制/粘贴/全选」就地执行剪贴板操作 */
const paneRefs = new Map<string, InstanceType<typeof TerminalPane>>()

/** 模板函数 ref：挂载时注册，卸载（Tab 关闭）时注销 */
function setPaneRef(id: string, el: unknown): void {
  if (el) paneRefs.set(id, el as InstanceType<typeof TerminalPane>)
  else paneRefs.delete(id)
}

// Tab 激活时终端自动聚焦（Xshell 同款：新开/切换/复制会话后焦点直进终端，免手动点击）。
// 终端 Tab 用 v-show 保活，切回的窗格 DOM 已存在；nextTick 等 v-show 生效后再 focus。
// 连接失败分支窗格已卸载（paneRefs 无实例）自然跳过，重试成功重新挂载后再次激活会聚焦。
watch(activeId, async (id) => {
  const tab = id ? tabs.value.find((t) => t.id === id) : null
  if (!tab || tab.type !== 'terminal') return
  await nextTick()
  paneRefs.get(tab.id)?.focus()
})

/** 打开会话终端 Tab（Xshell 多标签：同一会话可重复开 Tab，每标签一条独立连接） */
function openTerminal(node: { id: string; name: string; color?: string | null }): void {
  // 标签色：会话节点自定义色优先，未设置时回退 SSH 选项默认标签色（sshopt_default_tab_color）
  const tabColor: string | null = node.color || sshOpts.defaultTabColor || null
  const tab: WorkTab = {
    id: genTabId(),
    type: 'terminal',
    sessionId: node.id,
    connId: genTabId(),
    title: node.name,
    color: tabColor,
  }
  tabs.value.push(tab)
  activeId.value = tab.id
  debugLog(`workspace openTerminal: tab=${tab.id} session=${node.id} tabs=${tabs.value.length}`)
  // 失败重试：按原会话参数重连（重试用全新 connId：旧连接已被 store 清理，TerminalPane 经 sessionId watch 重绑写入器）
  retryHandlers.set(tab.id, () => {
    const t = tabs.value.find((x) => x.id === tab.id)
    if (t) t.connId = genTabId()
    return terminalStore.openTerminal({ id: node.id, name: node.name, color: tabColor }, t?.connId ?? tab.connId)
  })
  runConnect(tab, `连接「${node.name}」`)
}

/**
 * 打开本地终端 Tab（byte-stream 终端，本地 ConPTY）：无需表单与持久会话，直接创建，
 * 连接路由键 = local-<tabId>，读写经 local_shell_* 命令路由（菜单栏「终端」入口）
 */
function openLocalTerminal(): void {
  const connKey = `local-${genTabId()}`
  // 标题序号：按已开本地终端数追加（首个「本地终端」，后续「本地终端 2」…，关闭后不重排）
  const localCount = tabs.value.filter((t) => t.type === 'terminal' && t.sessionId?.startsWith('local-')).length
  const tab: WorkTab = {
    id: genTabId(),
    type: 'terminal',
    sessionId: connKey,
    connId: connKey,
    title: localCount === 0 ? '本地终端' : `本地终端 ${localCount + 1}`,
    color: null,
  }
  tabs.value.push(tab)
  activeId.value = tab.id
  debugLog(`workspace openLocalTerminal: tab=${tab.id} conn=${connKey}`)
  // 失败重试：换全新 connKey 重连（旧连接已被 store 清理，TerminalPane 经 sessionId watch 重绑写入器）
  retryHandlers.set(tab.id, () => {
    const t = tabs.value.find((x) => x.id === tab.id)
    const nextKey = `local-${genTabId()}`
    if (t) {
      t.connId = nextKey
      t.sessionId = nextKey
    }
    return terminalStore.openTerminal({ id: nextKey, name: '本地终端', sessionType: 'local' }, nextKey)
  })
  runConnect(tab, '打开本地终端')
}

/** 双击首页/导航页空白处：打开本地终端（点在树节点/按钮/输入框等交互元素上不触发） */
function onBlankDblclick(e: MouseEvent): void {
  const t = e.target as HTMLElement
  if (t.closest('button, input, a, .workspace__tree-node, .workspace__search, .workspace__nav-header')) return
  // 浏览器双击默认选中一个词、高亮残留不美观（用户反馈「效果不好」）：清掉选中
  window.getSelection()?.removeAllRanges()
  openLocalTerminal()
}

/** 连接失败处理（Xshell 惯例：失败后终端 Tab 保留可改参数重连）：
 * 失败原因记入 failedConns（终端区就地展示「重试」入口），toast 提示 */
function runConnect(tab: WorkTab, action: string): void {
  const connect = retryHandlers.get(tab.id)
  if (!connect) return
  void connect().catch((e) => {
    const reason = friendlyError(e)
    failedConns.value.set(tab.id, reason)
    ui.toast(`${action}失败：${reason}`, 'error')
  })
}

/** 就地重试（终端区「重试」按钮）：按原会话参数重连，再次失败时原因就地更新（不打扰 toast） */
function retryConnection(tab: WorkTab): void {
  const connect = retryHandlers.get(tab.id)
  if (!connect || retryingConns.value.has(tab.id)) return
  retryingConns.value.add(tab.id)
  failedConns.value.delete(tab.id)
  void connect()
    .catch((e) => {
      failedConns.value.set(tab.id, friendlyError(e))
    })
    .finally(() => {
      retryingConns.value.delete(tab.id)
    })
}

/**
 * 打开 byte-stream 会话终端 Tab（Telnet / RLOGIN / 串口会话树的持久会话路径）：
 * 与 SSH 会话同语义——sessionId = 会话 id（双击已有 Tab 直接聚焦不重复建连），
 * connId = 每标签独立连接路由键（Xshell 多标签）。RLOGIN 复用 Telnet 传输
 * （Rust 侧零协议改动，仅端口默认 513），见 plans/calm-frolicking-duckling.md
 */
function openByteStreamSession(node: SessionNode): void {
  const cfg = (node.config ?? {}) as Record<string, unknown>
  const stype = (cfg.session_type as string | undefined) ?? 'telnet'
  const tabColor: string | null = (cfg.color as string | null) ?? sshOpts.defaultTabColor ?? null
  const tab: WorkTab = {
    id: genTabId(),
    type: 'terminal',
    sessionId: node.id,
    connId: genTabId(),
    title: node.name,
    color: tabColor,
  }
  tabs.value.push(tab)
  activeId.value = tab.id
  debugLog(`workspace openByteStreamSession: tab=${tab.id} type=${stype} conn=${tab.connId}`)
  // 失败重试：按原会话参数重连（重试用全新 connId，TerminalPane 经 sessionId watch 重绑写入器）
  retryHandlers.set(tab.id, () => {
    const t = tabs.value.find((x) => x.id === tab.id)
    if (t) t.connId = genTabId()
    return terminalStore.openTerminal(
      {
        id: node.id,
        name: node.name,
        color: tabColor,
        // RLOGIN 无独立后端：路由为 Telnet（端口 513 即 RLOGIN 服务）
        sessionType: stype === 'rlogin' ? 'telnet' : (stype as 'telnet' | 'serial'),
        host: cfg.host as string | undefined,
        port: cfg.port as number | undefined,
        serialPort: cfg.serial_port as string | undefined,
        baudRate: cfg.baud_rate as number | undefined,
      },
      t?.connId ?? tab.connId,
    )
  })
  runConnect(tab, `连接「${node.name}」`)
}

/** 打开传输队列 Tab（单例） */
function openTransferTab(): void {
  const existing = tabs.value.find((t) => t.type === 'transfer')
  if (existing) {
    activeId.value = existing.id
    return
  }
  const tab: WorkTab = { id: genTabId(), type: 'transfer', title: '传输队列' }
  tabs.value.push(tab)
  activeId.value = tab.id
}

/** 打开 SFTP 双栏 Tab（单例，P0）：本地/远程双栏文件传输（绑定最近终端会话，独立 SFTP 会话 Tab 不参与单例判定）。
    远程栏初始路径 = 绑定 SSH 终端的实时 cwd（终端 OSC 7 追踪，无记录则留空走 DualPane 内部默认） */
function openSftpTab(): void {
  const existing = tabs.value.find((t) => t.type === 'sftp' && !t.sessionId)
  const boundKey = lastTerminalSession.value?.id
  // 单例命中：重新同步绑定终端最新 cwd 再激活（终端可能已 cd 别处；原实现只激活不更新，
  // 是"快捷打开窗口依然没和 ssh 所在目录一致"的主要复现路径）。路径为空保留现值不覆盖
  if (existing) {
    if (boundKey) {
      const cwd = terminalStore.getSessionCwd(boundKey)
      if (cwd) sftpRemotePaths.value[existing.id] = cwd
    }
    activeId.value = existing.id
    return
  }
  const tab: WorkTab = { id: genTabId(), type: 'sftp', title: 'SFTP 文件传输' }
  sftpRemotePaths.value[tab.id] = boundKey ? terminalStore.getSessionCwd(boundKey) : ''
  // 迟到 cwd 兜底：初始 cwd 为空（终端尚无 OSC 7 上报，如刚换主机的会话）时，
  // 挂一次性 watcher 等 sessionCwd 到位——注入的 PROMPT_COMMAND 会在首个提示符前上报；
  // 5s 未到位即放弃，保持 DualPane 内部 '/' 兜底。
  if (boundKey && !sftpRemotePaths.value[tab.id]) {
    const stop = watch(
      () => terminalStore.getSessionCwd(boundKey),
      (cwd) => {
        if (cwd) {
          sftpRemotePaths.value[tab.id] = cwd
          stop()
        }
      },
    )
    setTimeout(() => stop(), 5000)
  }
  tabs.value.push(tab)
  activeId.value = tab.id
}

/**
 * 打开独立 SFTP 会话（树节点双击）：
 * 建 SSH 连接（SFTP 子系统基于 SSH channel）后直接打开 SFTP 双栏 Tab。
 * Xshell 多开惯例：每会话每次打开都新建独立连接 Tab（connId 为连接路由键）。
 * 失败：移除 Tab 并 toast（connectSession 的 catch 已把连接状态置 disconnected）
 */
async function openSftpSession(node: { id: string; name: string; color?: string | null }): Promise<void> {
  const tabColor: string | null = node.color || sshOpts.defaultTabColor || null
  const tab: WorkTab = {
    id: genTabId(),
    type: 'sftp',
    sessionId: node.id,
    connId: genTabId(),
    title: node.name,
    color: tabColor,
  }
  tabs.value.push(tab)
  activeId.value = tab.id
  try {
    // sessionType 缺省 'ssh' → connectSession 萰到 sshConnect 路由（后端按会话配置建连并注册 ssh_sessions），
    // 输出无窗格写入器被安全丢弃；断开按记录的类型路由 sshDisconnect
    await terminalStore.connectSession({ id: node.id, name: node.name, color: tabColor }, tab.connId)
  } catch (e) {
    tabs.value = tabs.value.filter((t) => t.id !== tab.id)
    if (activeId.value === tab.id) {
      activeId.value = tabs.value.at(-1)?.id ?? null
    }
    ui.toast(`连接「${node.name}」失败：${friendlyError(e)}`, 'error')
  }
}

/**
 * 按会话类型统一分发打开（树点击/右键打开连接/Ctrl+T/保存后/会话列表/复制会话共用的路由）：
 * mysql/redis → 各自工作台，telnet/rlogin/serial → byte-stream 终端，
 * sftp → 独立 SFTP 双栏 Tab，缺省/ssh → 终端
 */
function openSessionByType(target: SessionNode): void {
  const stype = target.config?.session_type
  if (stype === 'mysql') {
    void connectMysqlSession(target)
    return
  }
  if (stype === 'redis') {
    void connectRedisSession(target)
    return
  }
  if (stype === 'telnet' || stype === 'rlogin' || stype === 'serial') {
    openByteStreamSession(target)
    return
  }
  if (stype === 'sftp') {
    void openSftpSession(target)
    return
  }
  openTerminal(target)
}

/** 打开 SSH 隧道 Tab（单例，P1） */
function openTunnelTab(): void {
  const existing = tabs.value.find((t) => t.type === 'tunnel')
  if (existing) {
    activeId.value = existing.id
    return
  }
  const tab: WorkTab = { id: genTabId(), type: 'tunnel', title: 'SSH 隧道' }
  tabs.value.push(tab)
  activeId.value = tab.id
}

/** 打开快捷命令 Tab（单例，P1 Compose Pane） */
function openComposeTab(): void {
  const existing = tabs.value.find((t) => t.type === 'compose')
  if (existing) {
    activeId.value = existing.id
    return
  }
  const tab: WorkTab = { id: genTabId(), type: 'compose', title: '快捷命令' }
  tabs.value.push(tab)
  activeId.value = tab.id
}

/** 打开 MySQL Tab（单例，P1） */
function openMysqlTab(): void {
  const existing = tabs.value.find((t) => t.type === 'mysql')
  if (existing) {
    activeId.value = existing.id
    return
  }
  const tab: WorkTab = { id: genTabId(), type: 'mysql', title: 'MySQL' }
  tabs.value.push(tab)
  activeId.value = tab.id
}

/** 打开 Redis Tab（单例，与 MySQL 同模式） */
function openRedisTab(): void {
  const existing = tabs.value.find((t) => t.type === 'redis')
  if (existing) {
    activeId.value = existing.id
    return
  }
  const tab: WorkTab = { id: genTabId(), type: 'redis', title: 'Redis' }
  tabs.value.push(tab)
  activeId.value = tab.id
}

/** Ctrl+T / 加号：优先打开当前选中会话的终端；未选会话时默认打开本地终端 */
function onCreate(): void {
  if (selectedId.value) {
    const node = findNode(nodes.value, selectedId.value)
    if (node && !node.is_folder) {
      openSessionByType(node)
      return
    }
  }
  // 未选会话：默认打开本地终端（不再提示先选会话）
  openLocalTerminal()
}

/** 关闭 Tab；终端 Tab 委托 terminalStore 断开会话并清理 Rust 侧 session/PTY（架构红线） */
function closeTab(id: string): void {
  const idx = tabs.value.findIndex((t) => t.id === id)
  if (idx < 0) return
  const [closed] = tabs.value.splice(idx, 1)
  if (closed?.type === 'terminal' && closed.connId) {
    // store 侧同步断开连接、清理标签与状态（按连接键路由，含未建立连接的兜底）
    void terminalStore.closeBySessionId(closed.connId)
    // 同步清理失败状态与重连闭包（Tab 已关闭，就地重试入口随之消失）
    failedConns.value.delete(closed.id)
    retryHandlers.delete(closed.id)
  }
  // 独立 SFTP 会话 Tab 关闭即断开（连接键不命中终端 Tab → 直接 sshDisconnect 清理 ssh_sessions）
  if (closed?.type === 'sftp' && closed.connId) {
    void terminalStore.closeBySessionId(closed.connId)
  }
  // 清理 SFTP Tab 的远程栏路径记录（按 Tab id 持有，关闭即释放）
  delete sftpRemotePaths.value[id]
  // MySQL/Redis 工作台 Tab 关闭即断开（与右键"关闭连接"行为一致，驱动树中历史库灰化）
  if (closed?.type === 'mysql') {
    void useMysqlStore().disconnect()
  }
  if (closed?.type === 'redis') {
    void useRedisStore().disconnect()
  }
  if (activeId.value === id) {
    const next = tabs.value[Math.min(idx, tabs.value.length - 1)]
    activeId.value = next?.id ?? null
  }
}

/** 拖拽排序：按 FlexTabs 返回的新 id 顺序重排 */
function onReorder(ids: string[]): void {
  const byId = new Map(tabs.value.map((t) => [t.id, t]))
  tabs.value = ids
    .map((id) => byId.get(id))
    .filter((t): t is WorkTab => !!t)
}

/** 右键菜单重命名：更新工作区 Tab 标题（FlexTabs 仅传 tab id，标题源在本视图），并同步 store 内标签标题 */
function onRenameTab(id: string, title: string): void {
  const tab = tabs.value.find((t) => t.id === id)
  if (!tab) return
  tab.title = title
  if (tab.type === 'terminal' && tab.connId) {
    terminalStore.renameBySessionId(tab.connId, title)
  }
}

/** 标签拖出（P1）：终端 Tab 拖出标签栏时，在新窗口打开该会话终端。
 *  原 Tab 关闭并断开连接（避免双窗口争用同一 SSH session） */
function onDragOut(tabId: string): void {
  const tab = tabs.value.find((t) => t.id === tabId)
  if (!tab) return
  if (tab.type === 'terminal' && tab.sessionId) {
    closeTab(tabId)
    void openSessionWindow({ sessionId: tab.sessionId, title: tab.title })
  } else {
    ui.toast('该标签不支持拖出新窗口', 'info')
  }
}

/** Ctrl+Tab：循环切换 */
function cycleTab(): void {
  if (tabs.value.length < 2) return
  const idx = tabs.value.findIndex((t) => t.id === activeId.value)
  const next = tabs.value[(idx + 1) % tabs.value.length]
  activeId.value = next.id
}

/** FlexTabs 渲染数据（按会话 color 着色；连接成功瞬间闪绿，1.5s 后恢复原色） */
/** Tab 类型 → 图标名（全部已在 lucide.ts mdiMap 登记，标签栏辨识会话类型） */
const tabIconByType: Record<WorkTab['type'], string> = {
  terminal: 'mdi-console',
  transfer: 'mdi-arrow-left-right',
  tunnel: 'mdi-lan-connect',
  sftp: 'mdi-folder-open',
  compose: 'mdi-flash-outline',
  mysql: 'mdi-database',
  redis: 'mdi-database',
  mysqlcli: 'mdi-console-line',
  query: 'mdi-code-tags',
}

const tabItems = computed<FlexTabItem[]>(() =>
  tabs.value.map((t) => ({
    id: t.id,
    title: t.title,
    icon: tabIconByType[t.type],
    color: t.connId && terminalStore.sessionFlash[t.connId] ? 'rgb(var(--v-theme-success))' : t.color ?? undefined,
    closable: true,
    duplicatable: t.type === 'terminal' && !!t.sessionId && !t.sessionId.startsWith('local-'),
  })),
)

// ---------------- 状态栏数据（低频事件驱动） ----------------

/** 各会话连接状态：id -> status */
const connStatus = ref(new Map<string, string>())
/** 各传输任务状态：taskId -> status */
const transferStatus = ref(new Map<string, string>())

const activeSessionStatus = computed(() => {
  const key = activeTab.value?.connId ?? activeTab.value?.sessionId
  return key ? (connStatus.value.get(key) ?? null) : null
})

/** 工具栏"字体/编码/配色"可见态：活动 Tab 为已连接的 SSH 终端时出现，无连接/断开隐藏（本地终端不算 SSH 连接） */
const sshQuickToggles = computed(() => {
  const tab = activeTab.value
  if (!tab || tab.type !== 'terminal') return false
  if (!tab.sessionId || tab.sessionId.startsWith('local-')) return false
  return connStatus.value.get(tab.connId ?? tab.sessionId) === 'connected'
})

/** SFTP 工具可见态（传输/传文件按钮）：任一 SSH 终端已连接（耦合 SFTP 可用）
    或任一独立 SFTP 会话 Tab 已连接；全部断开隐藏（与激活 Tab 无关） */
const sftpTools = computed(() => {
  const anySshTerminal = tabs.value.some((t) => {
    if (t.type !== 'terminal' || !t.connId || !t.sessionId || t.sessionId.startsWith('local-')) return false
    const stype = findNode(nodes.value, t.sessionId)?.config?.session_type
    // null/undefined/'ssh' = SSH 会话（SFTP 能力）；数据库与 byte-stream 会话不含 SFTP
    return (stype == null || stype === 'ssh' || stype === 'sftp')
      && connStatus.value.get(t.connId) === 'connected'
  })
  const anySftpSession = tabs.value.some(
    (t) => t.type === 'sftp' && !!t.connId && connStatus.value.get(t.connId) === 'connected',
  )
  return anySshTerminal || anySftpSession
})

const activeSessionName = computed(() => activeTab.value?.title ?? '')

/** 活动会话主机地址（状态栏展示，取自会话配置） */
const activeSessionHost = computed(() => {
  const sessionId = activeTab.value?.sessionId
  if (!sessionId) return ''
  const node = findNode(nodes.value, sessionId)
  return (node?.config?.host as string | undefined) ?? ''
})

/** 活动会话编码（状态栏展示） */
const activeSessionEncoding = computed(() => {
  const sessionId = activeTab.value?.sessionId
  if (!sessionId) return ''
  const node = findNode(nodes.value, sessionId)
  return (node?.config?.encoding as string | undefined) ?? ''
})

/** 终端路径联动（P0 SFTP 部分接入后由终端/传输 Tab 更新） */
const currentPath = ref('')

const transferCount = computed(() => {
  let n = 0
  transferStatus.value.forEach((status) => {
    if (status === 'Running' || status === 'Queued') n += 1
  })
  return n
})

async function loadTransferSnapshot(): Promise<void> {
  try {
    const list = await transferList()
    const map = new Map<string, string>()
    for (const task of list as { id: string; status: string }[]) {
      map.set(task.id, task.status)
    }
    transferStatus.value = map
  } catch {
    /* 快照失败不阻塞 UI */
  }
}

// ---------------- 菜单栏 / 工具栏 / 快速连接 ----------------

/** 会话表单对话框（editingSession 为 null 表示新建） */
const showSessionForm = ref(false)
const editingSession = ref<SessionConfig | null>(null)
const presetHost = ref('')
/** 新建会话预设的所属文件夹 id（根级新建 = null；文件夹右键「在此新建会话…」设置） */
const newFolderPreset = ref<string | null>(null)
/** 新建会话预设为 SFTP 类型（工具栏「传文件」→「新建 SFTP 会话…」，配合独立会话启动） */
const presetSftp = ref(false)
/** 编辑已保存连接标记（menuRename 已存连接分支设置）：保存走 saveHandler 覆盖链路而非 session_save */
const editingSavedConn = ref<{ mysql: boolean; id: string } | null>(null)

function openSessionForm(): void {
  editingSession.value = null
  editingSavedConn.value = null
  presetHost.value = ''
  newFolderPreset.value = null
  presetSftp.value = false
  showSessionForm.value = true
}

/** 快速连接：打开会话表单并预填主机（Xshell 快速连接流程） */
function quickConnect(host: string): void {
  editingSession.value = null
  editingSavedConn.value = null
  presetHost.value = host
  newFolderPreset.value = null
  presetSftp.value = false
  showSessionForm.value = true
}

/** 新建独立 SFTP 会话（工具栏「传文件」→「新建 SFTP 会话…」）：打开新建表单并预设类型=SFTP，
    保存后由 openSessionByType（sftp 分支）以独立连接启动独立双栏 Tab */
function openNewSftpSession(): void {
  editingSession.value = null
  editingSavedConn.value = null
  presetHost.value = ''
  newFolderPreset.value = null
  presetSftp.value = true
  showSessionForm.value = true
}

/** 右键「在此新建会话…」：预设所属文件夹并打开新建会话表单（组树后文件夹归类闭环入口） */
function menuNewSessionInFolder(): void {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node?.isFolder) return
  editingSession.value = null
  editingSavedConn.value = null
  presetHost.value = ''
  newFolderPreset.value = node.id
  presetSftp.value = false
  showSessionForm.value = true
}

/** 右键「在此新建子文件夹…」：预设父文件夹打开新建文件夹对话框（支持任意层级嵌套） */
function menuNewSubFolder(): void {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node?.isFolder) return
  editingFolderId.value = null
  editingFolderParent.value = node.id
  folderName.value = ''
  showFolderDialog.value = true
}

/** 会话表单「所属文件夹」候选：组树结果按树序递归拍平（缩进表达层级；根级由控件的清空表达） */
const folderOptions = computed(() => {
  const out: { value: string; title: string }[] = []
  const walk = (list: SessionNode[], depth: number): void => {
    for (const n of list) {
      if (n.is_folder) {
        out.push({ value: n.id, title: `${'　'.repeat(depth)}${n.name}` })
        walk(n.children ?? [], depth + 1)
      }
    }
  }
  walk(nodes.value, 0)
  return out
})

/** 会话保存成功（新建/编辑/快速连接）：刷新树并按类型打开（sftp 会话开独立 SFTP 双栏 Tab） */
async function onSessionSaved(config: SessionConfig): Promise<void> {
  // 编辑已保存连接：树已在 saveHandler 链路刷新，仅提示（config.id 非会话 id，不走打开流程）
  if (editingSavedConn.value) {
    editingSavedConn.value = null
    ui.toast('连接已更新', 'success')
    return
  }
  await loadTree()
  const node = findNode(nodes.value, config.id)
  if (node && !node.is_folder) {
    openSessionByType(node)
    return
  }
  openTerminal({ id: config.id, name: config.name, color: config.color })
}

/** 编辑已保存连接的保存入口（SessionForm saveHandler 覆盖）：原地更新保存列表（按 id）并刷新树 */
async function saveSavedConn(cfg: SessionConfig): Promise<SessionConfig> {
  const marker = editingSavedConn.value
  if (!marker) return cfg
  const auth = cfg.auth_type
  const password =
    auth && (auth.type === 'password' || auth.type === 'interactive') ? (auth.password ?? '') : ''
  if (marker.mysql) {
    await useMysqlStore().updateConnection(marker.id, {
      name: cfg.name,
      host: cfg.host,
      port: cfg.port,
      username: cfg.username,
      password,
    })
  } else {
    await useRedisStore().updateConnection(marker.id, {
      name: cfg.name,
      host: cfg.host,
      port: cfg.port,
      // Redis 空串归一化为 null（与 connectSaved 建连语义一致）
      username: cfg.username?.trim() ? cfg.username.trim() : null,
      password: password || null,
    })
  }
  await loadTree()
  return cfg
}

/** 新建文件夹（P0：新建在根级） / 重命名文件夹（右键菜单）对话框共用状态 */
const showFolderDialog = ref(false)
const folderName = ref('')
/** 编辑态：null = 新建文件夹；否则为被重命名的文件夹 id */
const editingFolderId = ref<string | null>(null)
/** 重命名时保留的原父文件夹 id（防改名后被误移到根级） */
const editingFolderParent = ref<string | null>(null)
/** 对话框标题（新建/重命名共用一套输入框，按编辑态切换文案） */
const folderDialogTitle = computed(() => (editingFolderId.value ? '重命名文件夹' : '新建文件夹'))

/** 打开「新建文件夹」对话框（ToolBar/MenuBar/树按钮入口，重置为新建编辑态） */
function openNewFolderDialog(): void {
  editingFolderId.value = null
  editingFolderParent.value = null
  folderName.value = ''
  showFolderDialog.value = true
}

// ---------------- 文件菜单"打开"：会话列表对话框（SessionListDialog） ----------------

const showSessionListDialog = ref(false)

/** 扁平化的已添加会话列表（非文件夹节点）+ 数据库已存连接（MySQL/Redis store，树中 savedconn 常驻节点同源） */
const sessionListFlat = computed(() => {
  const out: SessionNode[] = []
  const walkList = (list: SessionNode[]) => {
    for (const n of list) {
      if (n.is_folder) walkList(n.children ?? [])
      else out.push(n)
    }
  }
  walkList(nodes.value)
  // 数据库已存连接：与树中数据库分区同源（session 树已有同 host 的 MySQL 会话则去重）
  const sessionHosts = new Set<string>()
  for (const n of nodes.value) {
    if (!n.is_folder && n.config?.session_type === 'mysql' && n.config.host) {
      sessionHosts.add(n.config.host)
    }
  }
  for (const c of useMysqlStore().savedConnections) {
    if (sessionHosts.has(c.host)) continue
    out.push({
      id: `savedconn-${c.id}`,
      name: c.name,
      is_folder: false,
      children: [],
      config: { host: c.host, port: c.port, username: c.username, session_type: 'mysql' },
    } as unknown as SessionNode)
  }
  for (const c of useRedisStore().savedConnections) {
    out.push({
      id: `savedconn-${c.id}`,
      name: c.name,
      is_folder: false,
      children: [],
      config: { host: c.host, port: c.port, username: c.username, session_type: 'redis' },
    } as unknown as SessionNode)
  }
  return out
})

/** 打开会话列表中选中的会话终端 */
function openSessionFromList(target: { id: string; name: string }): void {
  showSessionListDialog.value = false
  selectedId.value = target.id
  // 数据库已存连接：打开对应工作台并按保存配置建连（与树中 savedconn 节点单击一致）
  if (target.id.startsWith('savedconn-')) {
    const savedId = target.id.slice('savedconn-'.length)
    if (useMysqlStore().savedConnections.some((c) => c.id === savedId)) {
      void connectMysqlSaved(savedId)
    } else {
      openRedisTab()
      void useRedisStore().connectSaved(savedId)
    }
    return
  }
  const node = findNode(nodes.value, target.id)
  if (node && !node.is_folder) {
    openSessionByType(node)
    return
  }
  openTerminal(target)
}

/** 列表"属性"：打开会话选项对话框（会话模式，编辑写会话级键） */
function openListProperties(node: { id: string; name: string }): void {
  openSessionSettingsFor({ sessionId: node.id, title: node.name })
}

/** 列表"删除会话"：二次确认后删除（含 Rust 侧清理） */
async function deleteFromList(node: { id: string; name: string } | null): Promise<void> {
  if (!node) return
  const ok = await ui.confirm({
    title: '删除确认',
    message: `确定删除会话「${node.name}」吗？此操作不可恢复。`,
    danger: true,
  })
  if (!ok) return
  try {
    await sessionStore.remove(node.id, false)
    ui.toast(`已删除会话「${node.name}」`, 'success')
    await loadTree()
  } catch (e) {
    ui.toast(`删除失败：${friendlyError(e)}`, 'error')
  }
}

async function createFolder(): Promise<void> {
  const name = folderName.value.trim()
  if (!name) return
  const renaming = editingFolderId.value !== null
  try {
    // 重命名：传原 id + 原父级 → 后端 folder_save 按 id upsert 原地改名（保留位置）；
    // 新建（根级/子文件夹）：随机 id + 预设父级（根级入口父为 null）
    await sessionStore.saveFolder({
      id: renaming ? editingFolderId.value! : crypto.randomUUID(),
      name,
      parent_id: editingFolderParent.value,
    })
    folderName.value = ''
    editingFolderId.value = null
    editingFolderParent.value = null
    showFolderDialog.value = false
    await loadTree()
  } catch (e) {
    ui.toast(`保存文件夹失败：${friendlyError(e)}`, 'error')
  }
}

/** 断开当前活动会话（Xshell 惯例：断开连接保留标签，终端留在灰态可回看缓冲区） */
function disconnectActive(): void {
  const tab = tabs.value.find((t) => t.id === activeId.value)
  if (!tab?.connId) return
  const key = tab.connId
  // 先置灰再断开：覆盖连接进行中点断开的竞态（后端尚未注册句柄、无 disconnected 事件回流，
  // connectSession 的 M2 守卫靠此状态拦截连接完成后的复活回写）；SFTP 独立会话 Tab 同理。
  // store sessionStatus 驱动树状态点/标签，本地 connStatus 驱动底部状态条，两处同步置灰
  terminalStore.sessionStatus[key] = 'disconnected'
  connStatus.value = new Map(connStatus.value).set(key, 'disconnected')
  void terminalStore.disconnectSession(key)
}

// ---------------- 日志（P1） ----------------

/** 会话日志查看器开关 */
const showLogViewer = ref(false)
/** 主密码设置对话框开关（首次设置 + 修改/校验二合一） */
const showMasterPassword = ref(false)
/** 快捷键速查对话框（帮助菜单"快捷键列表…"入口） */
const showShortcutList = ref(false)
/** 高功能设置对话框开关 + 默认分区（选项菜单"设置"/帮助菜单"关于"入口） */
const showSettings = ref(false)
const settingsSection = ref<'appearance' | 'terminal' | 'sftp' | 'data' | 'security' | 'about'>(
  'appearance',
)

/** SSH 选项对话框开关（标签右键菜单/菜单栏"会话设置"入口） */
const showSshOptionsDialog = ref(false)

/** 菜单栏组件引用（Alt+字母菜单导航经 openMenu 程序化打开对应菜单） */
const menuBarRef = ref<InstanceType<typeof MenuBar> | null>(null)

/** 窗口菜单动态标签列表：全部已开标签 + 当前激活勾选（Xshell 窗口菜单惯例） */
const windowMenuTabs = computed(() =>
  tabs.value.map((t) => ({ id: t.id, title: t.title, active: t.id === activeId.value })),
)

/** 窗口菜单标签跳转：切换激活标签 */
function onMenuTabAction(id: string): void {
  activeId.value = id
}

/** 工具栏"字体"面板字号切换：改终端默认字号（实时生效到已打开终端并持久化） */
function onToolbarFont(size: number): void {
  settings.setTerminalFontSize(size)
}

/** 工具栏"字体"面板字体家族切换：改终端默认字体（useXterm watch 联动） */
function onToolbarFontFamily(family: string): void {
  settings.setTerminalFontFamily(family)
}

/** 工具栏"字体"面板字体样式切换：常规/粗体/斜体（useXterm watch 联动） */
function onToolbarFontStyle(style: string): void {
  settings.setTerminalFontStyle(style as FontFamilyStyle)
}

/** 工具栏"编码"快捷切换：改活动会话编码（保存后树刷新，终端 watch 联动重解码） */
async function onToolbarEncoding(encoding: string): Promise<void> {
  const sessionId = activeTab.value?.sessionId
  if (!sessionId || sessionId.startsWith('local-')) {
    ui.toast('本地终端无会话编码，请先连接一个会话', 'warning')
    return
  }
  const cfg = sessionStore.getSessionById(sessionId)
  if (!cfg || cfg.encoding === encoding) return
  await sessionStore.save({ ...cfg, encoding })
  await loadTree() // 本地树刷新后 activeSessionEncoding/状态栏与终端 watch 联动
  ui.toast(`编码已切换为 ${encoding}`)
}

/** 配色方案对话框开关（工具栏"配色"按钮） */
const showColorScheme = ref(false)

/** 会话设置目标：会话节点 id与会话名（会话模式，编辑写会话级键覆盖全局值） */
const sshOptionsSessionId = ref<string | null>(null)
const sshOptionsSessionName = ref('')
/** 本地终端（conn 键 local-<id>）打开时选项对话框只留终端/外观组（无 SSH 连接层） */
const sshOptionsLocalOnly = ref(false)

/** 打开会话选项对话框（会话模式）；无目标会话（非终端 Tab）时 toast 提示 */
function openSessionSettingsFor(tab: { sessionId?: string; title: string } | null): void {
  if (!tab || !tab.sessionId) {
    ui.toast('请先选择一个会话终端', 'warning')
    return
  }
  sshOptionsLocalOnly.value = tab.sessionId.startsWith('local-')
  sshOptionsSessionId.value = tab.sessionId
  sshOptionsSessionName.value = tab.title
  showSshOptionsDialog.value = true
}

/** 标签右键菜单"会话设置"：按目标 Tab 解析会话并打开 */
function onSessionSettings(tabId: string): void {
  openSessionSettingsFor(tabs.value.find((t) => t.id === tabId) ?? null)
}

/** 标签右键菜单"复制会话"：同一会话再开一个 Tab（每标签一条独立连接，Xshell 多标签惯例） */
function onDuplicateSession(tabId: string): void {
  const tab = tabs.value.find((t) => t.id === tabId)
  if (!tab) return
  if (tab.type === 'sftp' && tab.sessionId) {
    // 独立 SFTP 会话 Tab：复制即再开一个同会话的 SFTP 双栏 Tab
    const node = findNode(nodes.value, tab.sessionId)
    if (node && !node.is_folder) {
      void openSftpSession(node)
      return
    }
  }
  if (tab.type === 'terminal' && tab.sessionId) {
    const node = findNode(nodes.value, tab.sessionId)
    if (node) {
      openSessionByType(node)
      return
    }
  }
  ui.toast('该标签不支持复制会话', 'info')
}

/** 活动终端 Tab 的连接路由键（监控/快速命令/日志均按连接键路由；无终端 Tab 时为 null） */
/** 当前活动终端 Tab 的会话 id（日志按会话 id 落盘/查询；本地终端无持久会话，其值即路由键） */
const activeTerminalId = computed(() => {
  const tab = activeTab.value
  return tab?.type === 'terminal' && tab.sessionId ? tab.sessionId : null
})

/** 最近一个可作 SFTP 远程栏的终端会话：优先当前活动终端 Tab（用户在哪个终端，快捷 SFTP 就绑定它，
    其最新 cwd 即用户期望的远程栏初始目录），兜底倒序找已连接的 SSH 终端。
    本地/Telnet/串口与断开会话无 SFTP 能力排除——断开后自动回退到更早的已连接会话。
    修复：原实现仅按标签序取"最后打开的 SSH 终端"，对当前活动终端 cd 后点快捷 SFTP 会绑定到
    其他标签的陈旧 cwd，导致目录对不上（快捷打开窗口与 SSH 所在目录不一致）。 */
const lastTerminalSession = computed<{ id: string; name: string } | null>(() => {
  const terminalOf = (t: {
    title: string
    panes?: { sessionId: string }[]
  }): { id: string; name: string } | null => {
    const sid = t?.panes?.[0]?.sessionId
    if (!sid || sid.startsWith('local-')) return null
    if (terminalStore.isConnected(sid) && terminalStore.sessionTypeOf(sid) === 'ssh') {
      return { id: sid, name: t.title }
    }
    return null
  }
  // 活动终端优先：当前正看着的终端 cwd 就是快捷 SFTP 应镜像的目录
  const activeTab0 = activeTab.value
  if (activeTab0?.type === 'terminal' && activeTab0.connId) {
    const hit = terminalOf({ title: activeTab0.title, panes: [{ sessionId: activeTab0.connId }] })
    if (hit) return hit
  }
  const tabs = terminalStore.tabs
  for (let i = tabs.length - 1; i >= 0; i--) {
    const hit = terminalOf(tabs[i])
    if (hit) return hit
  }
  return null
})

/** SFTP 双栏远程栏会话键：独立 SFTP 会话 Tab 连接就绪后用自身连接键（未就绪为空串，
    FilePane 提示选择会话且连接完成时 sessionId 变化触发重载）；工具栏单例 Tab 回退
    最近可用 SSH 会话键（现有 SSH 耦合不变） */
function sftpPaneSessionId(tab: WorkTab): string {
  if (!tab.connId) return lastTerminalSession.value?.id ?? ''
  return terminalStore.isConnected(tab.connId) ? tab.connId : ''
}

const lastTerminalSessionName = computed(() => lastTerminalSession.value?.name ?? '')

/** 快捷命令发送完成（Compose Pane）：汇总成败 toast */
function onComposeSent(payload: { commandText: string; sessionIds: string[] }): void {
  const n = payload.sessionIds.length
  ui.toast(n > 0 ? `已发送到 ${n} 个会话` : '没有已连接的目标会话', n > 0 ? 'success' : 'warning')
}

/** 搜索：展开导航并聚焦搜索框 */
const searchRef = ref<{ focus: () => void } | null>(null)

function focusSearch(): void {
  ui.navCollapsed = false
  searchRef.value?.focus()
}

/** 清空搜索（清空按钮 / Esc） */
function clearSearch(): void {
  keyword.value = ''
  searchRef.value?.focus()
}

/** 菜单栏动作接线 */
async function onMenuAction(action: string): Promise<void> {
  switch (action) {
    case 'new-session':
      openSessionForm()
      break
    case 'new-folder':
      openNewFolderDialog()
      break
    case 'open-session-list':
      showSessionListDialog.value = true
      break
    case 'local-terminal':
      openLocalTerminal()
      break
    case 'quit':
      // 关闭窗口（当前版本关闭即隐藏到托盘）
      await getCurrentWindow().close()
      break
    case 'copy':
    case 'cut':
    case 'paste':
    case 'select-all': {
      // 真实执行（Xshell 菜单肌肉记忆）：活动终端窗格存在时执行剪贴板操作，无终端时提示快捷键
      const pane = paneRefs.get(activeId.value ?? '')
      if (!pane) {
        ui.toast('终端内 Ctrl+C 复制 / Ctrl+V 粘贴 / Ctrl+A 全选', 'info')
        break
      }
      if (action === 'copy' || action === 'cut') {
        // 复制/剪切：选中内容写入剪贴板（终端选区无删除语义，剪切即复制），无选中时提示
        const copied = await pane.copySelection()
        if (!copied) ui.toast('当前无选中内容', 'info')
      } else if (action === 'paste') {
        await pane.pasteFromClipboard()
      } else {
        pane.selectAll()
      }
      break
    }
    case 'toggle-nav':
      ui.navCollapsed = !ui.navCollapsed
      break
    case 'toggle-quickbar':
      ui.quickBarVisible = !ui.quickBarVisible
      break
    case 'toggle-composebar':
      ui.composeBarVisible = !ui.composeBarVisible
      break
    case 'transfer':
      openTransferTab()
      break
    case 'sftp':
      openSftpTab()
      break
    case 'quick-command':
      openComposeTab()
      break
    case 'tunnel':
      openTunnelTab()
      break
    case 'mysql':
      openMysqlTab()
      break
    case 'redis':
      openRedisTab()
      break
    case 'session-log':
      if (activeTerminalId.value) {
        showLogViewer.value = true
      } else {
        ui.toast('请先选择一个已连接的会话终端', 'warning')
      }
      break
    case 'master-password':
      showMasterPassword.value = true
      break
    case 'shortcut-list':
      showShortcutList.value = true
      break
    case 'next-tab':
      cycleTab()
      break
    case 'settings':
      settingsSection.value = 'appearance'
      showSettings.value = true
      break
    case 'about':
      // 关于：打开设置对话框的"关于"分区（含版本与技术栈说明）
      settingsSection.value = 'about'
      showSettings.value = true
      break
    case 'check-update':
      // 检测更新：调用 updater 插件，按结果提示（无更新/有更新/网络错误）
      runCheckUpdate()
      break
  }
}

// 终端内键位映射触发的菜单命令（经 ui store 分发，seq 递增支持连续触发）
watch(
  () => ui.menuActionRequest,
  ({ action }) => {
    if (action) void onMenuAction(action)
  },
)

// ---------------- 更新检测 ----------------

/** 检测更新：无更新提示最新版本；有更新确认后下载安装并重启；网络错误 toast 提示。
 *  silent（启动自动检测）时仅在有更新时弹确认，无更新/网络错误静默不提示 */
async function runCheckUpdate(silent = false): Promise<void> {
  const r = await check_update()
  if (!r.ok) {
    if (!silent) ui.toast(`检测更新失败：${r.error ?? '未知错误'}`, 'error')
    return
  }
  if (!r.update) {
    if (!silent) ui.toast('已是最新版本', 'success')
    return
  }
  const v = r.update.version
  const ok = await ui.confirm({
    title: '检测到新版本',
    message: `新版本 v${v} 可用，是否下载并安装？（安装完成后需重启应用生效）`,
    confirmText: '下载并安装',
  })
  if (!ok) return
  try {
    await install_update(r.update)
    // 已落盘安装，新版本在下次启动时生效（重启依赖 plugin-process，暂提示用户手动重启）
    ui.toast('更新安装完成，重启应用后生效', 'success')
  } catch (e) {
    ui.toast(`安装更新失败：${friendlyError(e)}`, 'error')
  }
}

// ---------------- 生命周期 / 全局快捷键 ----------------

let unlisteners: UnlistenFn[] = []
/** registerShortcut 注销函数集合（组件卸载时统一调用） */
const offs: Array<() => void> = []

/** 注册全局快捷键：新会话/关标签/切换标签按设置值（设置对话框可修改），Alt+菜单导航/Alt+数字直达不开放修改 */
function registerGlobalShortcuts(): void {
  offs.forEach((off) => {
    try {
      off()
    } catch {
      /* 重复注销忽略 */
    }
  })
  offs.length = 0
  offs.push(
    ui.registerShortcut(settings.shortcutNewSession, onCreate),
    ui.registerShortcut(settings.shortcutCloseTab, () => {
      if (activeId.value) closeTab(activeId.value)
    }),
    ui.registerShortcut(settings.shortcutNextTab, cycleTab),
  )
  // Alt+字母菜单导航（Windows 桌面惯例）：打开加速下划线对应的菜单
  const menuAccels: Array<[string, number]> = [
    ['f', 0],
    ['e', 1],
    ['v', 2],
    ['t', 3],
    ['s', 4],
    ['w', 5],
    ['h', 6],
  ]
  for (const [key, idx] of menuAccels) {
    offs.push(ui.registerShortcut(`alt+${key}`, () => menuBarRef.value?.openMenu(idx)))
  }
  for (let i = 1; i <= 9; i++) {
    const idx = i - 1
    offs.push(
      ui.registerShortcut(`alt+${i}`, () => {
        const tab = tabs.value[idx]
        if (tab) activeId.value = tab.id
      }),
    )
  }
}

onMounted(async () => {
  // 挂载时刷新树中库节点（HMR/状态重置后 connId watch 不触发，库节点需恢复挂载）
  void refreshMysqlTreeDbs()
  // 全局快捷键（Xshell 惯例）
  window.addEventListener('keydown', ui.handleKeydown)
  registerGlobalShortcuts()
  // 快捷键设置变更（设置对话框修改/首次后端加载）时重新注册
  watch(
    () => [settings.shortcutNewSession, settings.shortcutCloseTab, settings.shortcutNextTab],
    () => registerGlobalShortcuts(),
  )
  // 低频状态事件（组件卸载时 unlisten，架构红线）
  try {
    unlisteners.push(
      await listen<{ id: string; status: string }>('session-status', (e) => {
        const p = e.payload as { id: string; status: string }
        connStatus.value = new Map(connStatus.value).set(p.id, p.status)
      }),
    )
    unlisteners.push(
      await listen<{ task: { id: string; status: string } }>('transfer-status', (e) => {
        const t = (e.payload as { task: { id: string; status: string } }).task
        const map = new Map(transferStatus.value)
        map.set(t.id, t.status)
        transferStatus.value = map
      }),
    )
    // 托盘菜单「设置」：唤起主窗口后打开设置对话框（外观分区）
    unlisteners.push(
      await listen('tray-settings-requested', () => {
        settingsSection.value = 'appearance'
        showSettings.value = true
      }),
    )
    // 托盘菜单「托盘」勾选态变更（勾选=关闭窗口隐藏到托盘）：同步设置 store
    unlisteners.push(
      await listen<boolean>('tray-close-to-tray-changed', (e) => {
        settings.setTrayCloseToTray(e.payload)
      }),
    )
  } catch (e) {
    console.error('注册事件监听失败', e)
  }

  await Promise.allSettled([
    loadTree(),
    useMysqlStore().loadSavedConnections(),
    useRedisStore().loadSavedConnections(),
    loadTransferSnapshot(),
  ])

  // 启动自动检测更新（设置「启动时自动检测更新」开启时，默认关闭）：等待设置加载后按开关决定
  void settings.ensureLoaded().then(() => {
    if (settings.autoUpdateCheck) void runCheckUpdate(true)
  })

  // 标签拖出新窗口（P1）：新窗口 URL 带 ?session=<id>，启动后自动打开对应会话终端
  const urlSessionId = new URLSearchParams(window.location.search).get('session')
  if (urlSessionId) {
    const node = findNode(nodes.value, urlSessionId)
    if (node && !node.is_folder) {
      openSessionByType(node)
    } else {
      ui.toast('未找到对应会话，无法自动打开终端', 'warning')
    }
  }
  // 主窗口启动（无 ?session 参数）保持欢迎页，不自动开本地终端（用户 2026-09-24 指令）
})

onUnmounted(() => {
  window.removeEventListener('keydown', ui.handleKeydown)
  offs.forEach((off) => {
    try {
      off()
    } catch {
      /* 忽略 */
    }
  })
  unlisteners.forEach((fn) => {
    try {
      fn()
    } catch {
      /* 忽略 */
    }
  })
  unlisteners = []
})
</script>

<template>
  <div class="workspace">
    <!-- 顶部菜单栏 + 工具栏 + 快速连接地址栏（参考 Xshell） -->
    <MenuBar
      ref="menuBarRef"
      :toggles="{ nav: !ui.navCollapsed, quickbar: ui.quickBarVisible, composebar: ui.composeBarVisible }"
      :window-tabs="windowMenuTabs"
      @action="onMenuAction"
      @tab-action="onMenuTabAction"
    />
    <ToolBar
      :nav-open="!ui.navCollapsed"
      :font-size="settings.terminalFontSize"
      :font-family="settings.terminalFontFamily"
      :font-style="settings.terminalFontStyle"
      :encoding="activeSessionEncoding"
      :ssh-connected="sshQuickToggles"
      :db-tools="mysqlDbTools"
      :sftp-tools="sftpTools"
      @nav="ui.toggleNav()"
      @new-session="openSessionForm"
      @new-folder="openNewFolderDialog"
      @connect="onCreate"
      @disconnect="disconnectActive"
      @search="focusSearch"
      @transfer="openTransferTab"
      @sftp="openSftpTab"
      @sftp-new="openNewSftpSession"
      @font-size="onToolbarFont"
      @font-family="onToolbarFontFamily"
      @font-style="onToolbarFontStyle"
      @encoding="onToolbarEncoding"
      @scheme="showColorScheme = true"
      @db-transfer="showDbTransfer = true"
      @db-generate="showDataGenerate = true"
      @db-sync="showDbSync = true"
      @db-structure-sync="showStructureSync = true"
    />
    <!-- 地址栏（Xshell 惯例：工具栏下独立一整行，可输入地址回车连接，下拉切换会话） -->
    <AddressBar
      :address="activeAddress"
      :sessions="addressBarSessions"
      @connect-address="connectAddress"
      @goto-session="gotoSession"
    />

    <div class="workspace__body">
      <!-- 左缘感应区：自动隐藏模式下鼠标靠近弹出导航 -->
      <div v-if="ui.navAutoHide" class="workspace__nav-edge" @mouseenter="navHover = true" />

      <!-- 左导航树：可停靠 / 自动隐藏 -->
      <aside
        v-show="navVisible"
        class="workspace__nav"
        :class="{ 'workspace__nav--floating': ui.navAutoHide }"
        :style="{ width: `${ui.navWidth}px`, flexBasis: `${ui.navWidth}px` }"
        @mouseleave="navHover = false"
        @dblclick="onBlankDblclick"
      >
        <div class="workspace__nav-header">
          <span class="text-subtitle-2">会话</span>
          <v-spacer />
          <v-btn
            :icon="ui.navAutoHide ? 'mdi-pin-off' : 'mdi-pin'"
            size="20"
            variant="text"
            :title="ui.navAutoHide ? '切换为停靠' : '切换为自动隐藏'"
            @click="ui.navAutoHide = !ui.navAutoHide"
          />
          <v-btn
            icon="mdi-chevron-left"
            size="20"
            variant="text"
            title="收起导航"
            @click="ui.navCollapsed = true"
          />
        </div>
        <!-- 搜索框：紧凑内联输入（与工具栏快速连接同一规格，匹配整体布局） -->
        <div class="workspace__search mb-1">
          <v-icon icon="mdi-magnify" size="13" title="搜索会话" />
          <input
            ref="searchRef"
            v-model="keyword"
            class="workspace__search-input"
            type="text"
            placeholder="搜索会话…"
            aria-label="搜索会话"
            @keydown.escape="clearSearch"
          />
          <v-icon
            v-if="keyword"
            icon="mdi-close-circle"
            size="13"
            class="workspace__search-clear"
            title="清空搜索"
            @click="clearSearch"
          />
        </div>
        <div
          class="workspace__tree"
          role="tree"
          aria-label="会话列表"
          @pointermove="onTreePointerMove"
          @pointerup="endTreePointerDrag"
          @pointercancel="clearTreeDrag"
        >
          <template v-for="node in flatNodes" :key="node.id">
            <!-- 分区虚线分隔（SSH 服务 / 数据库服务） -->
            <div v-if="node.isSeparator" class="workspace__tree-sep" />
            <div
              v-else
              role="treeitem"
              tabindex="0"
              class="workspace__tree-node"
              :class="{
                'workspace__node--selected': node.id === selectedId,
                'workspace__tree-node--session': !node.isFolder,
                'workspace__tree-node--section': node.isSection,
                'workspace__tree-node--grey': !!node.grey,
                'workspace__tree-node--drop-into':
                  dropTarget?.id === node.id && dropTarget.position === 'into',
                'workspace__tree-node--drop-before':
                  dropTarget?.id === node.id && dropTarget.position === 'before',
                'workspace__tree-node--drop-after':
                  dropTarget?.id === node.id && dropTarget.position === 'after',
              }"
              :data-node-id="node.id"
              :style="{ paddingLeft: `${8 + node.depth * 20}px`, touchAction: 'none' }"
              @click="onNodeClick(node)"
              @keydown="onNodeKeydown(node, $event)"
              @contextmenu.prevent="onTreeContextmenu(node, $event)"
              @pointerdown="onTreePointerDown(node, $event)"
            >
              <!-- 可展开行收缩箭头：host 行切换库列表、文件夹行切换子级（@click.stop 不触发连接） -->
              <v-icon
                v-if="node.hasDbChildren || node.isSavedConn || node.id === 'db-standalone' || node.isFolder"
                :icon="node.isOpen ? 'mdi-chevron-down' : 'mdi-chevron-right'"
                size="14"
                class="mr-1"
                title="展开/收起"
                @click.stop="toggleTreeExpand(node)"
              />
              <v-icon
                :icon="treeIcon(node)"
                size="14"
                class="mr-1"
                :color="treeIconColor(node) ?? undefined"
              />
              <!-- 会话节点单行：以主机地址显示（Navicat 风格） -->
              <div class="workspace__node-text">
                <span class="workspace__node-name" :title="node.name">{{ node.name }}</span>
              </div>
              <!-- 在线状态点：SSH/sftp/byte-stream 会话（数据库/文件夹/分区头用图标表达，v-if="node.status" 排除） -->
              <span
                v-if="node.status"
                class="workspace__status-dot"
                :class="`workspace__status-dot--${node.status}`"
                :title="
                  node.status === 'connected'
                    ? '已连接'
                    : node.status === 'connecting'
                      ? '连接中'
                      : '未连接'
                "
              />
            </div>
          </template>
          <!-- 空态：搜索无结果/树为空（EmptyState 组件）。判定用"无任何会话行"
               （flatNodes 全为分区头/分隔线也算空；搜索无结果时分区头仍存在） -->
          <EmptyState
            v-if="!flatNodes.some((n) => !n.isSection && !n.isSeparator)"
            icon="mdi-database-search"
            :title="keyword ? '无匹配会话' : '暂无会话'"
            :desc="keyword ? '换个关键词，或清除筛选查看全部会话' : '从「会话」菜单新建连接开始'"
            :action-text="keyword ? '清除筛选' : undefined"
            class="workspace__tree-empty"
            @action="clearSearch"
          />
        </div>
      </aside>

      <!-- 导航右缘拖拽手柄：拖拽调宽 / 双击恢复默认 -->
      <div
        v-if="navVisible"
        class="workspace__nav-resize"
        :style="{ left: `${ui.navWidth - 3}px` }"
        title="拖拽调整宽度；双击恢复默认"
        @mousedown="startNavResize"
        @dblclick="ui.setNavWidth(240)"
      />

      <!-- 树右键菜单：按节点类型上下文渲染（Navicat 风格；未连接时灰化数据库操作） -->
      <v-menu
        v-model="treeMenu.visible"
        :target="[treeMenu.x, treeMenu.y]"
        location="bottom start"
        origin="auto"
        :close-on-content-click="true"
      >
        <v-list density="compact">
          <!-- 数据库叶子：Navicat 风格 12 项菜单（打开态感知：已连接绿/断开红/未连接灰，未实现项置灰） -->
          <template v-if="treeMenuCtx?.kind === 'db-leaf'">
            <!-- 打开态首项：已打开=关闭数据库；未打开=打开数据库 -->
            <v-list-item v-if="treeMenuCtx.open" @click="menuCloseDb">
              <v-list-item-title>关闭数据库</v-list-item-title>
            </v-list-item>
            <v-list-item v-else @click="menuOpenDb">
              <v-list-item-title>打开数据库</v-list-item-title>
            </v-list-item>
            <v-list-item @click="menuEditDb">
              <v-list-item-title>编辑数据库...</v-list-item-title>
            </v-list-item>
            <v-list-item :disabled="!treeMenuCtx.connId" @click="menuCreateDb">
              <v-list-item-title>新建数据库...</v-list-item-title>
            </v-list-item>
            <v-list-item :disabled="!treeMenuCtx.connId" @click="menuDropDb">
              <v-list-item-title class="text-error">删除数据库</v-list-item-title>
            </v-list-item>
            <v-divider />
            <v-list-item :disabled="!treeMenuCtx.connId" @click="menuNewQuery">
              <v-list-item-title>新建查询</v-list-item-title>
            </v-list-item>
            <v-divider />
            <v-list-item :disabled="!treeMenuCtx.connId" @click="menuNewCli">
              <v-list-item-title>命令列界面...</v-list-item-title>
            </v-list-item>
            <v-list-item :disabled="!treeMenuCtx.open" @click="menuRunSqlFile">
              <v-list-item-title>运行 SQL 文件...</v-list-item-title>
            </v-list-item>
            <v-menu location="end" :close-on-content-click="true">
              <template #activator="{ props: subProps }">
                <v-list-item v-bind="subProps" :disabled="!treeMenuCtx.connId">
                  <v-list-item-title>转储 SQL 文件</v-list-item-title>
                </v-list-item>
              </template>
              <v-list density="compact">
                <v-list-item @click="menuDumpSql(true)">
                  <v-list-item-title>转储结构和数据</v-list-item-title>
                </v-list-item>
                <v-list-item @click="menuDumpSql(false)">
                  <v-list-item-title>仅转储结构</v-list-item-title>
                </v-list-item>
              </v-list>
            </v-menu>
            <v-list-item @click="menuPrintDb">
              <v-list-item-title>打印数据库</v-list-item-title>
            </v-list-item>
            <v-list-item @click="menuErModel">
              <v-list-item-title>逆向数据库到模型...</v-list-item-title>
            </v-list-item>
            <v-list-item @click="menuFindInDb">
              <v-list-item-title>在数据库中查找</v-list-item-title>
            </v-list-item>
            <v-divider />
            <v-list-item @click="menuRefresh">
              <v-list-item-title>刷新</v-list-item-title>
            </v-list-item>
          </template>

          <!-- 数据库连接节点（MySQL/Redis 会话 + 已存连接）：状态感知；Redis 无数据库概念，仅保留连接管理项 -->
          <template v-else-if="treeMenuCtx?.kind === 'conn'">
            <v-list-item v-if="!treeMenuCtx.connected" @click="menuConnect">
              <v-list-item-title>打开连接</v-list-item-title>
            </v-list-item>
            <v-list-item v-else @click="menuCloseConnection">
              <v-list-item-title>关闭连接</v-list-item-title>
            </v-list-item>
            <v-list-item @click="menuRename">
              <v-list-item-title>编辑连接...</v-list-item-title>
            </v-list-item>
            <v-list-item v-if="treeMenuCtx.isSessionNode && treeMenuCtx.isDb" @click="menuSessionSettings">
              <v-list-item-title>会话设置...</v-list-item-title>
            </v-list-item>
            <v-list-item v-if="treeMenuCtx.isSessionNode" @click="menuCloneConnection">
              <v-list-item-title>复制连接...</v-list-item-title>
            </v-list-item>
            <v-list-item @click="menuDelete">
              <v-list-item-title class="text-error">删除连接</v-list-item-title>
            </v-list-item>
            <!-- 数据库操作项仅 MySQL：Redis 无新建库/查询/SQL 文件 -->
            <template v-if="treeMenuCtx.isDb">
              <v-divider />
              <v-list-item :disabled="!treeMenuCtx.connected" @click="menuCreateDb">
                <v-list-item-title>新建数据库...</v-list-item-title>
              </v-list-item>
              <v-list-item @click="menuNewQuery">
                <v-list-item-title>新建查询</v-list-item-title>
              </v-list-item>
              <v-list-item :disabled="!treeMenuCtx.connected" @click="menuRunSqlFile">
                <v-list-item-title>运行 SQL 文件...</v-list-item-title>
              </v-list-item>
            </template>
            <v-divider />
            <v-list-item @click="menuRefresh">
              <v-list-item-title>刷新</v-list-item-title>
            </v-list-item>
          </template>

          <!-- 通用会话节点：打开/编辑/复制/删除 + 刷新 -->
          <template v-else-if="treeMenuCtx?.kind === 'session'">
            <v-list-item @click="menuConnect">
              <v-list-item-title>打开连接</v-list-item-title>
            </v-list-item>
            <v-list-item @click="menuRename">
              <v-list-item-title>编辑连接...</v-list-item-title>
            </v-list-item>
            <v-list-item @click="menuSessionSettings">
              <v-list-item-title>会话设置...</v-list-item-title>
            </v-list-item>
            <v-list-item @click="menuCloneConnection">
              <v-list-item-title>复制连接...</v-list-item-title>
            </v-list-item>
            <v-list-item @click="menuDelete">
              <v-list-item-title class="text-error">删除连接</v-list-item-title>
            </v-list-item>
            <v-divider />
            <v-list-item @click="menuRefresh">
              <v-list-item-title>刷新</v-list-item-title>
            </v-list-item>
          </template>

          <!-- 分区头（SSH/数据库/Redis）：批量删除该分区下所有会话 -->
          <template v-else-if="treeMenuCtx?.kind === 'section'">
            <v-list-item @click="menuDeleteSection">
              <v-list-item-title class="text-error">删除</v-list-item-title>
            </v-list-item>
          </template>

          <!-- 文件夹：在此新建会话/子文件夹 + 重命名/删除（组树后归类闭环入口） -->
          <template v-else>
            <v-list-item @click="menuNewSessionInFolder">
              <v-list-item-title>在此新建会话…</v-list-item-title>
            </v-list-item>
            <v-list-item @click="menuNewSubFolder">
              <v-list-item-title>在此新建子文件夹…</v-list-item-title>
            </v-list-item>
            <v-divider />
            <v-list-item @click="menuRename">
              <v-list-item-title>重命名</v-list-item-title>
            </v-list-item>
            <v-divider />
            <v-list-item @click="menuDelete">
              <v-list-item-title class="text-error">删除</v-list-item-title>
            </v-list-item>
          </template>
        </v-list>
      </v-menu>

      <!-- 右侧多 Tab 工作区 -->
      <main class="workspace__main">
        <FlexTabs
          v-model="activeId"
          :tabs="tabItems"
          @create="onCreate"
          @close="closeTab"
          @reorder="onReorder"
          @drag-out="onDragOut"
          @rename="onRenameTab"
          @session-settings="onSessionSettings"
          @duplicate-session="onDuplicateSession"
        />
        <div class="workspace__content">
          <!-- 无 Tab 欢迎页（批3）：空窗引导，附快捷动作 → 新建会话 / 打开设置 -->
          <EmptyState
            v-if="tabs.length === 0"
            class="workspace__welcome"
            icon="mdi-sitemap"
            title="欢迎使用 FyShell"
            desc="连接会话开始工作：SSH 终端、SFTP 传输、MySQL / Redis 数据库管理"
            @dblclick="onBlankDblclick"
          >
            <div class="d-flex ga-2 mt-2 justify-center">
              <v-btn
                size="small"
                variant="tonal"
                density="compact"
                color="primary"
                prepend-icon="mdi-plus"
                @click="openSessionForm"
              >
                新建会话
              </v-btn>
              <v-btn size="small" variant="text" density="compact" prepend-icon="mdi-cog-outline" @click="showSettings = true">
                打开设置
              </v-btn>
            </div>
          </EmptyState>
          <!-- v-show 保持终端 Tab 存活，切换不销毁会话状态 -->
          <div
            v-for="tab in tabs"
            :key="tab.id"
            v-show="tab.id === activeId"
            class="workspace__pane"
          >
            <!-- 终端 Tab：终端 + 命令栏 + 撰写栏（命令栏/撰写栏随 Tab 内嵌，不再全局） -->
            <template v-if="tab.type === 'terminal' && tab.connId">
              <!-- 连接失败（Xshell 惯例：Tab 保留可改参数重连）：就地展示错误与重试入口 -->
              <div v-if="failedConns.has(tab.id)" class="workspace__conn-error">
                <v-icon icon="mdi-alert-circle-outline" size="40" class="workspace__conn-error-icon" />
                <div class="workspace__conn-error-title">连接「{{ tab.title }}」失败</div>
                <div class="workspace__conn-error-msg">{{ failedConns.get(tab.id) }}</div>
                <div class="workspace__conn-error-actions">
                  <v-btn
                    size="small"
                    color="primary"
                    prepend-icon="mdi-refresh"
                    :loading="retryingConns.has(tab.id)"
                    @click="retryConnection(tab)"
                  >重试</v-btn>
                  <v-btn size="small" variant="tonal" @click="closeTab(tab.id)">关闭标签</v-btn>
                </div>
              </div>
              <template v-else>
                <TerminalPane
                  :ref="(el) => setPaneRef(tab.id, el)"
                  :session-id="tab.connId"
                  :session-node-id="tab.sessionId"
                />
                <QuickCommandBar :session-id="tab.connId" />
                <ComposeBar :session-id="tab.connId" />
              </template>
            </template>
            <TransferQueueView v-else-if="tab.type === 'transfer'" />
            <DualPane
              v-else-if="tab.type === 'sftp'"
              :session-id="sftpPaneSessionId(tab)"
              :session-name="tab.sessionId ? tab.title : lastTerminalSessionName"
              :remote-path="sftpRemotePaths[tab.id] ?? ''"
              @update:remotePath="(p: string) => (sftpRemotePaths[tab.id] = p)"
              @remote-path-change="(p: string) => (currentPath = p)"
              @transfer-started="openTransferTab"
            />
            <TunnelView v-else-if="tab.type === 'tunnel'" />
            <ComposePane v-else-if="tab.type === 'compose'" @sent="onComposeSent" />
            <MysqlDbWorkspace v-else-if="tab.type === 'mysql'" />
            <MysqlCliConsole v-else-if="tab.type === 'mysqlcli'" @exit="closeTab(tab.id)" />
            <MysqlQueryTab v-else-if="tab.type === 'query'" />
            <RedisDbWorkspace v-else-if="tab.type === 'redis'" />
          </div>
        </div>
      </main>
    </div>

    <!-- 底部状态栏 -->
    <StatusBar
      :connection-status="activeSessionStatus as 'connecting' | 'connected' | 'disconnected' | 'hostkey-verify' | null"
      :connection-name="activeSessionName"
      :current-path="currentPath"
      :transfer-count="transferCount"
      :host-ip="activeSessionHost"
      :encoding="activeSessionEncoding"
      :session-count="tabs.length"
    />

    <!-- 会话日志查看器（P1：活动会话日志落盘开关与查看） -->
    <v-dialog v-model="showLogViewer" width="640">
      <v-card class="log-dialog">
        <v-card-title class="d-flex align-center">
          <v-icon icon="mdi-text-box-outline" size="small" class="mr-2" />
          会话日志
          <v-spacer />
          <v-btn
            icon="mdi-close"
            size="x-small"
            variant="text"
            title="关闭"
            @click="showLogViewer = false"
          />
        </v-card-title>
        <v-divider />
        <LogViewer v-if="activeTerminalId" :session-id="activeTerminalId" />
      </v-card>
    </v-dialog>

    <!-- 会话表单（新建 / 编辑 / 快速连接） -->
    <SessionForm
      v-model="showSessionForm"
      :session="editingSession"
      :folder-id="newFolderPreset"
      :folder-options="folderOptions"
      :preset-host="presetHost"
      :default-type="presetSftp ? 'sftp' : undefined"
      :save-handler="editingSavedConn ? saveSavedConn : undefined"
      @saved="onSessionSaved"
    />

    <!-- 新建文件夹对话框 -->
    <v-dialog v-model="showFolderDialog" width="360">
      <v-card>
        <v-card-title class="d-flex align-center text-subtitle-1">{{ folderDialogTitle }}
          <v-spacer />
          <v-btn
          icon="mdi-close"
          size="x-small"
          variant="text"
          title="关闭"
          @click="showFolderDialog = false"
          />
        </v-card-title>
        <v-divider />
        <v-card-text>
          <div class="fy-field-row">
            <span class="fy-field-row__label">文件夹名称</span>
            <v-text-field
              v-model="folderName"
              density="compact"
              autofocus
              @keydown.enter="createFolder"
            />
          </div>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="showFolderDialog = false">取消</v-btn>
          <v-btn color="primary" @click="createFolder">确定</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- 新建数据库对话框（树右键"新建数据库..."入口；connId 挂当前活动 MySQL 连接） -->
    <NewDatabaseDialog
      v-model="showNewDbDialog"
      :conn-id="mysqlConnId"
      @saved="refreshMysqlTreeDbs"
    />

    <!-- 导入导出对话框（树右键"运行 SQL 文件..."入口；connId 挂当前活动 MySQL 连接） -->
    <ImportExportDialog
      v-model="ioDialogVisible"
      :conn-id="ioDialogConnId"
      :initial-include-create-table="ioIncludeCreateTable"
    />

    <!-- 编辑数据库/逆向到模型/在库中查找对话框（树右键入口；connId 挂当前活动 MySQL 连接） -->
    <EditDatabaseDialog
      v-model="showEditDbDialog"
      :conn-id="mysqlConnId"
      :db-name="editDbName"
    />
    <ErModelDialog
      v-model="showErModelDialog"
      :conn-id="mysqlConnId"
      :db-name="erDbName"
    />
    <FindInDbDialog
      v-model="showFindDialog"
      :conn-id="mysqlConnId"
      :db-name="findDbName"
      @open-table="openFoundTable"
    />

    <!-- 工具栏数据库工具 4 项对话框（connId 挂当前活动 MySQL 连接） -->
    <DataTransferDialog
      v-model="showDbTransfer"
      :conn-id="mysqlConnId"
      @completed="refreshMysqlTreeDbs"
    />
    <DataGenerateDialog
      v-model="showDataGenerate"
      :conn-id="mysqlConnId"
      @completed="refreshMysqlTreeDbs"
    />
    <DbSyncDialog v-model="showDbSync" :conn-id="mysqlConnId" @completed="refreshMysqlTreeDbs" />
    <StructureSyncDialog
      v-model="showStructureSync"
      :conn-id="mysqlConnId"
      @completed="refreshMysqlTreeDbs"
    />

    <!-- 文件菜单"打开"：会话列表对话框（Xshell 会话管理器风格，选择/双击/右键即打开） -->
    <SessionListDialog
      v-model="showSessionListDialog"
      :sessions="sessionListFlat"
      @open-session="openSessionFromList"
      @properties="openListProperties"
      @delete="deleteFromList"
      @new-session="openSessionForm"
      @new-folder="openNewFolderDialog"
      @refresh="loadTree"
    />

    <!-- 主密码设置对话框（首次设置 + 修改/校验） -->
    <MasterPasswordDialog v-model="showMasterPassword" />
    <ShortcutListDialog v-model="showShortcutList" />
    <!-- 配色方案对话框（工具栏"配色"入口，Xshell 风格方案管理） -->
    <ColorSchemeDialog v-model="showColorScheme" />

    <!-- 高功能设置对话框（外观/终端/SFTP/数据/安全/关于），主密码为快捷入口 -->
    <SettingsDialog
      v-model="showSettings"
      :initial-section="settingsSection"
      @open-master-password="showMasterPassword = true"
    />

    <!-- SSH 选项对话框（标签右键/菜单栏"会话设置"入口；会话模式编辑写会话级键；
         byte-stream 会话编辑会话字段，保存后刷新会话树） -->
    <!-- SSH 选项对话框（标签右键/菜单栏"会话设置"入口；会话模式编辑写会话级键） -->
    <SshOptionsDialog
      v-model="showSshOptionsDialog"
      :session-id="sshOptionsSessionId ?? undefined"
      :session-name="sshOptionsSessionName"
      :terminal-only="sshOptionsLocalOnly"
    />

    <!-- 全局弹层（确认 / toast / 主题同步） -->
    <GlobalDialog />

    <!-- HostKey 首次确认弹层（连接新主机时 Rust 侧挂起等待确认，必须挂载） -->
    <HostkeyDialog />

    <!-- 终端 rz/sz 传输弹层（rz/sz 触发，必须挂载） -->
    <RzszDialog />
  </div>
</template>

<style scoped>
.workspace {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
  background: rgb(var(--v-theme-surface));
  color: rgb(var(--v-theme-on-surface));
}

/* 会话日志对话框：标题行+divider 之外给 LogViewer 固定内容高度 */
.log-dialog :deep(.log-viewer) {
  height: 420px;
}

.workspace__body {
  position: relative;
  display: flex;
  flex: 1 1 auto;
  min-height: 0;
  overflow: hidden;
}

/* 左缘感应区：自动隐藏模式下鼠标靠近即弹出导航 */
.workspace__nav-edge {
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  width: 4px;
  z-index: 10;
  cursor: col-resize;
}

/* 左导航树：宽度由 ui.navWidth 动态绑定（可拖拽调整），CSS 仅作兜底。
   二期：底色提一档用 --fy-surface-raised，与右侧内容白色面形成两级层次 */
.workspace__nav {
  display: flex;
  flex-direction: column;
  flex: 0 0 240px;
  width: 240px;
  min-width: 0;
  padding: 6px 6px 0;
  border-right: 1px solid var(--fy-chrome-border);
  background: var(--fy-surface-raised);
  overflow: hidden;
  /* 双击空白打开终端：禁止文本选中，双击不会拖出词选中高亮 */
  user-select: none;
}

/* 导航右缘拖拽手柄：6px 命中区，悬停高亮主题蓝 */
.workspace__nav-resize {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 6px;
  z-index: 11;
  cursor: col-resize;
  user-select: none;
}

.workspace__nav-resize:hover {
  background: rgba(var(--v-theme-primary), 0.25);
}

/* 自动隐藏模式：导航悬浮于内容之上（参考 Xshell） */
.workspace__nav--floating {
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  z-index: 10;
  box-shadow: var(--fy-shadow-pop), 2px 0 6px rgb(0 0 0 / 0.18);
}

.workspace__nav-header {
  display: flex;
  align-items: center;
  min-height: 26px;
  user-select: none;
}

/* 搜索框：紧凑内联输入（与工具栏快速连接同一规格） */
.workspace__search {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 26px; /* 与全局单行控件基线一致（字段/快速连接 26px，原 24px） */
  flex: none;
  padding: 0 6px;
  border: 1px solid var(--fy-chrome-border);
  border-radius: 4px;
  background: rgb(var(--v-theme-surface));
  color: rgba(var(--v-theme-on-surface), 0.6);
}

.workspace__search-input {
  flex: 1 1 auto;
  min-width: 0;
  height: 100%;
  border: none;
  outline: none;
  background: transparent;
  color: rgb(var(--v-theme-on-surface));
  font-size: 12px;
}

.workspace__search-input:focus-visible {
  outline: none;
}

.workspace__search:focus-within {
  border-color: var(--fy-focus-color);
}

.workspace__search-input::placeholder {
  color: rgba(var(--v-theme-on-surface), 0.4);
}

.workspace__search-clear {
  cursor: pointer;
  flex: none;
}

.workspace__tree {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
}

.workspace__tree-node {
  display: flex;
  align-items: center;
  min-height: var(--fy-row-height-tree, 24px); /* 紧凑行高保证信息密度 */
  padding-right: 6px;
  cursor: pointer;
  border-radius: 4px;
  font-size: 12px;
  white-space: nowrap;
  user-select: none;
  /* 二期：克制动效（hover 底色/选中底色渐变，transform/opacity 线不受影响） */
  transition: background var(--fy-dur-fast) var(--fy-ease);
}

/* 历史库（断开连接后保留展示）：整行弱化 */
.workspace__tree-node--grey {
  opacity: 0.55;
}

/* 会话节点双行（名称 + user@host） */
.workspace__tree-node--session {
  padding-top: 2px;
  padding-bottom: 2px;
}

.workspace__node-text {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto; /* 撑满剩余宽度，让右侧状态点贴右缘 */
  min-width: 0;
  line-height: 1.25;
}

/* hover 反馈统一走 --fy-hover-bg 变量（二期收敛组件内散落灰度） */
.workspace__tree-node:hover {
  background: var(--fy-hover-bg);
}

/* 键盘导航可见焦点环（treeitem 可聚焦后必须保留；fallback 清理到反馈变量） */
.workspace__tree-node:focus-visible {
  outline: 1px solid var(--fy-focus-color);
  outline-offset: -1px;
}

/* 选中态：--fy-select-bg 底色 + 左侧主题色重点条（现代工具 DataGrip 惯例）。
   box-shadow inset 实现左条，不改变流式布局；border-radius 与底色圆角一致 */
.workspace__node--selected {
  background: var(--fy-select-bg);
  box-shadow: inset 2px 0 0 rgb(var(--v-theme-primary));
}

/* 拖拽落点高亮（会话树拖拽改序）：移入文件夹 = 整行描边底色；before/after = 上下 2px 指示线 */
.workspace__tree-node--drop-into {
  background: rgba(var(--v-theme-primary), 0.12);
  box-shadow: inset 0 0 0 1px rgb(var(--v-theme-primary));
}

.workspace__tree-node--drop-before {
  box-shadow: inset 0 2px 0 0 rgb(var(--v-theme-primary));
}

.workspace__tree-node--drop-after {
  box-shadow: inset 0 -2px 0 0 rgb(var(--v-theme-primary));
}

.workspace__node-name {
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 会话在线状态点（批2）：6px 圆点，语义色表达连接状态，transition 淡入淡出随状态切换平滑变化。
   颜色用主题语义色变量（success/warning/灰），深浅主题自动；连接中黄 / 已连接绿 / 未连接灰 */
.workspace__status-dot {
  flex: 0 0 auto;
  width: 6px;
  height: 6px;
  margin-left: 6px;
  border-radius: 50%;
  background: rgba(var(--v-theme-on-surface), 0.25);
  transition: background var(--fy-dur-fast) var(--fy-ease);
}

.workspace__status-dot--connected {
  background: rgb(var(--v-theme-success));
}

.workspace__status-dot--connecting {
  background: rgb(var(--v-theme-warning));
}

/* 未连接：default 灰（由基类定义），无额外覆盖 */


/* 分区虚线分隔（SSH 服务 / 数据库服务） */
.workspace__tree-sep {
  height: 0;
  border-top: 1px dashed rgba(var(--v-theme-on-surface), 0.12);
  margin: 4px 10px;
  user-select: none;
}

/* 分区头：浅色文字 + 紧凑行高，单击可收缩 */
.workspace__tree-node--section {
  color: rgba(var(--v-theme-on-surface), 0.75);
}

/* 工作台无 Tab 欢迎页：整区居中于空内容区（EmptyState 垂直居中，动作按钮在其下方） */
.workspace__welcome {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  /* 双击空白打开终端：禁止文本选中，双击不会拖出词选中高亮 */
  user-select: none;
}

/* 树空态容器：在窄树内压缩 EmptyState 内边距（16px 上下的紧凑留白即可） */
.workspace__tree-empty :deep(.fy-empty) {
  padding: 20px 8px;
}

/* 右侧多 Tab 工作区 */
.workspace__main {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}

.workspace__content {
  position: relative;
  flex: 1 1 auto;
  min-height: 0;
  overflow: hidden;
}

.workspace__pane {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

/* 连接失败就地重试态（Xshell 惯例：失败后 Tab 保留）：主区中央图标 + 会话名 + 原因 + 操作 */
.workspace__conn-error {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 4px;
  background-color: var(--fy-terminal-bg);
  color: rgba(var(--v-theme-on-surface), 0.45);
}

.workspace__conn-error-icon {
  margin-bottom: 4px;
  color: rgb(var(--v-theme-error));
}

.workspace__conn-error-title {
  font-size: 12px;
  color: rgb(var(--v-theme-on-surface));
}

.workspace__conn-error-msg {
  max-width: 60ch;
  text-align: center;
  font-size: 12px;
  line-height: 1.5;
}

.workspace__conn-error-actions {
  display: flex;
  gap: 8px;
  margin-top: 8px;
}

</style>
