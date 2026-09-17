<template>
  <div class="session-tree" @contextmenu.prevent="showMenu($event, null)">
    <!-- 顶部工具栏 -->
    <div class="session-tree__header">
      <v-icon size="small" class="mr-1">mdi-server-network</v-icon>
      <span class="session-tree__title">会话</span>
      <v-spacer />
      <v-tooltip text="新建会话" location="bottom">
        <template #activator="{ props: activatorProps }">
          <v-btn v-bind="activatorProps" icon="mdi-plus" size="x-small" variant="text" @click="openNewSession(null)" />
        </template>
      </v-tooltip>
      <v-tooltip text="新建文件夹" location="bottom">
        <template #activator="{ props: activatorProps }">
          <v-btn v-bind="activatorProps" icon="mdi-folder-plus-outline" size="x-small" variant="text" @click="openNewFolder(null)" />
        </template>
      </v-tooltip>
      <v-tooltip text="刷新" location="bottom">
        <template #activator="{ props: activatorProps }">
          <v-btn v-bind="activatorProps" icon="mdi-refresh" size="small" variant="text" :loading="store.loading" @click="store.load()" />
        </template>
      </v-tooltip>
    </div>

    <!-- 导航过滤栏：实时过滤节点 -->
    <div class="session-tree__search">
      <v-text-field
        v-model="store.search"
        placeholder="搜索名称或主机"
        prepend-inner-icon="mdi-magnify"
        density="compact"
        variant="outlined"
        hide-details
        clearable
      />
    </div>

    <!-- 会话树 -->
    <div class="session-tree__body">
      <v-list density="compact" class="session-tree__list">
        <v-list-item
          v-for="item in flatNodes"
          :key="`${item.node.kind}-${item.node.id}`"
          class="session-tree__item"
          :style="{ paddingInlineStart: `${12 + item.depth * 16}px` }"
          @click="onItemClick(item)"
          @dblclick="onDoubleClick(item)"
          @contextmenu.prevent.stop="showMenu($event, item)"
        >
          <template #prepend>
            <template v-if="isFolder(item.node)">
              <v-icon size="small" :color="item.expanded ? 'primary' : undefined">
                {{ item.expanded ? 'mdi-folder-open' : 'mdi-folder-outline' }}
              </v-icon>
            </template>
            <template v-else>
              <v-icon
                size="small"
                :color="store.isStarred(item.node.id) ? 'warning' : undefined"
                class="mr-1"
                @click.stop="store.toggleStar(item.node.id)"
              >
                {{ store.isStarred(item.node.id) ? 'mdi-star' : 'mdi-star-outline' }}
              </v-icon>
              <!-- Tab 着色色块 -->
              <span
                class="session-tree__color"
                :style="{ backgroundColor: colorOf(item.node) ?? 'transparent' }"
              />
            </template>
          </template>

          <v-list-item-title class="session-tree__label">
            {{ item.node.name }}
            <span v-if="!isFolder(item.node)" class="session-tree__host">{{ hostOf(item.node) }}</span>
          </v-list-item-title>

          <template #append>
            <v-icon v-if="isFolder(item.node)" size="x-small">
              {{ item.expanded ? 'mdi-chevron-up' : 'mdi-chevron-down' }}
            </v-icon>
            <v-icon
              v-else
              size="x-small"
              class="session-tree__connect-icon"
              @click.stop="emit('open', item.node.id)"
            >
              mdi-console
            </v-icon>
          </template>
        </v-list-item>
      </v-list>
      <div v-if="flatNodes.length === 0" class="session-tree__empty">
        {{ store.search.trim() ? '无匹配会话' : '暂无会话，右键新建' }}
      </div>
    </div>

    <!-- 右键菜单 -->
    <v-menu
      v-model="menu.visible"
      :target="[menu.x, menu.y]"
      location="bottom start"
      origin="auto"
      :close-on-content-click="true"
    >
      <v-list density="compact" class="session-tree__menu">
        <v-list-item
          v-for="(item, i) in menuItems"
          :key="i"
          :prepend-icon="item.icon"
          @click="item.action()"
        >
          <v-list-item-title>{{ item.title }}</v-list-item-title>
        </v-list-item>
      </v-list>
    </v-menu>

    <!-- 重命名 / 新建文件夹对话框 -->
    <v-dialog v-model="rename.visible" width="400">
      <v-card>
        <v-card-title>{{ rename.mode === 'rename' ? '重命名' : '新建文件夹' }}</v-card-title>
        <v-card-text>
          <v-form ref="renameFormRef" @submit.prevent="submitRename">
            <v-text-field
              v-model="rename.name"
              label="名称"
              density="compact"
              autofocus
              :rules="[rules.required]"
              @keyup.enter="submitRename"
            />
          </v-form>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="rename.visible = false">取消</v-btn>
          <v-btn color="primary" @click="submitRename">确定</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- 会话配置对话框 -->
    <SessionForm
      v-model="formVisible"
      :session="editingSession"
      :folder-id="presetFolderId"
      @saved="onSaved"
    />

    <!-- Telnet / 串口连接表单（byte-stream 终端，不持久化到会话树） -->
    <ByteStreamForm
      v-model="byteStreamFormVisible"
      :type="byteStreamFormType"
      @saved="onByteStreamSaved"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, reactive, ref } from 'vue'
