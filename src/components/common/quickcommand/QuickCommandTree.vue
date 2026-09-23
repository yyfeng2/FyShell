<template>
  <div class="qc-tree" @contextmenu.prevent="showMenu($event, null)">
    <!-- 顶部工具栏 -->
    <div class="qc-tree__header">
      <v-icon size="small" class="mr-1">mdi-console-line</v-icon>
      <span class="qc-tree__title">{{ insertMode ? '插入命令' : '快捷命令' }}</span>
      <v-spacer />
      <template v-if="!insertMode">
        <v-tooltip text="新建命令" location="bottom">
          <template #activator="{ props: activatorProps }">
            <v-btn v-bind="activatorProps" icon="mdi-plus" size="x-small" variant="text" @click="openEditor(null, 'command-create')" />
          </template>
        </v-tooltip>
        <v-tooltip text="新建文件夹" location="bottom">
          <template #activator="{ props: activatorProps }">
            <v-btn v-bind="activatorProps" icon="mdi-folder-plus-outline" size="x-small" variant="text" @click="openFolderEditor(null)" />
          </template>
        </v-tooltip>
        <v-tooltip text="刷新" location="bottom">
          <template #activator="{ props: activatorProps }">
            <v-btn v-bind="activatorProps" icon="mdi-refresh" size="x-small" variant="text" :loading="qcStore.loading" @click="qcStore.load()" />
          </template>
        </v-tooltip>
      </template>
    </div>

    <!-- 搜索过滤 -->
    <div class="qc-tree__search">
      <v-text-field
        v-model="qcStore.search"
        placeholder="搜索命令名称或内容"
        prepend-inner-icon="mdi-magnify"
        density="compact"
        variant="outlined"
        hide-details
        clearable
      />
    </div>

    <!-- 树状命令列表 -->
    <div class="qc-tree__body">
      <v-list density="compact" class="qc-tree__list">
        <v-list-item
          v-for="item in flatNodes"
          :key="`${item.node.kind}-${item.node.id}`"
          class="qc-tree__item"
          :style="{ paddingInlineStart: `${12 + item.depth * 16}px` }"
          @click="onItemClick(item)"
          @dblclick="onDoubleClick(item)"
          @contextmenu.prevent.stop="showMenu($event, item)"
        >
          <template #prepend>
            <v-icon
              v-if="isFolder(item.node)"
              size="small"
              :color="item.expanded ? 'primary' : undefined"
            >
              {{ item.expanded ? 'mdi-folder-open' : 'mdi-folder-outline' }}
            </v-icon>
            <v-icon v-else size="small" color="primary">mdi-code-tags</v-icon>
          </template>

          <v-list-item-title class="qc-tree__label">
            {{ item.node.name }}
            <span v-if="!isFolder(item.node)" class="qc-tree__preview">{{ previewOf(item.node) }}</span>
          </v-list-item-title>

          <template #append>
            <v-icon v-if="isFolder(item.node)" size="x-small">
              {{ item.expanded ? 'mdi-chevron-up' : 'mdi-chevron-down' }}
            </v-icon>
            <!-- 发送图标：hover 显示，点击后选择目标会话 -->
            <v-icon
              v-else-if="!insertMode"
              size="x-small"
              class="qc-tree__send-icon"
              @click.stop="openSendDialog(commandOf(item.node))"
            >
              mdi-send-outline
            </v-icon>
          </template>
        </v-list-item>
      </v-list>
      <div v-if="flatNodes.length === 0" class="qc-tree__empty">
        {{ qcStore.search.trim() ? '无匹配命令' : '暂无快捷命令，右键新建' }}
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
      <v-list density="compact" class="qc-tree__menu">
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

    <!-- 命令编辑 / 新建文件夹对话框 -->
    <v-dialog v-model="editor.visible" width="460">
      <v-card>
        <v-card-title class="d-flex align-center">{{ editorTitle }}
          <v-spacer />
          <v-btn
          icon="mdi-close"
          size="x-small"
          variant="text"
          title="关闭"
          @click="editor.visible = false"
          />
        </v-card-title>
        <v-card-text>
          <div class="fy-field-row">
            <span class="fy-field-row__label">名称</span>
            <v-text-field v-model="editor.name" density="compact" autofocus @keyup.enter="submitEditor" />
          </div>
          <div class="fy-field-row">
            <span class="fy-field-row__label">命令内容（支持多行，发送时按行回车）</span>
            <v-textarea
              v-if="editor.mode === 'command-create' || editor.mode === 'command-rename'"
              v-model="editor.commandText"
              density="compact"
              variant="outlined"
              rows="3"
            />
          </div>
          <div class="fy-field-row">
            <span class="fy-field-row__label">所属分组</span>
            <v-select
              v-if="editor.mode === 'command-create' || editor.mode === 'command-rename'"
              v-model="editor.groupId"
              :items="folderOptions"
              density="compact"
              variant="outlined"
              clearable
            />
          </div>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="editor.visible = false">取消</v-btn>
          <v-btn color="primary" @click="submitEditor">确定</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- 发送目标选择对话框 -->
    <v-dialog v-model="sendDialog.visible" width="420">
      <v-card>
        <v-card-title class="d-flex align-center">发送到会话
          <v-spacer />
          <v-btn
          icon="mdi-close"
          size="x-small"
          variant="text"
          title="关闭"
          @click="sendDialog.visible = false"
          />
        </v-card-title>
        <v-card-text class="qc-tree__send-body">
          <div v-if="connectedSessions.length === 0" class="text-medium-emphasis">
            暂无已连接会话，请先连接目标服务器
          </div>
          <template v-else>
            <v-checkbox
              v-model="allSelected"
              label="全部已连接会话"
              density="compact"
              hide-details
              :indeterminate="sendDialog.selected.length > 0 && !allSelected"
            />
            <v-divider class="my-2" />
            <v-checkbox
              v-for="cfg in connectedSessions"
              :key="cfg.id"
              v-model="sendDialog.selected"
              :value="cfg.id"
              density="compact"
              hide-details
            >
              <template #label>
                <span>{{ cfg.name }}</span>
                <span class="qc-tree__preview ml-2">{{ cfg.username }}@{{ cfg.host }}</span>
              </template>
            </v-checkbox>
          </template>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="sendDialog.visible = false">取消</v-btn>
          <v-btn
            color="primary"
            :disabled="sendDialog.selected.length === 0"
            @click="confirmSend"
          >
            发送
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { useQuickCommandStore, isCommandFolder, commandTextOf } from '@/stores/quickCommand'
import { useSessionStore } from '@/stores/session'
import { useTerminalStore } from '@/stores/terminal'
import { useUiStore } from '@/stores/ui'
import type { QuickCommand, QuickCommandNode } from '@/api/types'

