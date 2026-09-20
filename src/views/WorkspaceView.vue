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
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import FlexTabs, { type FlexTabItem } from '@/components/common/FlexTabs.vue'
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
import ZmodemDialog from '@/components/ssh/terminal/ZmodemDialog.vue'
import TerminalPane from '@/components/ssh/terminal/TerminalPane.vue'
import ComposeBar from '@/components/ssh/terminal/ComposeBar.vue'
import DualPane from '@/components/sftp/DualPane.vue'
import TransferQueueView from '@/views/transfer/TransferQueueView.vue'
import TunnelView from '@/views/tunnel/TunnelView.vue'
import ComposePane from '@/components/common/quickcommand/ComposePane.vue'
import LogViewer from '@/components/ssh/log/LogViewer.vue'
import MysqlDbWorkspace from '@/components/mysql/MysqlDbWorkspace.vue'
import ImportExportDialog from '@/components/mysql/ImportExportDialog.vue'
import EditDatabaseDialog from '@/components/mysql/EditDatabaseDialog.vue'
import NewDatabaseDialog from '@/components/mysql/NewDatabaseDialog.vue'
import ErModelDialog from '@/components/mysql/ErModelDialog.vue'
import FindInDbDialog from '@/components/mysql/FindInDbDialog.vue'
import MysqlCliConsole from '@/components/mysql/MysqlCliConsole.vue'
import MysqlQueryTab from '@/components/mysql/MysqlQueryTab.vue'
import RedisDbWorkspace from '@/components/redis/RedisDbWorkspace.vue'
import SessionListDialog from '@/components/ssh/session/SessionListDialog.vue'
import { useMysqlStore } from '@/stores/mysql'
import { useRedisStore } from '@/stores/redis'
import { mysqlDbList, mysqlDbSwitch, mysqlDbDrop } from '@/api/mysqlDb'
import { mysqlListTables, mysqlQuery } from '@/api/mysql'
import { sessionList, sessionClone } from '@/api/session'
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
    ui.toast(`加载会话列表失败：${String(e)}`, 'error')
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

/** 会话树归一化：兼容「config 挂载」与「节点本身即 SessionConfig（Rust enum 平铺序列化）」
    两种 wire 形态——不归一化时 config 恒为 undefined，host 匹配/连接路由/图标全部失效 */
