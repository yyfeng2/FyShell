<script setup lang="ts">
/**
 * WorkspaceView —— 应用主工作区
 *
 * 整体布局（参考 Xshell，架构 §3.1）：
 * - 左导航树（可停靠 / 自动隐藏）+ 右多 Tab 工作区 + 底部状态栏
 * - Tab 按连接着色（会话 color 字段）
 * - 工作区按 Tab 类型渲染：终端 Tab 渲染终端区、传输 Tab 渲染传输队列视图
 *
 * 全局快捷键（技术红线）：Ctrl+T 新标签、Ctrl+W 关闭、Ctrl+Tab 切换、Alt+1~9 直达
 */
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import FlexTabs, { type FlexTabItem } from '@/components/common/FlexTabs.vue'
import StatusBar from '@/components/common/StatusBar.vue'
import GlobalDialog from '@/components/common/GlobalDialog.vue'
import MenuBar from '@/components/common/MenuBar.vue'
import MasterPasswordDialog from '@/components/common/MasterPasswordDialog.vue'
import SettingsDialog from '@/components/common/SettingsDialog.vue'
import SshOptionsDialog from '@/components/ssh/options/SshOptionsDialog.vue'
import ToolBar from '@/components/common/ToolBar.vue'
import QuickCommandBar from '@/components/common/QuickCommandBar.vue'
import SessionForm from '@/components/ssh/session/SessionForm.vue'
import ByteStreamForm from '@/components/ssh/session/ByteStreamForm.vue'
import HostkeyDialog from '@/components/ssh/terminal/HostkeyDialog.vue'
import TerminalPane from '@/components/ssh/terminal/TerminalPane.vue'
import DualPane from '@/components/sftp/DualPane.vue'
import TransferQueueView from '@/views/transfer/TransferQueueView.vue'
import TunnelView from '@/views/tunnel/TunnelView.vue'
import MonitorDrawer from '@/components/ssh/monitor/MonitorDrawer.vue'
import MonitorMiniBar from '@/components/ssh/monitor/MonitorMiniBar.vue'
import ComposePane from '@/components/common/quickcommand/ComposePane.vue'
import LogViewer from '@/components/ssh/log/LogViewer.vue'
import MysqlDbWorkspace from '@/components/mysql/MysqlDbWorkspace.vue'
import { useMysqlStore } from '@/stores/mysql'
import { mysqlDbList, mysqlDbSwitch } from '@/api/mysqlDb'
import { sessionList } from '@/api/session'
import { transferList } from '@/api/sftp'
import { useUiStore } from '@/stores/ui'
import { useSettingsStore } from '@/stores/settings'
import { useSessionStore, type SessionConfig } from '@/stores/session'
import { useTerminalStore } from '@/stores/terminal'
import { debugLog } from '@/api/channels'
import { useDragOutWindow } from '@/composables/useDragOutWindow'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { check_update, install_update } from '@/api/updater'

const ui = useUiStore()
const sessionStore = useSessionStore()
/** 高功能设置（主题模式/终端项/SFTP 目录），持久化到后端 SQLite settings 表 */
const settings = useSettingsStore()
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

/** 工作区 Tab（终端 / 传输 / SFTP 双栏 / 隧道 / 快捷命令 / MySQL 六类） */
interface WorkTab {
  id: string
  type: 'terminal' | 'transfer' | 'tunnel' | 'sftp' | 'compose' | 'mysql'
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
  /** 数据库会话（session_type === 'mysql'，树图标与连接路由区分） */
  isMysql: boolean
  /** 数据库子节点（已连接 MySQL 会话的库，Navicat 风格） */
  isDbLeaf?: boolean
  /** 分区头（SSH 服务 / 数据库服务，可收缩） */
  isSection?: boolean
  /** 分区虚线分隔 */
  isSeparator?: boolean
}

// ---------------- 会话树 ----------------