const props = defineProps<{
  /** 插入模式（Compose Pane 侧栏）：单击命令即插入草稿，隐藏右键 CRUD 与发送图标 */
  insertMode?: boolean
}>()

const emit = defineEmits<{
  /** 插入模式：单击命令插入草稿（由父级对接 ComposePane） */
  (e: 'insert', command: QuickCommand): void
  /** 发送完成：携带命令与目标会话（由父级做后续联动，如激活目标标签） */
  (e: 'send', command: QuickCommand, sessionIds: string[]): void
}>()

const qcStore = useQuickCommandStore()
const sessionStore = useSessionStore()
const terminalStore = useTerminalStore()
const uiStore = useUiStore()

const isFolder = isCommandFolder

function commandOf(node: QuickCommandNode): QuickCommand {
  return node as unknown as QuickCommand
}

function previewOf(node: QuickCommandNode): string {
  const text = commandTextOf(node).split('\n')[0] ?? ''
  return text.length > 24 ? `${text.slice(0, 24)}…` : text
}

// ---------- 树的展开与扁平化渲染 ----------
const expandedIds = ref<Set<string>>(new Set())

interface FlatNode {
  node: QuickCommandNode
  depth: number
  expanded: boolean
}

/** 搜索时全部展开；平时按展开状态渲染 */
const flatNodes = computed<FlatNode[]>(() => {
  const searching = (qcStore.search ?? '').trim().length > 0
  const out: FlatNode[] = []
  const walk = (list: QuickCommandNode[], depth: number): void => {
    for (const node of list) {
      if (isFolder(node)) {
        const expanded = searching || expandedIds.value.has(node.id)
        out.push({ node, depth, expanded })
        if (expanded) {
          const children = (node as unknown as { children: QuickCommandNode[] }).children
          walk(children, depth + 1)
        }
      } else {
        out.push({ node, depth, expanded: false })
      }
    }
  }
  walk(qcStore.filteredNodes, 0)
  return out
})

function toggleExpand(node: QuickCommandNode): void {
  const next = new Set(expandedIds.value)
  if (next.has(node.id)) next.delete(node.id)
  else next.add(node.id)
  expandedIds.value = next
}

function onItemClick(item: FlatNode): void {
  if (isFolder(item.node)) {
    toggleExpand(item.node)
    return
  }
  // 插入模式：单击命令即插入草稿
  if (props.insertMode) {
    emit('insert', commandOf(item.node))
  }
}

