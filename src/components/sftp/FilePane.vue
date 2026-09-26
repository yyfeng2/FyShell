<template>
  <div class="file-pane" :class="{ 'file-pane--disabled': disabled }">
    <!-- 地址栏：上级 / 刷新 / 可编辑路径回车跳转 -->
    <div class="file-pane__toolbar">
      <v-btn
        icon
        variant="text"
        size="x-small"
        :disabled="isRoot || disabled"
        title="上级目录"
        @click="goUp"
      >
        <v-icon icon="mdi-chevron-up" size="16" />
      </v-btn>
      <v-btn
        icon
        variant="text"
        size="x-small"
        :disabled="loading || disabled"
        title="刷新"
        @click="refresh"
      >
        <v-icon icon="mdi-refresh" size="16" />
      </v-btn>
      <v-text-field
        v-model="pathInput"
        class="file-pane__path"
        density="compact"
        variant="outlined"
        hide-details
        placeholder="输入路径后回车跳转"
        :disabled="disabled"
        :title="currentPath"
        @keyup.enter="commitPath"
        @contextmenu.prevent="openPathMenu"
      />
      <!-- 收藏路径下拉：按窗格侧过滤，点击快速跳转，条目可单独移除 -->
      <v-menu :close-on-content-click="false">
        <template #activator="{ props: favProps }">
          <v-btn icon variant="text" size="x-small" v-bind="favProps" title="收藏路径">
            <v-icon :icon="isFavorited ? 'mdi-star' : 'mdi-star-outline'" size="16" />
          </v-btn>
        </template>
        <v-list density="compact" min-width="240" max-height="320">
          <v-list-item
            v-for="fav in favoriteForSide"
            :key="fav.id"
            :title="fav.path"
            @click="jumpFavorite(fav)"
          >
            <template #append>
              <v-btn
                icon="mdi-close"
                size="x-small"
                variant="text"
                title="取消收藏"
                @click.stop="removeFavorite(fav)"
              />
            </template>
          </v-list-item>
          <v-list-item v-if="!favoriteForSide.length" title="暂无收藏路径" disabled />
          <v-divider />
          <!-- 收藏入口：无激活器时的添加/取消动作（此前只有下拉且无添加入口，收藏功能不可用） -->
          <v-list-item
            :title="isFavorited ? '取消收藏当前路径' : '收藏当前路径'"
            :prepend-icon="isFavorited ? 'mdi-star-off' : 'mdi-star-plus-outline'"
            @click="toggleFavorite()"
          />
        </v-list>
      </v-menu>
      <!-- 路径栏右键菜单（收藏 / 取消收藏；openPathMenu 置坐标后由 v-model 打开） -->
      <v-menu
        v-model="pathMenu.visible"
        :position-x="pathMenu.x"
        :position-y="pathMenu.y"
        :close-on-content-click="false"
        absolute
      >
        <v-list density="compact" min-width="200">
          <v-list-item
            :title="isFavorited ? '取消收藏当前路径' : '收藏当前路径'"
            :prepend-icon="isFavorited ? 'mdi-star-off' : 'mdi-star-plus-outline'"
            @click="toggleFavorite()"
          />
        </v-list>
      </v-menu>
    </div>

    <!-- 表头（可点击排序，目录恒排前） -->
    <div class="file-pane__head">
      <span class="file-pane__cell file-pane__cell--name" @click="toggleSort('name')">
        名称<span v-if="sortKey === 'name'" class="file-pane__arrow">{{ sortDesc ? '▾' : '▴' }}</span>
      </span>
      <span class="file-pane__cell" @click="toggleSort('size')">
        大小<span v-if="sortKey === 'size'" class="file-pane__arrow">{{ sortDesc ? '▾' : '▴' }}</span>
      </span>
      <span class="file-pane__cell" @click="toggleSort('modified_at')">
        修改时间<span v-if="sortKey === 'modified_at'" class="file-pane__arrow">{{ sortDesc ? '▾' : '▴' }}</span>
      </span>
      <span class="file-pane__cell" @click="toggleSort('permissions')">
        权限<span v-if="sortKey === 'permissions'" class="file-pane__arrow">{{ sortDesc ? '▾' : '▴' }}</span>
      </span>
    </div>

    <!-- 文件表：虚拟滚动保证大目录不卡 -->
    <div
      ref="bodyEl"
      class="file-pane__body"
      :class="{ 'file-pane__body--drag': dragOver }"
      @dragover="onDragOver"
      @drop="onDrop"
      @dragenter="onDragEnter"
      @dragleave="onDragLeave"
      @contextmenu.prevent="openMenu($event, null)"
    >
      <!-- 非空刷新时的半透明遮罩：阻断交互并给出进度反馈 -->
      <div v-if="loading && entries.length" class="file-pane__overlay">
        <v-progress-linear indeterminate height="2" />
      </div>
      <v-virtual-scroll
        v-if="!disabled && sorted.length"
        :items="sorted"
        :height="bodyHeight"
        :item-height="ROW_HEIGHT"
      >
        <template #default="{ item }">
          <div
            class="file-pane__row"
            :class="{ 'file-pane__row--selected': isSelected(asEntry(item).name) }"
            :draggable="!disabled && !loading"
            @dragstart="onDragStart($event, asEntry(item))"
            @click="onRowClick(asEntry(item), $event)"
            @dblclick="openEntry(asEntry(item))"
            @contextmenu.stop.prevent="openMenu($event, asEntry(item))"
          >
            <span class="file-pane__cell file-pane__cell--name" :title="asEntry(item).name">
              <v-icon
                v-if="asEntry(item).is_dir"
                icon="mdi-folder"
                size="16"
                class="file-pane__icon file-pane__icon--dir"
              />
              <v-icon v-else icon="mdi-file-outline" size="16" class="file-pane__icon" />
              <span class="file-pane__name-text">{{ asEntry(item).name }}</span>
            </span>
            <span class="file-pane__cell">{{ asEntry(item).is_dir ? '—' : formatSize(asEntry(item).size) }}</span>
            <span class="file-pane__cell">{{ formatTime(asEntry(item).modified_at) }}</span>
            <span class="file-pane__cell">{{ asEntry(item).permissions || '—' }}</span>
          </div>
        </template>
      </v-virtual-scroll>

      <EmptyState
        v-else-if="disabled"
        size="compact"
        icon="mdi-console"
        title="请先选择活动会话"
        desc="在顶部会话标签中选择一个 SSH 会话"
      />
      <div v-else-if="loading" class="file-pane__hint">加载中…</div>
      <EmptyState
        v-else
        size="compact"
        icon="mdi-folder-open-outline"
        title="目录为空"
        desc="此目录下没有文件"
      />
    </div>

    <!-- 错误提示 -->
    <v-snackbar v-model="snackbar" timeout="3000" location="bottom">{{ errorMsg }}</v-snackbar>

    <!-- 右键菜单：新建文件夹 / 重命名 / 权限 / 删除 / 传输到对侧 / 终端定位 -->
    <v-menu v-model="menu.visible" :target="[menu.x, menu.y]" min-width="180">
      <v-list density="compact">
        <v-list-item @click="actionMkdir">
          <v-list-item-title>新建文件夹</v-list-item-title>
        </v-list-item>
        <!-- 编辑（WinSCP 语义）：仅远程窗格的文件；双击文本文件同样进入 -->
        <v-list-item
          v-if="side === 'remote' && menu.entry && !menu.entry.is_dir"
          @click="actionEdit"
        >
          <v-list-item-title>编辑</v-list-item-title>
        </v-list-item>
        <v-list-item :disabled="!menu.entry" @click="actionRename">
          <v-list-item-title>重命名</v-list-item-title>
        </v-list-item>
        <v-list-item v-if="side === 'remote'" :disabled="!menu.entry" @click="actionChmod">
          <v-list-item-title>权限</v-list-item-title>
        </v-list-item>
        <v-list-item :disabled="!menu.entry" @click="actionDelete">
          <v-list-item-title>删除</v-list-item-title>
        </v-list-item>
        <v-divider />
        <v-list-item :disabled="!menu.entry || !canTransfer" @click="actionTransfer">
          <v-list-item-title>传输到对侧</v-list-item-title>
        </v-list-item>
        <v-list-item v-if="side === 'remote'" :disabled="!sessionId" @click="actionLocateTerminal">
          <v-list-item-title>将终端定位到当前目录</v-list-item-title>
        </v-list-item>
      </v-list>
    </v-menu>

    <!-- 新建 / 重命名 / 删除确认弹层（危险操作二次确认） -->
    <v-dialog v-model="dialogVisible" max-width="420">
      <v-card>
        <v-card-title class="d-flex align-center text-subtitle-1">{{ dialogTitle }}
          <v-spacer />
          <v-btn
          icon="mdi-close"
          size="x-small"
          variant="text"
          title="关闭"
          @click="dialogVisible = false"
          />
        </v-card-title>
        <v-divider />
        <v-card-text>
          <template v-if="dialogType === 'delete'">
            确认删除{{ targetEntry?.is_dir ? '文件夹' : '文件' }}「{{ targetEntry?.name }}」？此操作不可恢复。
          </template>
          <template v-else-if="dialogType === 'mkdir'">
            <div class="fy-field-row">
              <span class="fy-field-row__label">文件夹名称</span>
              <v-text-field
                v-model="dialogValue"
                density="compact"
                variant="outlined"
                autofocus
                @keyup.enter="confirmDialog"
              />
            </div>
          </template>
          <template v-else-if="dialogType === 'chmod'">
            <!-- 九宫格 rwx 勾选（行 = 属主/属组/其他，列 = 读/写/执行） -->
            <div class="chmod-grid">
              <div class="chmod-grid__row chmod-grid__row--head">
                <span />
                <span>读</span><span>写</span><span>执行</span>
              </div>
              <div class="chmod-grid__row">
                <span class="chmod-grid__label">属主</span>
                <v-checkbox
                  v-for="i in 3"
                  :key="`owner-${i}`"
                  v-model="chmodOwner[i - 1]"
                  density="compact"
                  hide-details
                  @update:model-value="onChmodBitChange"
                />
              </div>
              <div class="chmod-grid__row">
                <span class="chmod-grid__label">属组</span>
                <v-checkbox
                  v-for="i in 3"
                  :key="`group-${i}`"
                  v-model="chmodGroup[i - 1]"
                  density="compact"
                  hide-details
                  @update:model-value="onChmodBitChange"
                />
              </div>
              <div class="chmod-grid__row">
                <span class="chmod-grid__label">其他</span>
                <v-checkbox
                  v-for="i in 3"
                  :key="`other-${i}`"
                  v-model="chmodOther[i - 1]"
                  density="compact"
                  hide-details
                  @update:model-value="onChmodBitChange"
                />
              </div>
            </div>
            <div class="fy-field-row">
              <span class="fy-field-row__label">八进制权限（如 644）</span>
              <v-text-field
                v-model="chmodValue"
                density="compact"
                variant="outlined"
                @update:model-value="onChmodOctalInput"
              />
            </div>
          </template>
          <template v-else-if="dialogType === 'rename'">
            <div class="fy-field-row">
              <span class="fy-field-row__label">新名称</span>
              <v-text-field
                v-model="dialogValue"
                density="compact"
                variant="outlined"
                autofocus
                @keyup.enter="confirmDialog"
              />
            </div>
          </template>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" :disabled="busy" @click="dialogVisible = false">取消</v-btn>
          <v-btn
            :color="dialogType === 'delete' ? 'error' : 'primary'"
            :loading="busy"
            @click="confirmDialog"
          >
            {{ dialogType === 'delete' ? '确认删除' : '确定' }}
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- 远程文件文本编辑器（双击/右键「编辑」打开，保存后刷新列表） -->
    <RemoteTextEditorDialog
      v-model="editorOpen"
      :session-id="props.sessionId"
      :path="editorPath"
      @saved="refresh"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import {
  sftpList,
  localList,
  sftpMkdir,
  sftpRename,
  sftpDelete,
  sftpChmod,
  sftpFavoriteList,
  sftpFavoriteAdd,
  sftpFavoriteRemove,
} from '@/api/sftp'
import { useTerminalStore } from '@/stores/terminal'
import {
  DEFAULT_PATH,
  formatSize,
  formatTime,
  isRootPath,
  joinPath,
  parentPath,
  type FileEntry,
  type PaneSide,
} from './file-utils'
import type { SftpFavorite } from '@/api/types'
import { friendlyError } from '@/utils/errors'
import EmptyState from '@/components/common/EmptyState.vue'
import RemoteTextEditorDialog from './RemoteTextEditorDialog.vue'