import {
  useSessionStore,
  isFolderNode,
  nodeConfig,
  type SessionConfig,
  type SessionNode,
} from '@/stores/session'
import { useTerminalStore } from '@/stores/terminal'
import { useUiStore } from '@/stores/ui'
import SessionForm from './SessionForm.vue'
import ByteStreamForm from './ByteStreamForm.vue'

const emit = defineEmits<{
  /** 双击会话：发起连接（由父级对接 terminal store 的 openTerminal） */
  (e: 'open', sessionId: string): void
}>()

const store = useSessionStore()
const uiStore = useUiStore()
/** 终端连接管理：本地终端新建入口直接经 store 打开 byte-stream 终端 */
const terminalStore = useTerminalStore()

const isFolder = isFolderNode

const rules = {
  required: (v: string | null | undefined) => (v !== null && v !== undefined && v.trim() !== '') || '必填项',
}
const renameFormRef = ref<{ validate: () => Promise<{ valid: boolean }>; resetValidation: () => void } | null>(null)

function colorOf(node: SessionNode): string | null {
  return nodeConfig(node)?.color ?? null
}

function hostOf(node: SessionNode): string {
  const cfg = nodeConfig(node)
  return cfg ? `${cfg.username}@${cfg.host}` : ''
}

// ---------- 树的展开与扁平化渲染 ----------
const expandedIds = ref<Set<string>>(new Set())

interface FlatNode {
  node: SessionNode
  depth: number
  expanded: boolean
}

/** 搜索时全部展开；平时按展开状态渲染 */
const flatNodes = computed<FlatNode[]>(() => {
  const searching = (store.search ?? '').trim().length > 0
  const out: FlatNode[] = []
  const walk = (list: SessionNode[], depth: number): void => {
    for (const node of list) {
      const expanded = searching || expandedIds.value.has(node.id)
      out.push({ node, depth, expanded })
      if (isFolder(node) && expanded) walk(node.children, depth + 1)
    }
  }
  walk(store.filteredNodes, 0)
  return out
})

function onItemClick(item: FlatNode): void {
  if (isFolder(item.node)) toggleExpand(item.node)
}

function onDoubleClick(item: FlatNode): void {
  if (isFolder(item.node)) {
    toggleExpand(item.node)
  } else {
    emit('open', item.node.id)
  }
}

function toggleExpand(node: SessionNode): void {
  const next = new Set(expandedIds.value)
  if (next.has(node.id)) next.delete(node.id)
  else next.add(node.id)
  expandedIds.value = next
}