function onDoubleClick(item: FlatNode): void {
  // 插入模式下单击已处理，双击不再重复插入
  if (props.insertMode) return
  if (isFolder(item.node)) {
    toggleExpand(item.node)
    return
  }
  openSendDialog(commandOf(item.node))
}

// ---------- 右键菜单 ----------
const menu = reactive({ visible: false, x: 0, y: 0 })
const menuItems = ref<{ title: string; icon: string; action: () => void }[]>([])

function showMenu(e: MouseEvent, item: FlatNode | null): void {
  if (props.insertMode) return
  menu.x = e.clientX
  menu.y = e.clientY
  menuItems.value = buildMenuItems(item ? item.node : null)
  menu.visible = true
}

function buildMenuItems(node: QuickCommandNode | null): { title: string; icon: string; action: () => void }[] {
  if (!node) {
    return [
      { title: '新建命令', icon: 'mdi-plus', action: () => openEditor(null, 'command-create') },
      { title: '新建文件夹', icon: 'mdi-folder-plus-outline', action: () => openFolderEditor(null) },
    ]
  }
  if (isFolder(node)) {
    return [
      { title: '新建命令', icon: 'mdi-plus', action: () => openEditor(node.id, 'command-create') },
      { title: '新建文件夹', icon: 'mdi-folder-plus-outline', action: () => openFolderEditor(node.id) },
      { title: '重命名', icon: 'mdi-pencil-outline', action: () => startRename(node) },
      { title: '删除', icon: 'mdi-delete-outline', action: () => removeNode(node) },
    ]
  }
  return [
    { title: '发送到会话', icon: 'mdi-send-outline', action: () => openSendDialog(commandOf(node)) },
    { title: '重命名', icon: 'mdi-pencil-outline', action: () => startRename(node) },
    { title: '删除', icon: 'mdi-delete-outline', action: () => removeNode(node) },
  ]
}

// ---------- 命令 / 文件夹编辑 ----------
type EditorMode = 'command-create' | 'command-rename' | 'folder-create' | 'folder-rename'

const editor = reactive({
  visible: false,
  mode: 'command-create' as EditorMode,
  name: '',
  commandText: '',
  groupId: null as string | null,
  targetId: '',
  parentId: null as string | null,
})

const editorTitle = computed<string>(() => {
  switch (editor.mode) {
    case 'command-create':
      return '新建命令'
    case 'command-rename':
      return '编辑命令'
    case 'folder-create':
      return '新建文件夹'
    case 'folder-rename':
      return '重命名文件夹'
  }
  return ''
})

/** 分组下拉选项（新建/编辑命令时选择所属分组，遍历全量树） */
const folderOptions = computed<{ title: string; value: string }[]>(() => {
  const out: { title: string; value: string }[] = []
  const walk = (list: QuickCommandNode[], depth: number): void => {
    for (const node of list) {
      if (isFolder(node)) {
        out.push({ title: `${'　'.repeat(depth)}${node.name}`, value: node.id })
        walk((node as unknown as { children: QuickCommandNode[] }).children, depth + 1)
      }
    }
  }
  walk(qcStore.nodes, 0)
  return out
})

/** 新建命令（folderId = 预设所属分组）；folderId 为 null 表示根级 */
function openEditor(folderId: string | null, _mode: EditorMode): void {
  editor.mode = 'command-create'
  editor.name = ''
  editor.commandText = ''
  editor.groupId = folderId
  editor.targetId = ''
  editor.parentId = null
  editor.visible = true
}

/** 新建文件夹（parentId = 预设父分组；null 表示根级） */
function openFolderEditor(folderId: string | null): void {
  editor.mode = 'folder-create'
  editor.name = ''
  editor.parentId = folderId
  editor.visible = true
}

function startRename(node: QuickCommandNode): void {
  if (isFolder(node)) {
    editor.mode = 'folder-rename'
    editor.targetId = node.id
    editor.parentId = (node as unknown as { parent_id: string | null }).parent_id ?? null
    editor.commandText = ''
  } else {
    const cmd = commandOf(node)
    editor.mode = 'command-rename'
    editor.targetId = cmd.id
    editor.groupId = (cmd as unknown as { group_id: string | null }).group_id ?? null
    editor.commandText = cmd.command_text ?? ''
  }
  editor.name = node.name
  editor.visible = true
}