const props = withDefaults(
  defineProps<{
    /** 本地 / 远程 */
    side: PaneSide
    /** 远程侧必填：活动会话 ID */
    sessionId?: string
    /** 当前路径（v-model:path） */
    path?: string
    /** 禁用整窗格（如远程未选会话） */
    disabled?: boolean
  }>(),
  {
    sessionId: '',
    path: '',
    disabled: false,
  },
)

const emit = defineEmits<{
  /** 路径变更（双击目录、上级、地址栏回车） */
  (e: 'update:path', value: string): void
  /** 右键"传输到对侧"：请求把条目传输到另一侧 */
  (e: 'transfer-request', entries: FileEntry[]): void
  /** 拖拽释放到本窗格：payload.from 为来源侧，由父级判断是否为跨侧传输 */
  (e: 'drop-files', payload: { from: PaneSide; entries: FileEntry[] }): void
}>()

/** 终端连接状态查询（只读，判断"定位到当前目录"的目标终端是否已连接） */
const terminalStore = useTerminalStore()

const ROW_HEIGHT = 28

/** VVirtualScroll 槽的 item 类型为 unknown，模板中统一断言为 FileEntry */
const asEntry = (it: unknown): FileEntry => it as FileEntry

// ---------- 列表数据 ----------

const entries = ref<FileEntry[]>([])
const loading = ref(false)
const errorMsg = ref('')
const snackbar = ref(false)