function normalizeTreeNodes(raw: unknown): SessionNode[] {
  if (!Array.isArray(raw)) return []
  const out: SessionNode[] = []
  for (const item of raw) {
    if (!item || typeof item !== 'object') continue
    const r = item as Record<string, unknown>
    const id = String(r.id ?? '')
    if (!id) continue
    const isFolder =
      r.kind === 'folder' || r.kind === 'Folder' || r.is_folder === true || Array.isArray(r.children)
    if (isFolder) {
      out.push({
        id,
        name: String(r.name ?? ''),
        is_folder: true,
        children: normalizeTreeNodes(r.children),
      })
      continue
    }
    // 会话节点：config 挂载优先，缺失时节点本身即配置（平铺）
    const config = (r.config ?? r) as SessionNode['config']
    out.push({
      id,
      name: String(r.name ?? ''),
      is_folder: false,
      config,
    })
  }
  return out
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
    if (dbs && dbs.savedId === null && dbs.sessionId === null && !standaloneTaken) {
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
  const onUp = (): void => {
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
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
  const stype = target.config?.session_type
  if (stype === 'mysql') {
    void connectMysqlSession(target)
    return
  }
  if (stype === 'redis') {
    void connectRedisSession(target)
    return
  }
  // Telnet / RLOGIN / 串口：byte-stream 会话打开终端（持久会话路径）
  if (stype === 'telnet' || stype === 'rlogin' || stype === 'serial') {
    openByteStreamSession(target)
    return
  }
  openTerminal(target)
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
    const stype = target.config?.session_type
    // 数据库会话：切到 MySQL 工作台并按会话配置建连（不开终端）
    if (stype === 'mysql') {
      void connectMysqlSession(target)
      return
    }
    // Redis 数据库会话：切到 Redis 工作台并按会话配置建连（不开终端）
    if (stype === 'redis') {
      void connectRedisSession(target)
      return
    }
    // Telnet / RLOGIN / 串口：byte-stream 会话打开终端（持久会话路径）
    if (stype === 'telnet' || stype === 'rlogin' || stype === 'serial') {
      openByteStreamSession(target)
      return
    }
    openTerminal(target)
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

/** 树中双击数据库子节点：切换 MySQL 当前库并刷新表清单 */
async function switchMysqlDbFromTree(node: FlatNode): Promise<void> {
  const mysqlStore = useMysqlStore()
  const connId = mysqlStore.connId
  const dbName = node.dbName
  if (!connId || !dbName) return
  if (dbName === lastDbSwitchId && Date.now() - lastDbSwitchAt < 800) return
  lastDbSwitchId = dbName
  lastDbSwitchAt = Date.now()
  try {
    // 打开 MySQL 工作台展示切库结果（树中切库的可见反馈）
    openMysqlTab()
    // 后端重建连接池并返回新 conn_id，须替换 store.connId
    //（MysqlDbWorkspace 的 connId watch 随之清空会话级状态并刷新库下拉）
    const newId = await mysqlDbSwitch(connId, dbName)
    mysqlStore.connId = newId
    mysqlStore.tables = []
    await mysqlStore.loadTables()
    mysqlStore.queryError = ''
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

/** 右键菜单"重命名"：打开会话表单编辑态（文件夹打开名称对话框） */
function menuRename(): void {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node) return
  if (node.isFolder) {
    showFolderDialog.value = true
    return
  }
  const target = findNode(nodes.value, node.id)
  if (!target) return
  editingSession.value = (target.config ?? {}) as unknown as SessionConfig
  presetHost.value = ''
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
      ui.toast(`删除失败：${String(e)}`, 'error')
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
    ui.toast(`删除失败：${String(e)}`, 'error')
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
      failed.push(`${t.name}（${String(e)}）`)
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
    showSessionForm.value = true
  } catch (e) {
    ui.toast(`复制连接失败：${String(e)}`, 'error')
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

/** 切换 MySQL 连接默认库（复用 mysql_db_switch 重建连接池；与工作台 switchDb 同型） */
async function switchMysqlDb(name: string): Promise<void> {
  const st = useMysqlStore()
  if (!st.connId || !name) return
  try {
    const newId = await mysqlDbSwitch(st.connId, name)
    st.connId = newId
    st.tables = []
    await st.loadTables()
    st.queryError = ''
    ui.toast(`已切换到数据库「${name}」`, 'success')
  } catch (e) {
    ui.toast(String(e), 'error')
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
      ui.toast(`打印数据库失败：${String(e)}`, 'error')
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
    ui.toast(`删除数据库失败：${String(e)}`, 'error')
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
  h1 { font-size: 18px; margin: 0 0 4px; }
  .gen { color: #666; margin: 0 0 16px; }
  section.tbl { margin-bottom: 18px; page-break-inside: avoid; }
  h2 { font-size: 14px; margin: 0 0 4px; }
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

/** 隐藏 iframe 写入 HTML 并调起打印（WebView2 内打印，不弹新窗口） */
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
  iframe.contentWindow?.print()
  window.setTimeout(() => document.body.removeChild(iframe), 60_000)
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

/** 连接失败处理（Xshell 惯例：失败后终端 Tab 保留可改参数重连）：
 * 失败原因记入 failedConns（终端区就地展示「重试」入口），toast 提示 */
function runConnect(tab: WorkTab, action: string): void {
  const connect = retryHandlers.get(tab.id)
  if (!connect) return
  void connect().catch((e) => {
    const reason = e instanceof Error ? e.message : String(e)
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
      failedConns.value.set(tab.id, e instanceof Error ? e.message : String(e))
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

/** 打开 SFTP 双栏 Tab（单例，P0）：本地/远程双栏文件传输 */
function openSftpTab(): void {
  const existing = tabs.value.find((t) => t.type === 'sftp')
  if (existing) {
    activeId.value = existing.id
    return
  }
  const tab: WorkTab = { id: genTabId(), type: 'sftp', title: 'SFTP 文件传输' }
  tabs.value.push(tab)
  activeId.value = tab.id
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
      openTerminal(node)
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
const tabItems = computed<FlexTabItem[]>(() =>
  tabs.value.map((t) => ({
    id: t.id,
    title: t.title,
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

function openSessionForm(): void {
  editingSession.value = null
  presetHost.value = ''
  showSessionForm.value = true
}

/** 快速连接：打开会话表单并预填主机（Xshell 快速连接流程） */
function quickConnect(host: string): void {
  editingSession.value = null
  presetHost.value = host
  showSessionForm.value = true
}

/** 会话保存成功（新建/编辑/快速连接）：刷新树并打开对应终端 */
async function onSessionSaved(config: SessionConfig): Promise<void> {
  await loadTree()
  openTerminal({ id: config.id, name: config.name, color: config.color })
}

/** 新建文件夹（P0：新建在根级） */
const showFolderDialog = ref(false)
const folderName = ref('')

// ---------------- 文件菜单"打开"：会话列表对话框（SessionListDialog） ----------------

const showSessionListDialog = ref(false)

/** 扁平化的已添加会话列表（非文件夹节点） */
const sessionListFlat = computed(() => {
  const out: SessionNode[] = []
  const walkList = (list: SessionNode[]) => {
    for (const n of list) {
      if (n.is_folder) walkList(n.children ?? [])
      else out.push(n)
    }
  }
  walkList(nodes.value)
  return out
})

/** 打开会话列表中选中的会话终端 */
function openSessionFromList(target: { id: string; name: string }): void {
  showSessionListDialog.value = false
  selectedId.value = target.id
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
    ui.toast(`删除失败：${String(e)}`, 'error')
  }
}

async function createFolder(): Promise<void> {
  const name = folderName.value.trim()
  if (!name) return
  try {
    await sessionStore.saveFolder({ id: crypto.randomUUID(), name, parent_id: null })
    folderName.value = ''
    showFolderDialog.value = false
    await loadTree()
  } catch (e) {
    ui.toast(`新建文件夹失败：${String(e)}`, 'error')
  }
}

/** 断开当前活动 Tab 的会话（closeTab 内含 Rust 侧清理逻辑） */
function disconnectActive(): void {
  if (activeId.value) closeTab(activeId.value)
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
  if (tab.type === 'terminal' && tab.sessionId) {
    const node = findNode(nodes.value, tab.sessionId)
    if (node) {
      openTerminal(node)
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

/** 最近打开的终端会话（SFTP 双栏远程栏默认浏览对象；无终端 Tab 时为空） */
const lastTerminalSessionId = computed<string>(() => {
  const t = terminalStore.tabs.at(-1)
  return t?.panes[0]?.sessionId ?? ''
})

const lastTerminalSessionName = computed(() => terminalStore.tabs.at(-1)?.title ?? '')

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
      showFolderDialog.value = true
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

/** 检测更新：无更新提示最新版本；有更新确认后下载安装并重启；网络错误 toast 提示 */
async function runCheckUpdate(): Promise<void> {
  const r = await check_update()
  if (!r.ok) {
    ui.toast(`检测更新失败：${r.error ?? '未知错误'}`, 'error')
    return
  }
  if (!r.update) {
    ui.toast('已是最新版本', 'success')
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
    ui.toast(`安装更新失败：${e instanceof Error ? e.message : String(e)}`, 'error')
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

  // 标签拖出新窗口（P1）：新窗口 URL 带 ?session=<id>，启动后自动打开对应会话终端
  const urlSessionId = new URLSearchParams(window.location.search).get('session')
  if (urlSessionId) {
    const node = findNode(nodes.value, urlSessionId)
    if (node && !node.is_folder) {
      openTerminal(node)
    } else {
      ui.toast('未找到对应会话，无法自动打开终端', 'warning')
    }
  } else if (tabs.value.length === 0) {
    // 默认打开本地终端（主窗口启动且无任何标签时）
    openLocalTerminal()
  }
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
      @nav="ui.toggleNav()"
      @new-session="openSessionForm"
      @new-folder="showFolderDialog = true"
      @connect="onCreate"
      @disconnect="disconnectActive"
      @search="focusSearch"
      @transfer="openTransferTab"
      @sftp="openSftpTab"
      @font-size="onToolbarFont"
      @font-family="onToolbarFontFamily"
      @font-style="onToolbarFontStyle"
      @encoding="onToolbarEncoding"
      @scheme="showColorScheme = true"
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
        <div class="workspace__tree" role="tree" aria-label="会话列表">
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
              }"
              :style="{ paddingLeft: `${8 + node.depth * 20}px` }"
              @click="onNodeClick(node)"
              @keydown="onNodeKeydown(node, $event)"
              @contextmenu.prevent="onTreeContextmenu(node, $event)"
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
            </div>
          </template>
          <div v-if="flatNodes.length === 0" class="workspace__tree-empty">无匹配会话</div>
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

          <!-- 文件夹：重命名/删除 -->
          <template v-else>
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
              :session-id="lastTerminalSessionId"
              :session-name="lastTerminalSessionName"
              @remote-path-change="(p: string) => (currentPath = p)"
            />
            <TunnelView v-else-if="tab.type === 'tunnel'" />
            <ComposePane v-else-if="tab.type === 'compose'" @sent="onComposeSent" />
            <MysqlDbWorkspace v-else-if="tab.type === 'mysql'" />
            <MysqlCliConsole v-else-if="tab.type === 'mysqlcli'" @exit="closeTab(tab.id)" />
            <MysqlQueryTab v-else-if="tab.type === 'query'" />
            <RedisDbWorkspace v-else-if="tab.type === 'redis'" />
          </div>
          <div v-if="tabs.length === 0" class="workspace__empty">
            <v-icon icon="mdi-console" size="48" class="mb-2" />
            <div class="workspace__empty-hint">双击左侧会话打开终端，Ctrl+T 新建标签</div>
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
      :preset-host="presetHost"
      @saved="onSessionSaved"
    />

    <!-- 新建文件夹对话框 -->
    <v-dialog v-model="showFolderDialog" width="360">
      <v-card>
        <v-card-title class="d-flex align-center text-subtitle-1">新建文件夹
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
    />

    <!-- 文件菜单"打开"：会话列表对话框（Xshell 会话管理器风格，选择/双击/右键即打开） -->
    <SessionListDialog
      v-model="showSessionListDialog"
      :sessions="sessionListFlat"
      @open-session="openSessionFromList"
      @properties="openListProperties"
      @delete="deleteFromList"
      @new-session="openSessionForm"
      @new-folder="showFolderDialog = true"
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

    <!-- 终端 ZMODEM 传输弹层（rz/sz 触发，必须挂载） -->
    <ZmodemDialog />
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

/* 左导航树：宽度由 ui.navWidth 动态绑定（可拖拽调整），CSS 仅作兜底 */
.workspace__nav {
  display: flex;
  flex-direction: column;
  flex: 0 0 240px;
  width: 240px;
  min-width: 0;
  padding: 6px 6px 0;
  border-right: 1px solid var(--fy-chrome-border);
  background: var(--fy-chrome-bg);
  overflow: hidden;
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
  background: rgb(var(--v-theme-primary) / 0.25);
}

/* 自动隐藏模式：导航悬浮于内容之上（参考 Xshell） */
.workspace__nav--floating {
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  z-index: 10;
  box-shadow: 2px 0 12px rgb(0 0 0 / 0.4);
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
  color: rgb(var(--v-theme-on-surface) / 0.6);
}

.workspace__search-input {
  flex: 1 1 auto;
  min-width: 0;
  height: 100%;
  border: none;
  outline: none;
  background: transparent;
  color: rgb(var(--v-theme-on-surface));
  font-size: 14px;
}

.workspace__search-input:focus-visible {
  outline: none;
}

.workspace__search:focus-within {
  border-color: var(--fy-focus-color);
}

.workspace__search-input::placeholder {
  color: rgb(var(--v-theme-on-surface) / 0.4);
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
  font-size: 14px;
  white-space: nowrap;
  user-select: none;
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
  min-width: 0;
  line-height: 1.25;
}

.workspace__node-host {
  font-family: var(--fy-font);
  font-size: 14px;
  color: rgb(var(--v-theme-on-surface) / 0.5);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.workspace__tree-node:hover {
  background: rgb(var(--v-theme-on-surface) / 0.08);
}

/* 键盘导航可见焦点环（treeitem 可聚焦后必须保留） */
.workspace__tree-node:focus-visible {
  outline: 1px solid rgb(var(--v-theme-primary, 82 132 255));
  outline-offset: -1px;
}

.workspace__node--selected {
  background: rgb(var(--v-theme-primary) / 0.15);
}

.workspace__node-name {
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 分区虚线分隔（SSH 服务 / 数据库服务） */
.workspace__tree-sep {
  height: 0;
  border-top: 1px dashed rgba(var(--v-theme-on-surface), 0.12);
  margin: 4px 10px;
  user-select: none;
}

/* 分区头：浅色文字 + 紧凑行高，单击可收缩 */
.workspace__tree-node--section {
  color: rgb(var(--v-theme-on-surface) / 0.75);
}

.workspace__tree-empty {
  padding: 12px 8px;
  font-size: 14px;
  color: rgb(var(--v-theme-on-surface) / 0.5);
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

/* 空态 */
.workspace__empty {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  color: rgb(var(--v-theme-on-surface) / 0.45);
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
  color: rgb(var(--v-theme-on-surface) / 0.45);
}

.workspace__conn-error-icon {
  margin-bottom: 4px;
  color: rgb(var(--v-theme-error));
}

.workspace__conn-error-title {
  font-size: 14px;
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

/* 空态提示文字：主区中央大字提示，14px 标题档 */
.workspace__empty-hint {
  font-size: 14px;
  color: rgb(var(--v-theme-on-surface) / 0.55);
}
</style>