async function submitEditor(): Promise<void> {
  const name = editor.name.trim()
  if (!name) return
  try {
    if (editor.mode === 'folder-create' || editor.mode === 'folder-rename') {
      await qcStore.saveFolder({
        id: editor.mode === 'folder-rename' ? editor.targetId : crypto.randomUUID(),
        name,
        parent_id: editor.parentId ?? null,
      })
    } else {
      await qcStore.saveCommand({
        id: editor.mode === 'command-rename' ? editor.targetId : crypto.randomUUID(),
        name,
        command_text: editor.commandText,
        group_id: editor.groupId ?? null,
      })
    }
    editor.visible = false
  } catch (err) {
    console.error('[quick-command-tree] 保存失败:', err)
    uiStore.toast('保存失败，请重试', 'error')
  }
}

// ---------- 删除（ui store 二次确认） ----------
async function removeNode(node: QuickCommandNode): Promise<void> {
  const folder = isFolder(node)
  const label = folder ? `文件夹「${node.name}」（含其中所有命令）` : `命令「${node.name}」`
  try {
    const ok = await uiStore.confirm({
      title: '删除确认',
      message: `确定要删除${label}吗？此操作不可恢复。`,
      danger: true,
    })
    if (ok) await qcStore.remove(node.id, folder)
  } catch (err) {
    console.error('[quick-command-tree] 删除失败:', err)
  }
}

// ---------- 发送到会话 ----------
const sendDialog = reactive({
  visible: false,
  command: null as QuickCommand | null,
  selected: [] as string[],
})

/** 所有已连接会话（会话列表来自 useSessionStore，连接状态来自 useTerminalStore） */
const connectedSessions = computed(() =>
  sessionStore.allConfigs.filter((cfg) => terminalStore.isConnected(cfg.id)),
)

const allSelected = computed({
  get: (): boolean =>
    connectedSessions.value.length > 0 && sendDialog.selected.length === connectedSessions.value.length,
  set: (v: boolean): void => {
    sendDialog.selected = v ? connectedSessions.value.map((c) => c.id) : []
  },
})

function openSendDialog(command: QuickCommand): void {
  sendDialog.command = command
  // 默认勾选当前活动会话（活动标签第一个窗格对应的会话）
  const currentId = terminalStore.activeTab?.panes[0]?.sessionId
  sendDialog.selected =
    currentId && terminalStore.isConnected(currentId) ? [currentId] : []
  sendDialog.visible = true
}

async function confirmSend(): Promise<void> {
  const command = sendDialog.command
  if (!command || sendDialog.selected.length === 0) return
  const sessionIds = [...sendDialog.selected]
  // 命令末尾自动补换行，逐会话发送（command_text 可能为 null/undefined，取空串防御）
  const raw = command.command_text ?? ''
  const text = raw.endsWith('\n') ? raw : `${raw}\n`
  const payload = new TextEncoder().encode(text)
  const results = await Promise.allSettled(
    sessionIds.map((id) => terminalStore.writeToSession(id, payload)),
  )
  const failed = sessionIds.filter((_, i) => results[i].status === 'rejected')
  if (failed.length === 0) {
    uiStore.toast(`已发送到 ${sessionIds.length} 个会话`, 'success')
  } else {
    uiStore.toast(
      `发送完成：成功 ${sessionIds.length - failed.length} 个，失败 ${failed.length} 个`,
      'warning',
    )
  }
  sendDialog.visible = false
  emit('send', command, sessionIds)
}

// ---------- 挂载时加载 ----------
onMounted(() => {
  void qcStore.load()
})
</script>

<style scoped>
.qc-tree {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-width: 220px;
}

.qc-tree__header {
  display: flex;
  align-items: center;
  padding: 8px 12px 4px;
}

.qc-tree__title {
  font-weight: 400;
  font-size: 12px;
}

.qc-tree__body {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
}

.qc-tree__list {
  padding: 0 4px;
}

.qc-tree__item {
  min-height: 32px;
  user-select: none;
}

.qc-tree__item:hover {
  background: var(--fy-hover-bg);
}

.qc-tree__label {
  font-size: 12px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.qc-tree__preview {
  color: rgba(var(--v-theme-on-surface), 0.5);
  font-size: 12px;
  margin-left: 6px;
}

/* 会话以 checkbox 全量平铺，多会话时超高，限高后内部滚动 */
.qc-tree__send-body {
  max-height: 50vh;
  overflow-y: auto;
}

/* 发送图标：普通态弱化显示（触屏可达），hover/焦点时强调 */
.qc-tree__send-icon {
  opacity: 0.35;
}

.qc-tree__item:hover .qc-tree__send-icon,
.qc-tree__item:focus-within .qc-tree__send-icon {
  opacity: 1;
}

.qc-tree__empty {
  padding: 16px;
  text-align: center;
  color: rgba(var(--v-theme-on-surface), 0.5);
  font-size: 12px;
}
</style>