const nodes = ref<SessionNode[]>([])
const keyword = ref('')
const expanded = ref(new Set<string>())

/** 已连接 MySQL 会话的数据库子节点（Navicat 风格：库作为会话子节点展示）。
    sessionId 为 null 表示树中无 MySQL 会话，库列表挂到数据库服务分区的独立节点下
    （host 为连接主机，作为独立节点显示名） */
const mysqlTreeDbs = ref<{ sessionId: string | null; host: string; databases: string[] } | null>(
  null,
)

/** 导航分区展开态（SSH 服务 / 数据库服务，单击分区头收缩） */
const openSections = ref(new Set(['section-ssh', 'section-db']))

// 连接状态变化：任意入口（会话树/工作台）连接成功后，把数据库子节点挂到
// 匹配的会话节点（按 host 匹配，MySQL 类型与用户名一致者优先）；断开时清空。
// 切库会产生新 conn_id（非 null），库列表随之自动刷新，不会误清
/** 加载当前连接的库列表并挂载到会话树（连接状态变化与树加载后均调用）。
    库列表挂 MySQL 会话节点下；树中无 MySQL 会话时挂数据库服务分区的独立节点 */
async function refreshMysqlTreeDbs(): Promise<void> {
  const store = useMysqlStore()
  const id = store.connId
  if (!id) {
    mysqlTreeDbs.value = null
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
    mysqlTreeDbs.value = { sessionId: match?.id ?? null, host: result.host, databases }
    debugLog(
      `[mysql-tree] conn=${id} databases=${databases.length} target=${match?.id ?? 'standalone'}`,
    )
    if (match) {
      expanded.value = new Set([...expanded.value, match.id])
    }
  } catch {
    mysqlTreeDbs.value = null
  }
}