/** 统一提示：复用错误提示条（信息类文案同样展示） */
function notify(message: string): void {
  errorMsg.value = message
  snackbar.value = true
}

const currentPath = computed(() => props.path || DEFAULT_PATH[props.side])
const isRoot = computed(() => isRootPath(props.side, currentPath.value))

/** 当前选中条目名集合（Ctrl/Shift 多选；空集表示未选中）。Shift 区间以 anchorName 为锚点 */
const selectedNames = ref<Set<string>>(new Set())
let anchorName: string | null = null

/** 目录加载序号：快速切换路径/连接时丢弃过期响应（防止慢的旧目录覆盖新目录/错弹错误） */
let loadSeq = 0
async function load(): Promise<void> {
  if (props.disabled) return
  if (props.side === 'remote' && !props.sessionId) {
    entries.value = []
    return
  }
  const seq = ++loadSeq
  const path = currentPath.value
  loading.value = true
  errorMsg.value = ''
  try {
    if (props.side === 'remote') {
      const list = ((await sftpList(props.sessionId, path)) as unknown[]) as FileEntry[]
      if (seq !== loadSeq) return // 已被更新的跳转覆盖：丢弃过期目录内容
      entries.value = list
    } else {
      const list = ((await localList(path)) as unknown[]) as FileEntry[]
      if (seq !== loadSeq) return
      entries.value = list
    }
  } catch (e) {
    if (seq !== loadSeq) return // 过期响应失败不弹错误（报错已属于离开的目录）
    entries.value = []
    errorMsg.value = friendlyError(e)
    snackbar.value = true
  } finally {
    loading.value = false
  }
}