// ---------- 右键菜单 ----------
const menu = reactive({ visible: false, x: 0, y: 0 })
const menuItems = ref<{ title: string; icon: string; action: () => void }[]>([])

function showMenu(e: MouseEvent, item: FlatNode | null): void {
  menu.x = e.clientX
  menu.y = e.clientY
  menuItems.value = buildMenuItems(item ? item.node : null)
  menu.visible = true
}

function buildMenuItems(node: SessionNode | null): { title: string; icon: string; action: () => void }[] {
  if (!node) {
    return [
      { title: '新建会话', icon: 'mdi-plus', action: () => openNewSession(null) },
      { title: '新建文件夹', icon: 'mdi-folder-plus-outline', action: () => openNewFolder(null) },
      ...byteStreamMenuItems(),
    ]
  }
  if (isFolder(node)) {
    return [
      { title: '新建会话', icon: 'mdi-plus', action: () => openNewSession(node.id) },
      { title: '新建文件夹', icon: 'mdi-folder-plus-outline', action: () => openNewFolder(node.id) },
      { title: '重命名', icon: 'mdi-pencil-outline', action: () => startRename(node) },
      { title: '删除', icon: 'mdi-delete-outline', action: () => removeNode(node) },
      ...byteStreamMenuItems(),
    ]
  }
  return [
    { title: '重命名', icon: 'mdi-pencil-outline', action: () => startRename(node) },
    { title: '删除', icon: 'mdi-delete-outline', action: () => removeNode(node) },
  ]
}

/** 新建连接子菜单项：本地终端 / Telnet / 串口（byte-stream 终端） */
function byteStreamMenuItems(): { title: string; icon: string; action: () => void }[] {
  return [
    { title: '新建本地终端', icon: 'mdi-console-line', action: openLocalTerminal },
    { title: 'Telnet 连接', icon: 'mdi-lan', action: () => openByteStreamForm('telnet') },
    { title: '串口终端', icon: 'mdi-serial-port', action: () => openByteStreamForm('serial') },
  ]
}

// ---------- 新建连接（byte-stream 终端入口） ----------
/** Telnet / 串口连接表单可见性与类型 */
const byteStreamFormVisible = ref(false)
const byteStreamFormType = ref<'telnet' | 'serial'>('telnet')

/** 新建本地终端：无需表单与持久会话，直接创建 byte-stream 终端 Tab */
function openLocalTerminal(): void {
  const connKey = `local-${crypto.randomUUID()}`
  void terminalStore
    .openTerminal({ id: connKey, name: '本地终端', sessionType: 'local' }, connKey)
    .catch((err) => {
      console.error('[session-tree] 打开本地终端失败:', err)
      uiStore.toast(`打开本地终端失败：${String(err)}`, 'error')
    })
}

/** 新建 Telnet / 串口连接：打开最小表单 */
function openByteStreamForm(type: 'telnet' | 'serial'): void {
  byteStreamFormType.value = type
  byteStreamFormVisible.value = true
}

/** Telnet / 串口表单提交：直接打开 byte-stream 终端 Tab（不持久化到会话树） */
function onByteStreamSaved(params: {
  type: 'telnet' | 'serial'
  host?: string
  port?: number
  serialPort?: string
  baudRate?: number
}): void {
  const connKey = `byte-${crypto.randomUUID()}`
  const title =
    params.type === 'telnet'
      ? `Telnet ${params.host}:${params.port}`
      : `串口 ${params.serialPort}@${params.baudRate}`
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
    .catch((err) => {
      console.error('[session-tree] 连接失败:', err)
      uiStore.toast(`连接失败：${String(err)}`, 'error')
    })
}

// ---------- 新建 / 重命名 ----------
const formVisible = ref(false)
const editingSession = ref<SessionConfig | null>(null)
const presetFolderId = ref<string | null>(null)

function openNewSession(folderId: string | null = null): void {
  editingSession.value = null
  presetFolderId.value = folderId
  formVisible.value = true
}