// 连接状态变化：任意入口（会话树/工作台）连接成功后刷新树中库节点；断开时清空。
// 切库会产生新 conn_id（非 null），库列表随之自动刷新，不会误清
watch(
  () => useMysqlStore().connId,
  (id) => {
    if (!id) {
      mysqlTreeDbs.value = null
      return
    }
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
    // 树就绪后刷新库节点挂载（连接已建立时重新匹配会话节点）
    const mysqlStore = useMysqlStore()
    if (mysqlStore.connId) void refreshMysqlTreeDbs()
  } catch (e) {
    ui.toast(`加载会话列表失败：${String(e)}`, 'error')
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
  const walk = (list: SessionNode[], depth: number, section: 'ssh' | 'db') => {
    for (const n of list) {
      const isMysqlSession = !n.is_folder && n.config?.session_type === 'mysql'
      // 分区过滤：SSH 区跳过 MySQL 会话；数据库区只保留 MySQL 会话
      //（数据库区不显示文件夹，扁平列出其中嵌套的 MySQL 会话）
      if (section === 'ssh' && isMysqlSession) continue
      if (section === 'db' && n.is_folder) {
        walk(n.children ?? [], depth, section)
        continue
      }
      if (section === 'db' && !isMysqlSession) continue
      out.push({
        id: n.id,
        // 主机显示（Navicat 风格单行）：会话节点以主机地址为主，名称兜底
        name: n.is_folder ? n.name : (n.config?.host || n.name),
        depth,
        isFolder: !!n.is_folder,
        color: n.config?.color ?? null,
        isOpen: !!n.is_folder && (forceExpand || expanded.value.has(n.id)),
        hostLabel: '',
        encoding: (n.config?.encoding as string | undefined) ?? null,
        isMysql: isMysqlSession,
      })
      if (n.is_folder && (forceExpand || expanded.value.has(n.id))) {
        walk(n.children ?? [], depth + 1, section)
      } else if (section === 'db' && mysqlTreeDbs.value?.sessionId === n.id) {
        // 已连接的 MySQL 会话：数据库作为子节点展示（Navicat 风格）
        for (const dbName of mysqlTreeDbs.value.databases) {
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
          })
        }
      }
    }
  }

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

  // SSH 服务分区：文件夹 + SSH 会话
  sectionHeader('section-ssh', 'SSH 服务')
  if (openSections.value.has('section-ssh')) {
    walk(filterTree(nodes.value, kw), 0, 'ssh')
  }

  // 分区虚线分隔
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

  // 数据库服务分区：MySQL 会话 + 库列表
  sectionHeader('section-db', '数据库服务')
  if (openSections.value.has('section-db')) {
    walk(filterTree(nodes.value, kw), 0, 'db')
    // 树中无 MySQL 会话时：独立节点显示连接主机 + 库列表（Navicat 风格）
    if (mysqlTreeDbs.value && mysqlTreeDbs.value.sessionId === null) {
      out.push({
        id: 'db-standalone',
        name: mysqlTreeDbs.value.host || 'MySQL 数据库',
        depth: 0,
        isFolder: false,
        color: null,
        isOpen: false,
        hostLabel: '',
        encoding: null,
        isMysql: true,
        isDbLeaf: false,
      })
      for (const dbName of mysqlTreeDbs.value.databases) {
        out.push({
          id: `db-standalone-${dbName}`,
          name: dbName,
          depth: 1,
          isFolder: false,
          color: null,
          isOpen: false,
          hostLabel: '',
          encoding: null,
          isMysql: false,
          isDbLeaf: true,
        })
      }
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
  // 会话单击即连接：已有该会话终端 Tab 时直接激活，不重复建连（双击也不会开两个 Tab）
  const existing = tabs.value.find((t) => t.type === 'terminal' && t.sessionId === node.id)
  if (existing) {
    activeId.value = existing.id
    return
  }
  const target = findNode(nodes.value, node.id)
  if (!target) return
  if (target.config?.session_type === 'mysql') {
    void connectMysqlSession(target)
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
  if (node.isDbLeaf || node.isSection || node.id === 'db-standalone') return
  selectedId.value = node.id
  treeMenu.x = e.clientX
  treeMenu.y = e.clientY
  treeMenu.node = node
  treeMenu.visible = true
}

/** 右键菜单"连接"：打开该会话终端 */
function menuConnect(): void {
  treeMenu.visible = false
  if (treeMenu.node && !treeMenu.node.isFolder) {
    const target = findNode(nodes.value, treeMenu.node.id)
    if (!target) return
    // 数据库会话：切到 MySQL 工作台并按会话配置建连（不开终端）
    if (target.config?.session_type === 'mysql') {
      void connectMysqlSession(target)
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

/** 树中最近一次切库（时间戳去重：双击会连发 click+dblclick，800ms 内重复点击同一库直接跳过） */
let lastDbSwitchId = ''
let lastDbSwitchAt = 0

/** 树中双击数据库子节点：切换 MySQL 当前库并刷新表清单 */
async function switchMysqlDbFromTree(node: FlatNode): Promise<void> {
  const mysqlStore = useMysqlStore()
  const connId = mysqlStore.connId
  if (!connId) return
  // 节点 id 格式 `db-${sessionId}-${dbName}` 或 `db-standalone-${dbName}`，库名取尾部
  const sessionId = mysqlTreeDbs.value?.sessionId
  const prefix = sessionId === null ? 'db-standalone-' : `db-${sessionId}-`
  if (!node.id.startsWith(prefix)) return
  const dbName = node.id.slice(prefix.length)
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

/** 右键菜单"设置"：打开该会话的会话选项对话框（会话模式，编辑写会话级键覆盖全局值） */
function menuSessionSettings(): void {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node || node.isFolder) return
  sshOptionsSessionId.value = node.id
  sshOptionsSessionName.value = node.name
  showSshOptionsDialog.value = true
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

/** 右键菜单"删除"：二次确认后删除（会话/文件夹，含 Rust 侧清理） */
async function menuDelete(): Promise<void> {
  treeMenu.visible = false
  const node = treeMenu.node
  if (!node) return
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

// ---------------- Tab 管理 ----------------

const tabs = ref<WorkTab[]>([])
const activeId = ref<string | null>(null)
const activeTab = computed(() => tabs.value.find((t) => t.id === activeId.value) ?? null)

/** 生成 Tab 唯一 ID（Tab ID 与会话 ID 无关） */
function genTabId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID()
  }
  return `tab-${Date.now()}-${Math.floor(Math.random() * 1e6)}`
}

/** 打开会话终端 Tab（Xshell 多标签：同一会话可重复开 Tab，每标签一条独立连接） */
function openTerminal(node: { id: string; name: string; color?: string | null }): void {
  const tab: WorkTab = {
    id: genTabId(),
    type: 'terminal',
    sessionId: node.id,
    connId: genTabId(),
    title: node.name,
    color: node.color ?? null,
  }
  tabs.value.push(tab)
  activeId.value = tab.id
  debugLog(`workspace openTerminal: tab=${tab.id} session=${node.id} tabs=${tabs.value.length}`)
  // 委托 terminalStore.openTerminal 建立 SSH 连接并把输出流绑到 TerminalPane
  void terminalStore
    .openTerminal({ id: node.id, name: node.name, color: node.color ?? null }, tab.connId)
    .catch((e) => {
      // 连接失败：移除 UI Tab（store 侧已回滚），展示错误信息
      ui.toast(`连接「${node.name}」失败：${e instanceof Error ? e.message : String(e)}`, 'error')
      const idx = tabs.value.findIndex((t) => t.id === tab.id)
      if (idx >= 0) tabs.value.splice(idx, 1)
      if (activeId.value === tab.id) {
        activeId.value = tabs.value[Math.min(idx, tabs.value.length - 1)]?.id ?? null
      }
    })
}

/**
 * 打开本地终端 Tab（byte-stream 终端）：无需表单与持久会话，直接创建，
 * 连接路由键 = local-<tabId>，读写经 local_shell_* 命令路由
 */
function openLocalTerminal(): void {
  const connKey = `local-${genTabId()}`
  const tab: WorkTab = {
    id: genTabId(),
    type: 'terminal',
    sessionId: connKey,
    connId: connKey,
    title: '本地终端',
    color: null,
  }
  tabs.value.push(tab)
  activeId.value = tab.id
  debugLog(`workspace openLocalTerminal: tab=${tab.id} conn=${connKey}`)
  void terminalStore
    .openTerminal({ id: connKey, name: '本地终端', sessionType: 'local' }, connKey)
    .catch((e) => {
      ui.toast(`打开本地终端失败：${e instanceof Error ? e.message : String(e)}`, 'error')
      const idx = tabs.value.findIndex((t) => t.id === tab.id)
      if (idx >= 0) tabs.value.splice(idx, 1)
      if (activeId.value === tab.id) {
        activeId.value = tabs.value[Math.min(idx, tabs.value.length - 1)]?.id ?? null
      }
    })
}

/**
 * 打开 Telnet / 串口终端 Tab（byte-stream 终端，连接参数由表单收集，不持久化）：
 * 连接路由键 = byte-<tabId>，读写经 telnet_* 与 serial_* 命令路由
 */
function openByteStreamTerminal(params: {
  type: 'telnet' | 'serial'
  host?: string
  port?: number
  serialPort?: string
  baudRate?: number
}): void {
  const connKey = `byte-${genTabId()}`
  const title =
    params.type === 'telnet'
      ? `Telnet ${params.host}:${params.port}`
      : `串口 ${params.serialPort}@${params.baudRate}`
  const tab: WorkTab = {
    id: genTabId(),
    type: 'terminal',
    sessionId: connKey,
    connId: connKey,
    title,
    color: null,
  }
  tabs.value.push(tab)
  activeId.value = tab.id
  debugLog(`workspace openByteStreamTerminal: tab=${tab.id} type=${params.type} conn=${connKey}`)
  void terminalStore
    .openTerminal(
      {
        id: connKey,
        name: title,
        sessionType: params.type,
        host: params.host,
        port: params.port,
        serialPort: params.serialPort,
        baudRate: params.baudRate,
      },
      connKey,
    )
    .catch((e) => {
      ui.toast(`连接失败：${e instanceof Error ? e.message : String(e)}`, 'error')
      const idx = tabs.value.findIndex((t) => t.id === tab.id)
      if (idx >= 0) tabs.value.splice(idx, 1)
      if (activeId.value === tab.id) {
        activeId.value = tabs.value[Math.min(idx, tabs.value.length - 1)]?.id ?? null
      }
    })
}

/** 树右键菜单：新建本地终端（直接创建，无需表单） */
function menuNewLocal(): void {
  treeMenu.visible = false
  openLocalTerminal()
}

/** 树右键菜单：新建 Telnet / 串口连接（打开最小表单） */
function menuNewByteStream(type: 'telnet' | 'serial'): void {
  treeMenu.visible = false
  byteStreamFormType.value = type
  showByteStreamForm.value = true
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

/** Ctrl+T / 加号：优先打开当前选中会话的终端 */
function onCreate(): void {
  if (selectedId.value) {
    const node = findNode(nodes.value, selectedId.value)
    if (node && !node.is_folder) {
      openTerminal(node)
      return
    }
  }
  ui.toast('请先在左侧选择一个会话', 'warning')
}

/** 关闭 Tab；终端 Tab 委托 terminalStore 断开会话并清理 Rust 侧 session/PTY（架构红线） */
function closeTab(id: string): void {
  const idx = tabs.value.findIndex((t) => t.id === id)
  if (idx < 0) return
  const [closed] = tabs.value.splice(idx, 1)
  if (closed?.type === 'terminal' && closed.connId) {
    // store 侧同步断开连接、清理标签与状态（按连接键路由，含未建立连接的兜底）
    void terminalStore.closeBySessionId(closed.connId)
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

/** FlexTabs 渲染数据（按会话 color 着色） */
const tabItems = computed<FlexTabItem[]>(() =>
  tabs.value.map((t) => ({
    id: t.id,
    title: t.title,
    color: t.color ?? undefined,
    closable: true,
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

/** Telnet / 串口连接表单（byte-stream 终端新建入口，不持久化） */
const showByteStreamForm = ref(false)
const byteStreamFormType = ref<'telnet' | 'serial'>('telnet')

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

// ---------------- 监控 / 日志（P1） ----------------

/** 服务器监控抽屉开关 */
const showMonitor = ref(false)
/** 会话日志查看器开关 */
const showLogViewer = ref(false)
/** 主密码设置对话框开关（首次设置 + 修改/校验二合一） */
const showMasterPassword = ref(false)
/** 高功能设置对话框开关 + 默认分区（选项菜单"设置"/帮助菜单"关于"入口） */
const showSettings = ref(false)
const settingsSection = ref<'appearance' | 'terminal' | 'sftp' | 'data' | 'security' | 'about'>(
  'appearance',
)

/** SSH 选项对话框开关（标签右键菜单/菜单栏"会话设置"入口） */
const showSshOptionsDialog = ref(false)
/** 会话设置目标：会话节点 id 与会话名（会话模式，编辑写会话级键覆盖全局值） */
const sshOptionsSessionId = ref<string | null>(null)
const sshOptionsSessionName = ref('')

/** 打开会话选项对话框（会话模式）；无目标会话（非终端 Tab）时 toast 提示 */
function openSessionSettingsFor(tab: { sessionId?: string; title: string } | null): void {
  if (!tab || !tab.sessionId) {
    ui.toast('请先选择一个会话终端', 'warning')
    return
  }
  sshOptionsSessionId.value = tab.sessionId
  sshOptionsSessionName.value = tab.title
  showSshOptionsDialog.value = true
}

/** 标签右键菜单"会话设置"：按目标 Tab 解析会话并打开 */
function onSessionSettings(tabId: string): void {
  openSessionSettingsFor(tabs.value.find((t) => t.id === tabId) ?? null)
}

/** 活动终端 Tab 的连接路由键（监控/快速命令/日志均按连接键路由；无终端 Tab 时为 null） */
const activeTerminalId = computed(() => {
  const tab = activeTab.value
  return tab?.type === 'terminal' && tab.connId ? tab.connId : null
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
    case 'quit':
      // 关闭窗口（当前版本关闭即隐藏到托盘）
      await getCurrentWindow().close()
      break
    case 'copy':
    case 'paste':
    case 'select-all':
      ui.toast('请在终端内使用对应快捷键', 'info')
      break
    case 'toggle-nav':
      ui.navCollapsed = !ui.navCollapsed
      break
    case 'toggle-theme':
      ui.toggleTheme()
      // 与设置对话框同步：菜单切换主题后更新设置中的主题模式并持久化
      settings.setThemeMode(ui.theme)
      break
    case 'toggle-nav-autohide':
      ui.navAutoHide = !ui.navAutoHide
      break
    case 'toggle-quickbar':
      ui.quickBarVisible = !ui.quickBarVisible
      break
    case 'transfer':
      openTransferTab()
      break
    case 'sftp':
      openSftpTab()
      break
    // P1 视图入口
    case 'monitor':
      showMonitor.value = true
      break
    case 'quick-command':
      openComposeTab()
      break
    case 'tunnel':
      openTunnelTab()
      break
    case 'session-settings':
      // 会话设置：对当前活动会话打开（会话模式，编辑写会话级键）
      openSessionSettingsFor(activeTab.value ?? null)
      break
    case 'mysql':
      openMysqlTab()
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

onMounted(async () => {
  // 全局快捷键（Xshell 惯例）
  window.addEventListener('keydown', ui.handleKeydown)
  offs.push(
    ui.registerShortcut('ctrl+t', onCreate),
    ui.registerShortcut('ctrl+w', () => {
      if (activeId.value) closeTab(activeId.value)
    }),
    ui.registerShortcut('ctrl+tab', cycleTab),
  )
  for (let i = 1; i <= 9; i++) {
    const idx = i - 1
    offs.push(
      ui.registerShortcut(`alt+${i}`, () => {
        const tab = tabs.value[idx]
        if (tab) activeId.value = tab.id
      }),
    )
  }

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
  } catch (e) {
    console.error('注册事件监听失败', e)
  }

  await Promise.allSettled([loadTree(), loadTransferSnapshot()])

  // 标签拖出新窗口（P1）：新窗口 URL 带 ?session=<id>，启动后自动打开对应会话终端
  const urlSessionId = new URLSearchParams(window.location.search).get('session')
  if (urlSessionId) {
    const node = findNode(nodes.value, urlSessionId)
    if (node && !node.is_folder) {
      openTerminal(node)
    } else {
      ui.toast('未找到对应会话，无法自动打开终端', 'warning')
    }
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
    <MenuBar @action="onMenuAction" />
    <ToolBar
      @new-session="openSessionForm"
      @new-folder="showFolderDialog = true"
      @connect="onCreate"
      @disconnect="disconnectActive"
      @search="focusSearch"
      @transfer="openTransferTab"
      @sftp="openSftpTab"
      @help="ui.toast('FyShell P0 — Tauri 2 + Vue 3 + Vuetify 3', 'info')"
      @quick-connect="quickConnect"
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
              }"
              :style="{ paddingLeft: `${8 + node.depth * 14}px` }"
              @click="onNodeClick(node)"
              @keydown="onNodeKeydown(node, $event)"
              @contextmenu.prevent="onTreeContextmenu(node, $event)"
            >
              <v-icon
                :icon="node.isSection
                  ? (node.isOpen ? 'mdi-chevron-down' : 'mdi-chevron-right')
                  : node.isFolder
                    ? (node.isOpen ? 'mdi-folder-open' : 'mdi-folder')
                    : node.isDbLeaf
                      ? 'mdi-database-outline'
                      : (node.isMysql ? 'mdi-database' : 'mdi-console')"
                size="14"
                class="mr-1"
                :style="!node.isFolder && !node.isSection && node.color ? { color: node.color } : undefined"
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

      <!-- 树右键菜单：连接 / 重命名 / 删除 -->
      <v-menu
        v-model="treeMenu.visible"
        :target="[treeMenu.x, treeMenu.y]"
        location="bottom start"
        origin="auto"
        :close-on-content-click="true"
      >
        <v-list density="compact">
          <v-list-item v-if="treeMenu.node && !treeMenu.node.isFolder" @click="menuConnect">
            <v-list-item-title>连接</v-list-item-title>
          </v-list-item>
          <v-list-item v-if="treeMenu.node && !treeMenu.node.isFolder" @click="menuSessionSettings">
            <v-list-item-title>设置</v-list-item-title>
          </v-list-item>
          <v-divider />
          <!-- 新建连接：本地终端 / Telnet / 串口（byte-stream 终端） -->
          <v-list-item @click="menuNewLocal">
            <v-list-item-title>新建本地终端</v-list-item-title>
          </v-list-item>
          <v-list-item @click="menuNewByteStream('telnet')">
            <v-list-item-title>Telnet 连接</v-list-item-title>
          </v-list-item>
          <v-list-item @click="menuNewByteStream('serial')">
            <v-list-item-title>串口终端</v-list-item-title>
          </v-list-item>
          <v-divider />
          <v-list-item @click="menuRename">
            <v-list-item-title>重命名</v-list-item-title>
          </v-list-item>
          <v-divider />
          <v-list-item @click="menuDelete">
            <v-list-item-title class="text-error">删除</v-list-item-title>
          </v-list-item>
        </v-list>
      </v-menu>

      <!-- 折叠后的展开入口（独立兄弟层，避免随 aside 的 v-show 一并隐藏而无法重新展开） -->
      <div v-if="ui.navCollapsed && !ui.navAutoHide" class="workspace__nav-expand">
        <v-btn
          icon="mdi-chevron-right"
          size="20"
          variant="text"
          title="展开导航"
          @click="ui.navCollapsed = false"
        />
      </div>

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
        />
        <div class="workspace__content">
          <!-- v-show 保持终端 Tab 存活，切换不销毁会话状态 -->
          <div
            v-for="tab in tabs"
            :key="tab.id"
            v-show="tab.id === activeId"
            class="workspace__pane"
          >
            <TerminalPane
              v-if="tab.type === 'terminal' && tab.connId"
              :session-id="tab.connId"
              :session-node-id="tab.sessionId"
            />
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
          </div>
          <div v-if="tabs.length === 0" class="workspace__empty">
            <v-icon icon="mdi-console" size="48" class="mb-2" />
            <div class="workspace__empty-hint">双击左侧会话打开终端，Ctrl+T 新建标签</div>
          </div>
        </div>
      </main>
    </div>

    <!-- 监控迷你条（P1：活动会话 CPU/内存/网络实时指示，点击打开抽屉） -->
    <MonitorMiniBar
      :session-id="activeTerminalId"
      :session-name="activeSessionName"
      @open="showMonitor = true"
    />

    <!-- 快速命令栏（Xshell 风格，查看菜单可开关） -->
    <QuickCommandBar :session-id="activeTerminalId" />

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

    <!-- 服务器监控抽屉（P1） -->
    <MonitorDrawer
      v-model="showMonitor"
      :session-id="activeTerminalId"
      :session-name="activeSessionName"
    />

    <!-- 会话日志查看器（P1：活动会话日志落盘开关与查看） -->
    <v-dialog v-model="showLogViewer" width="640">
      <LogViewer v-if="activeTerminalId" :session-id="activeTerminalId" />
    </v-dialog>

    <!-- 会话表单（新建 / 编辑 / 快速连接） -->
    <SessionForm
      v-model="showSessionForm"
      :session="editingSession"
      :preset-host="presetHost"
      @saved="onSessionSaved"
    />

    <!-- Telnet / 串口连接表单（byte-stream 终端，不持久化到会话树） -->
    <ByteStreamForm
      v-model="showByteStreamForm"
      :type="byteStreamFormType"
      @saved="openByteStreamTerminal"
    />

    <!-- 新建文件夹对话框 -->
    <v-dialog v-model="showFolderDialog" width="360">
      <v-card>
        <v-card-title class="text-subtitle-1">新建文件夹</v-card-title>
        <v-card-text>
          <v-text-field
            v-model="folderName"
            label="文件夹名称"
            density="compact"
            autofocus
            @keydown.enter="createFolder"
          />
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="showFolderDialog = false">取消</v-btn>
          <v-btn color="primary" @click="createFolder">确定</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- 主密码设置对话框（首次设置 + 修改/校验） -->
    <MasterPasswordDialog v-model="showMasterPassword" />

    <!-- 高功能设置对话框（外观/终端/SFTP/数据/安全/关于），主密码为快捷入口 -->
    <SettingsDialog
      v-model="showSettings"
      :initial-section="settingsSection"
      @open-master-password="showMasterPassword = true"
    />

    <!-- SSH 选项对话框（标签右键/菜单栏"会话设置"入口；会话模式编辑写会话级键） -->
    <SshOptionsDialog
      v-model="showSshOptionsDialog"
      :session-id="sshOptionsSessionId ?? undefined"
      :session-name="sshOptionsSessionName"
    />

    <!-- 全局弹层（确认 / toast / 主题同步） -->
    <GlobalDialog />

    <!-- HostKey 首次确认弹层（连接新主机时 Rust 侧挂起等待确认，必须挂载） -->
    <HostkeyDialog />
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
  border-right: 1px solid var(--fy-chrome-border, #d5d9de);
  background: var(--fy-chrome-bg, #f0f2f5);
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
  height: 24px;
  flex: none;
  padding: 0 6px;
  border: 1px solid var(--fy-chrome-border, #d5d9de);
  border-radius: 3px;
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
  font-size: 12px;
}

.workspace__search-input:focus-visible {
  outline: none;
}

.workspace__search:focus-within {
  border-color: rgb(var(--v-theme-primary));
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
  font-size: 12px;
  white-space: nowrap;
  user-select: none;
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
  font-size: 12px;
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
  background: rgb(var(--v-theme-primary, 82 132 255) / 0.18);
}

.workspace__node-name {
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 分区虚线分隔（SSH 服务 / 数据库服务） */
.workspace__tree-sep {
  height: 0;
  border-top: 1px dashed #c6c9ce;
  margin: 4px 10px;
  user-select: none;
}

/* 分区头：浅色文字 + 紧凑行高，单击可收缩 */
.workspace__tree-node--section {
  color: rgb(var(--v-theme-on-surface) / 0.75);
}

.workspace__tree-empty {
  padding: 12px 8px;
  font-size: 12px;
  color: rgb(var(--v-theme-on-surface) / 0.5);
}

/* 停靠模式收起后的展开入口 */
.workspace__nav-expand {
  position: absolute;
  left: 0;
  top: 40px;
  z-index: 9;
  background: var(--fy-chrome-bg, #f0f2f5);
  border: 1px solid var(--fy-chrome-border, #d5d9de);
  border-left: none;
  border-radius: 0 4px 4px 0;
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

/* 空态提示文字：主区中央大字提示，14px 标题档 */
.workspace__empty-hint {
  font-size: 14px;
  color: rgb(var(--v-theme-on-surface) / 0.55);
}
</style>