function refresh(): void {
  void load()
}

function goUp(): void {
  const parent = parentPath(props.side, currentPath.value)
  if (parent) emit('update:path', parent)
}

/** 地址栏回车跳转 */
function commitPath(): void {
  const target = pathInput.value.trim()
  if (!target || target === currentPath.value) return
  emit('update:path', target)
}

const pathInput = ref(currentPath.value)

watch(
  () => [currentPath.value, props.sessionId, props.side, props.disabled],
  () => {
    pathInput.value = currentPath.value
    clearSelection()
    void load()
  },
  { immediate: true },
)

// ---------- 排序 ----------

type SortKey = 'name' | 'size' | 'modified_at' | 'permissions'
const sortKey = ref<SortKey>('name')
const sortDesc = ref(false)

function toggleSort(key: SortKey): void {
  if (sortKey.value === key) {
    sortDesc.value = !sortDesc.value
  } else {
    sortKey.value = key
    sortDesc.value = false
  }
}

const sorted = computed(() => {
  const list = [...entries.value]
  const dir = sortDesc.value ? -1 : 1
  const key = sortKey.value
  list.sort((a, b) => {
    // 目录恒排在文件之前
    if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1
    if (a[key] === b[key]) return 0
    return a[key] > b[key] ? dir : -dir
  })
  return list
})

// ---------- 选择与多选（Ctrl/Shift 批量，WinSCP 惯例） ----------

function isSelected(name: string): boolean {
  return selectedNames.value.has(name)
}

/** 整体替换选中集（并更新区间锚点） */
function setSelection(names: string[]): void {
  selectedNames.value = new Set(names)
  anchorName = names.length === 1 ? names[0] : null
}

function clearSelection(): void {
  selectedNames.value = new Set()
  anchorName = null
}

/** 切换单项选中（Ctrl/无修饰键共用路径，切换后以该项为新锚点） */
function toggleSelect(entry: FileEntry): void {
  const set = new Set(selectedNames.value)
  if (set.has(entry.name)) {
    set.delete(entry.name)
    anchorName = entry.name // 二次点击取消时仍以该项为后续 Shift 锚点（主流管理器惯例）
  } else {
    set.add(entry.name)
    anchorName = entry.name
  }
  selectedNames.value = set
}

/**
 * Shift 区间选择：从锚点（最近一次单选/点击项）到当前项，按当前排序顺序闭区间全选。
 * 无锚点时降级为单选当前项（首次 Shift 无意义）。
 */