function openNewFolder(parentId: string | null = null): void {
  rename.mode = 'folder-create'
  rename.name = ''
  rename.parentId = parentId
  rename.visible = true
  void nextTick(() => renameFormRef.value?.resetValidation())
}

const rename = reactive({
  visible: false,
  mode: 'rename' as 'rename' | 'folder-create' | 'folder-rename',
  name: '',
  targetId: '',
  parentId: null as string | null,
})

function startRename(node: SessionNode): void {
  if (isFolder(node)) {
    rename.mode = 'folder-rename'
    rename.targetId = node.id
    rename.parentId = node.parent_id
  } else {
    rename.mode = 'rename'
    rename.targetId = node.id
  }
  rename.name = node.name
  rename.visible = true
  void nextTick(() => renameFormRef.value?.resetValidation())
}

async function submitRename(): Promise<void> {
  if (renameFormRef.value) {
    const { valid } = await renameFormRef.value.validate()
    if (!valid) return
  }
  const value = rename.name.trim()
  if (!value) return
  try {
    if (rename.mode === 'folder-create') {
      await store.saveFolder({ id: crypto.randomUUID(), name: value, parent_id: rename.parentId ?? null })
    } else if (rename.mode === 'folder-rename') {
      await store.saveFolder({ id: rename.targetId, name: value, parent_id: rename.parentId ?? null })
    } else {
      const cfg = store.getSessionById(rename.targetId)
      if (cfg) await store.save({ ...cfg, name: value })
    }
    rename.visible = false
  } catch (err) {
    console.error('[session-tree] 重命名失败:', err)
  }
}

// ---------- 删除（ui store 二次确认） ----------
async function removeNode(node: SessionNode): Promise<void> {
  const folder = isFolder(node)
  const label = folder ? `文件夹「${node.name}」（含其中所有会话）` : `会话「${node.name}」`
  try {
    const ok = await uiStore.confirm({
      title: '删除确认',
      message: `确定要删除${label}吗？此操作不可恢复。`,
      danger: true,
    })
    if (ok) await store.remove(node.id, folder)
  } catch (err) {
    console.error('[session-tree] 删除失败:', err)
  }
}

// ---------- 会话表单联动 ----------
function onSaved(): void {
  formVisible.value = false
  editingSession.value = null
}

// ---------- 挂载时加载 ----------
// Ctrl+T 新标签由 WorkspaceView 通过 ui.registerShortcut 全局注册，此处不再监听，避免重复触发

onMounted(() => {
  void store.load()
})
</script>

<style scoped>
.session-tree {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-width: 220px;
}

.session-tree__header {
  display: flex;
  align-items: center;
  padding: 8px 12px 4px;
}

.session-tree__title {
  font-weight: 600;
  font-size: 0.9rem;
}

.session-tree__search {
  padding: 4px 8px;
}

.session-tree__body {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
}

.session-tree__list {
  padding: 0 4px;
}

.session-tree__item {
  min-height: 32px;
  user-select: none;
}

.session-tree__item:hover {
  background: rgba(var(--v-theme-on-surface), 0.06);
}

.session-tree__label {
  font-size: 0.85rem;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.session-tree__host {
  color: rgba(var(--v-theme-on-surface), 0.5);
  font-size: 0.75rem;
  margin-left: 6px;
}

.session-tree__color {
  display: inline-block;
  width: 10px;
  height: 10px;
  border-radius: 3px;
  margin-right: 6px;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.15);
}

/* 连接图标：普通态弱化显示（触屏可达），hover/焦点时强调 */
.session-tree__connect-icon {
  opacity: 0.35;
}

.session-tree__item:hover .session-tree__connect-icon,
.session-tree__item:focus-within .session-tree__connect-icon {
  opacity: 1;
}

.session-tree__empty {
  padding: 16px;
  text-align: center;
  color: rgba(var(--v-theme-on-surface), 0.5);
  font-size: 0.85rem;
}
</style>