function rangeSelect(entry: FileEntry): void {
  const list = sorted.value
  if (!anchorName) {
    setSelection([entry.name])
    return
  }
  const from = list.findIndex((e) => e.name === anchorName)
  const to = list.findIndex((e) => e.name === entry.name)
  const names = (from >= 0 && to >= 0 ? list.slice(Math.min(from, to), Math.max(from, to) + 1) : [entry])
    .map((e) => e.name)
  setSelection(names)
}

/**
 * 行点击：Ctrl/Meta 切换、Shift 区间、普通点击单选（WinSCP 同键行为）。
 * 返回当前选中集合并更新锚点。
 */
function onRowClick(entry: FileEntry, event: MouseEvent): void {
  if (event.shiftKey) {
    rangeSelect(entry)
  } else if (event.ctrlKey || event.metaKey) {
    toggleSelect(entry)
  } else {
    setSelection([entry.name])
  }
}

/** 全体选中条目（按当前排序顺序；供批量传输/拖拽取数） */
const selectedEntries = computed(() => sorted.value.filter((e) => isSelected(e.name)))

function openEntry(entry: FileEntry): void {
  if (entry.is_dir) {
    emit('update:path', joinPath(props.side, currentPath.value, entry.name))
    return
  }
  // 远程文件双击进入文本编辑器（WinSCP 语义）；本地文件暂无打开能力，给出友好反馈
  if (props.side === 'remote') {
    openEditor(entry.name)
    return
  }
  errorMsg.value = `暂不支持打开文件「${entry.name}」`
  snackbar.value = true
}

// ---------- 远程文件文本编辑（WinSCP 语义：右键「编辑」+ 双击文本文件） ----------

/** 编辑器打开状态与目标远程文件绝对路径 */
const editorOpen = ref(false)
const editorPath = ref('')

/** 按文件名打开编辑器（路径取当前目录 + 文件名，仅远程侧） */
function openEditor(name: string): void {
  if (props.side !== 'remote' || !props.sessionId) return
  editorPath.value = joinPath('remote', currentPath.value, name)
  editorOpen.value = true
}

/** 右键「编辑」：关闭菜单后打开目标文件（menu.entry 已确保为远程文件） */
function actionEdit(): void {
  if (!menu.entry) return
  menu.visible = false
  openEditor(menu.entry.name)
}

// ---------- 右键菜单 ----------

const menu = reactive({
  visible: false,
  x: 0,
  y: 0,
  entry: null as FileEntry | null,
})

function openMenu(event: MouseEvent, entry: FileEntry | null): void {
  if (props.disabled) return
  // 右键语义（WinSCP 惯例）：右击已选中条目保留整组多选；右击未选中条目则替换为单选该项
  if (entry && !isSelected(entry.name)) setSelection([entry.name])
  menu.entry = entry
  menu.x = event.clientX
  menu.y = event.clientY
  menu.visible = true
}

/** 本地侧传输到远程需要活动会话；远程侧传输到本地始终可用 */
const canTransfer = computed(() => props.side === 'remote' || !!props.sessionId)

function actionTransfer(): void {
  menu.visible = false
  if (!canTransfer.value) return
  // 批量传输：优先携带全部选中条目；无选中时回退右键目标单条目
  const targets = selectedEntries.value.length
    ? selectedEntries.value
    : menu.entry
      ? [{ ...menu.entry }]
      : []
  if (!targets.length) return
  emit('transfer-request', targets)
}

// ---------- 将终端定位到当前目录（仅远程侧） ----------

/**
 * 右键"将终端定位到当前目录"：向该会话的 SSH 终端注入 `cd <path>` 并回车执行。
 * 写入通道与快捷命令栏一致（writeToSession 按会话类型路由）；路径含空格/单引号时按 POSIX shell 语义加引号。
 */
async function actionLocateTerminal(): Promise<void> {
  menu.visible = false
  if (props.side !== 'remote' || !props.sessionId) return
  if (!terminalStore.isConnected(props.sessionId)) {
    notify('当前会话终端未连接，无法定位')
    return
  }
  // POSIX 单引号包裹：内嵌单引号按 shell 惯例转义为 '\''
  const quoted = `'${currentPath.value.replace(/'/g, `'\\''`)}'`
  const payload = new TextEncoder().encode(`cd ${quoted}\n`)
  try {
    await terminalStore.writeToSession(props.sessionId, payload)
    notify(`已将终端定位到 ${currentPath.value}`)
  } catch (e) {
    notify(friendlyError(e))
  }
}

// ---------- 收藏路径（SQLite 持久化，按侧过滤） ----------

/** 收藏列表全量缓存（含两侧），下拉按窗格侧过滤 */
const favorites = ref<SftpFavorite[]>([])

/** 当前路径是否已收藏（同侧同名路径命中） */
const isFavorited = computed(() =>
  favorites.value.some((f) => f.side === props.side && f.path === currentPath.value),
)

/** 当前侧的收藏列表（下拉展示与跳转） */
const favoriteForSide = computed(() => favorites.value.filter((f) => f.side === props.side))

async function loadFavorites(): Promise<void> {
  try {
    favorites.value = await sftpFavoriteList()
  } catch {
    // 收藏加载失败不阻塞主流程
  }
}

/** 路径栏右键：收藏当前路径（幂等） */
async function actionFavorite(): Promise<void> {
  pathMenu.visible = false
  try {
    await sftpFavoriteAdd(props.side, currentPath.value)
    await loadFavorites()
    notify(`已收藏路径 ${currentPath.value}`)
  } catch (e) {
    notify(friendlyError(e))
  }
}

/** 路径栏右键：取消收藏当前路径（按侧 + 路径删除） */
async function actionUnfavorite(): Promise<void> {
  pathMenu.visible = false
  try {
    await sftpFavoriteRemove(props.side, currentPath.value)
    await loadFavorites()
    notify(`已取消收藏 ${currentPath.value}`)
  } catch (e) {
    notify(friendlyError(e))
  }
}

/** 收藏/取消收藏切换（下拉动作项与右键菜单共用，幂等） */
function toggleFavorite(): void {
  if (isFavorited.value) void actionUnfavorite()
  else void actionFavorite()
}

/** 收藏下拉点击条目：跳转到该路径 */
function jumpFavorite(fav: SftpFavorite): void {
  if (fav.side !== props.side) return
  emit('update:path', fav.path)
}

/** 收藏下拉条目的移除按钮 */
async function removeFavorite(fav: SftpFavorite): Promise<void> {
  try {
    await sftpFavoriteRemove(props.side, fav.path)
    await loadFavorites()
  } catch (e) {
    notify(friendlyError(e))
  }
}

/** 路径栏右键菜单（收藏 / 取消收藏） */
const pathMenu = reactive({ visible: false, x: 0, y: 0 })

function openPathMenu(event: MouseEvent): void {
  if (props.disabled) return
  pathMenu.x = event.clientX
  pathMenu.y = event.clientY
  pathMenu.visible = true
}

// ---------- 新建 / 重命名 / 权限 / 删除 ----------

const dialogType = ref<null | 'mkdir' | 'rename' | 'delete' | 'chmod'>(null)
const dialogValue = ref('')
const busy = ref(false)
const targetEntry = computed(() => menu.entry)

const dialogTitle = computed(() => {
  if (dialogType.value === 'mkdir') return '新建文件夹'
  if (dialogType.value === 'rename') return '重命名'
  if (dialogType.value === 'delete') return '删除确认'
  if (dialogType.value === 'chmod') return '权限设置'
  return ''
})

const dialogVisible = computed({
  get: () => dialogType.value !== null,
  set: (v: boolean) => {
    if (!v) dialogType.value = null
  },
})

function actionMkdir(): void {
  menu.visible = false
  dialogValue.value = ''
  dialogType.value = 'mkdir'
}

function actionRename(): void {
  menu.visible = false
  dialogValue.value = menu.entry?.name ?? ''
  dialogType.value = 'rename'
}

function actionDelete(): void {
  menu.visible = false
  dialogType.value = 'delete'
}

// ---------- 权限（chmod，仅远程侧） ----------

/** 九宫格勾选状态：owner/group/other 各 [读, 写, 执行]，默认 644 */
const chmodOwner = ref([true, true, false])
const chmodGroup = ref([true, false, false])
const chmodOther = ref([true, false, false])
/** 八进制权限输入框（与九宫格双向联动） */
const chmodValue = ref('644')

/** 打开权限对话框：默认回填当前权限位（无权限信息时回退 644） */
function actionChmod(): void {
  menu.visible = false
  applyChmodOctal(menu.entry?.permissions || '644')
  dialogType.value = 'chmod'
}

/** 九宫格勾选位 → 八进制串（如 "644"） */
function octalFromBits(): string {
  const bits = [...chmodOwner.value, ...chmodGroup.value, ...chmodOther.value]
  let val = 0
  bits.forEach((on, i) => {
    if (on) val |= 1 << (8 - i)
  })
  return val.toString(8).padStart(3, '0')
}

/** 九宫格勾选 → 同步八进制输入框 */
function onChmodBitChange(): void {
  chmodValue.value = octalFromBits()
}

/** 八进制输入 → 反向同步九宫格（非法输入不动作，保留原勾选） */
function onChmodOctalInput(value: string): void {
  const m = /^[0-7]{1,3}$/.exec(value.trim())
  if (!m) return
  const val = parseInt(value.trim(), 8)
  const bits = [...chmodOwner.value, ...chmodGroup.value, ...chmodOther.value]
  bits.forEach((_, i) => {
    bits[i] = (val & (1 << (8 - i))) !== 0
  })
  chmodOwner.value = bits.slice(0, 3)
  chmodGroup.value = bits.slice(3, 6)
  chmodOther.value = bits.slice(6, 9)
  chmodValue.value = val.toString(8).padStart(3, '0')
}

function applyChmodOctal(octal: string): void {
  onChmodOctalInput(octal)
}

/**
 * 本地变更接口不在 IPC 契约中（契约仅有 local_list），
 * 这里对 @/api/sftp 的可选导出做动态调用；若封装层暂未提供则给出友好提示。
 */
async function callLocalApi(fn: string, ...args: unknown[]): Promise<void> {
  const mod = (await import('@/api/sftp')) as Record<
    string,
    ((...a: unknown[]) => unknown) | undefined
  >
  const impl = mod[fn]
  if (typeof impl !== 'function') {
    throw new Error(`本地文件管理接口未就绪（api 层暂未提供 ${fn}）`)
  }
  await impl(...args)
}

async function confirmDialog(): Promise<void> {
  const type = dialogType.value
  if (!type || busy.value) return
  if (type !== 'delete' && !dialogValue.value.trim()) return
  busy.value = true
  try {
    if (type === 'mkdir') {
      if (props.side === 'remote' && !props.sessionId) return
      const fullPath = joinPath(props.side, currentPath.value, dialogValue.value.trim())
      if (props.side === 'remote') {
        await sftpMkdir(props.sessionId, fullPath)
      } else {
        await callLocalApi('localMkdir', fullPath)
      }
    } else if (type === 'rename') {
      const entry = targetEntry.value
      const name = dialogValue.value.trim()
      if (!entry) return
      if (name !== entry.name) {
        const oldPath = joinPath(props.side, currentPath.value, entry.name)
        const newPath = joinPath(props.side, currentPath.value, name)
        if (props.side === 'remote') {
          await sftpRename(props.sessionId, oldPath, newPath)
        } else {
          await callLocalApi('localRename', oldPath, newPath)
        }
      }
    } else if (type === 'delete') {
      const entry = targetEntry.value
      if (!entry) return
      const fullPath = joinPath(props.side, currentPath.value, entry.name)
      if (props.side === 'remote') {
        await sftpDelete(props.sessionId, fullPath, entry.is_dir)
      } else {
        await callLocalApi('localDelete', fullPath, entry.is_dir)
      }
    } else if (type === 'chmod') {
      const entry = targetEntry.value
      const m = /^[0-7]{1,3}$/.exec(chmodValue.value.trim())
      if (!entry || props.side !== 'remote' || !props.sessionId || !m) return
      const fullPath = joinPath(props.side, currentPath.value, entry.name)
      await sftpChmod(props.sessionId, fullPath, parseInt(chmodValue.value.trim(), 8))
    }
    dialogType.value = null
    menu.entry = null
    await load()
  } catch (e) {
    errorMsg.value = friendlyError(e)
    snackbar.value = true
  } finally {
    busy.value = false
  }
}

// ---------- 拖拽传输（实际入队由父级 DualPane 处理） ----------

/** 拖拽悬停状态（用于高亮目标窗格）；用计数避免子元素边界导致闪烁 */
const dragOver = ref(false)
let dragDepth = 0

function onDragEnter(event: DragEvent): void {
  event.preventDefault()
  dragDepth++
  dragOver.value = true
}

function onDragLeave(event: DragEvent): void {
  event.preventDefault()
  dragDepth--
  if (dragDepth <= 0) {
    dragDepth = 0
    dragOver.value = false
  }
}

function onDragOver(event: DragEvent): void {
  if (props.disabled) return
  event.preventDefault()
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy'
  dragOver.value = true
}

function onDrop(event: DragEvent): void {
  if (props.disabled) return
  event.preventDefault()
  dragDepth = 0
  dragOver.value = false
  const raw = event.dataTransfer?.getData('application/x-fyshell-entries')
  if (!raw) return
  try {
    const parsed = JSON.parse(raw) as { from?: PaneSide; entries?: FileEntry[] }
    if (parsed.from && Array.isArray(parsed.entries)) {
      if (parsed.from === props.side) {
        // 同侧拖放无需传输，给出提示
        errorMsg.value = '同一面板内无需传输'
        snackbar.value = true
        return
      }
      emit('drop-files', { from: parsed.from, entries: parsed.entries })
    }
  } catch {
    // 非法拖拽数据，忽略
  }
}

// 拖拽源标记：拖拽启动时写入 MIME 数据，由对侧窗格的 drop 事件接收
const dragMime = 'application/x-fyshell-entries'
function onDragStart(event: DragEvent, entry: FileEntry): void {
  if (props.disabled || !event.dataTransfer) return
  // 批量拖拽：当前项已在多选中时携带全部选中项；否则仅拖该项（并单选它以锚定视觉）
  const dragging = isSelected(entry.name) ? selectedEntries.value : [entry]
  if (dragging.length) setSelection(dragging.map((e) => e.name))
  event.dataTransfer.setData(
    dragMime,
    JSON.stringify({ from: props.side, entries: dragging.map((e) => ({ ...e })) }),
  )
  event.dataTransfer.effectAllowed = 'copyMove'
}

// ---------- 虚拟滚动高度（随容器自适应） ----------

const bodyEl = ref<HTMLElement | null>(null)
const bodyHeight = ref(320)
let resizeObserver: ResizeObserver | null = null

onMounted(() => {
  if (bodyEl.value && typeof ResizeObserver !== 'undefined') {
    resizeObserver = new ResizeObserver((observed) => {
      for (const entry of observed) {
        bodyHeight.value = Math.max(120, Math.floor(entry.contentRect.height))
      }
    })
    resizeObserver.observe(bodyEl.value)
  }
  // 收藏路径列表（SQLite；失败静默，不阻塞主流程）
  void loadFavorites()
})

onUnmounted(() => {
  resizeObserver?.disconnect()
  resizeObserver = null
})

/** 供父级（DualPane）在传输完成后触发对侧列表刷新 */
defineExpose({ refresh })
</script>

<style scoped>
.file-pane {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-width: 0;
  background: rgb(var(--v-theme-surface));
}

.file-pane--disabled {
  opacity: 0.6;
}

.file-pane__toolbar {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px;
}

.file-pane__path {
  flex: 1;
  min-width: 0;
}

.file-pane__head,
.file-pane__row {
  display: grid;
  /* 名称列设下限保证窄面板可读；固定列相对较宽时可横向滚动查看 */
  grid-template-columns: minmax(120px, 1fr) 88px 132px 88px;
  gap: 8px;
  align-items: center;
  padding: 0 8px;
}

.file-pane__head {
  height: 30px;
  font-size: 12px;
  font-weight: 400;
  color: rgba(var(--v-theme-on-surface), 0.7);
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  user-select: none;
}

.file-pane__head .file-pane__cell {
  cursor: pointer;
}

.file-pane__arrow {
  margin-left: 2px;
  font-size: 12px;
}

.file-pane__body {
  position: relative;
  flex: 1;
  min-height: 120px;
  overflow: hidden;
  overflow-x: auto;
}

.file-pane__body--drag {
  box-shadow: inset 0 0 0 2px rgba(var(--v-theme-primary), 0.6);
  background: rgba(var(--v-theme-primary), 0.08);
}

/* 非空刷新遮罩：阻断底层行交互并给出进度反馈 */
.file-pane__overlay {
  position: absolute;
  inset: 0;
  z-index: 5;
  background: rgba(var(--v-theme-background), 0.35);
}

.file-pane__row {
  height: 28px;
  font-size: 12px;
  cursor: default;
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.12);
}

.file-pane__row:hover {
  background: rgba(var(--v-theme-primary), 0.08);
}

.file-pane__row--selected {
  background: rgba(var(--v-theme-primary), 0.15);
}

.file-pane__cell {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.file-pane__cell--name {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.file-pane__name-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.file-pane__icon {
  flex: none;
  opacity: 0.85;
}

.file-pane__icon--dir {
  color: rgb(var(--v-theme-secondary));
}

/* 权限对话框：九宫格 rwx 勾选（行 = 属主/属组/其他，列 = 读/写/执行） */
.chmod-grid {
  margin-bottom: 12px;
}

.chmod-grid__row {
  display: grid;
  grid-template-columns: 48px 1fr 1fr 1fr;
  align-items: center;
}

.chmod-grid__row--head {
  font-size: 12px;
  font-weight: 400;
  color: rgba(var(--v-theme-on-surface), 0.6);
  user-select: none;
}

.chmod-grid__label {
  font-size: 12px;
  color: rgba(var(--v-theme-on-surface), 0.85);
}

.file-pane__hint {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  min-height: 120px;
  font-size: 12px;
  color: rgba(var(--v-theme-on-surface), 0.5);
}
</style>
