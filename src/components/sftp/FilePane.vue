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
        <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor">
          <path d="M7.41 15.41 12 10.83l4.59 4.58L18 14l-6-6-6 6z" />
        </svg>
      </v-btn>
      <v-btn
        icon
        variant="text"
        size="x-small"
        :disabled="loading || disabled"
        title="刷新"
        @click="refresh"
      >
        <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor">
          <path
            d="M17.65 6.35C16.2 4.9 14.21 4 12 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.6-6.66h-2.16c-.7 2.37-2.82 4.06-5.44 4.06-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.16.68 4.24 1.76L13 11h7V4l-2.35 2.35z"
          />
        </svg>
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
            :class="{ 'file-pane__row--selected': selectedName === asEntry(item).name }"
            :draggable="!disabled && !loading"
            @dragstart="onDragStart($event, asEntry(item))"
            @click="select(asEntry(item))"
            @dblclick="openEntry(asEntry(item))"
            @contextmenu.stop.prevent="openMenu($event, asEntry(item))"
          >
            <span class="file-pane__cell file-pane__cell--name" :title="asEntry(item).name">
              <svg
                v-if="asEntry(item).is_dir"
                class="file-pane__icon file-pane__icon--dir"
                viewBox="0 0 24 24"
                width="16"
                height="16"
                fill="currentColor"
              >
                <path d="M10 4H4c-1.1 0-1.99.9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2h-8l-2-2z" />
              </svg>
              <svg
                v-else
                class="file-pane__icon"
                viewBox="0 0 24 24"
                width="16"
                height="16"
                fill="currentColor"
              >
                <path d="M14 2H6c-1.1 0-1.99.9-1.99 2L4 20c0 1.1.89 2 1.99 2H18c1.1 0 2-.9 2-2V8l-6-6zm2 16H8v-2h8v2zm0-4H8v-2h8v2zm-3-5V3.5L18.5 9H13z" />
              </svg>
              <span class="file-pane__name-text">{{ asEntry(item).name }}</span>
            </span>
            <span class="file-pane__cell">{{ asEntry(item).is_dir ? '—' : formatSize(asEntry(item).size) }}</span>
            <span class="file-pane__cell">{{ formatTime(asEntry(item).modified_at) }}</span>
            <span class="file-pane__cell">{{ asEntry(item).permissions || '—' }}</span>
          </div>
        </template>
      </v-virtual-scroll>

      <div v-else-if="disabled" class="file-pane__hint">请先选择活动会话</div>
      <div v-else-if="loading" class="file-pane__hint">加载中…</div>
      <div v-else class="file-pane__hint">目录为空</div>
    </div>

    <!-- 错误提示 -->
    <v-snackbar v-model="snackbar" timeout="3000" location="bottom">{{ errorMsg }}</v-snackbar>

    <!-- 右键菜单：新建文件夹 / 重命名 / 权限 / 删除 / 传输到对侧 / 终端定位 -->
    <v-menu v-model="menu.visible" :target="[menu.x, menu.y]" min-width="180">
      <v-list density="compact">
        <v-list-item @click="actionMkdir">
          <v-list-item-title>新建文件夹</v-list-item-title>
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
        <v-card-title class="text-subtitle-1">{{ dialogTitle }}</v-card-title>
        <v-card-text>
          <template v-if="dialogType === 'delete'">
            确认删除{{ targetEntry?.is_dir ? '文件夹' : '文件' }}「{{ targetEntry?.name }}」？此操作不可恢复。
          </template>
          <template v-else-if="dialogType === 'mkdir'">
            <v-text-field
              v-model="dialogValue"
              label="文件夹名称"
              density="compact"
              variant="outlined"
              autofocus
              @keyup.enter="confirmDialog"
            />
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
            <v-text-field
              v-model="chmodValue"
              label="八进制权限（如 644）"
              density="compact"
              variant="outlined"
              @update:model-value="onChmodOctalInput"
            />
          </template>
          <template v-else-if="dialogType === 'rename'">
            <v-text-field
              v-model="dialogValue"
              label="新名称"
              density="compact"
              variant="outlined"
              autofocus
              @keyup.enter="confirmDialog"
            />
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

/** 当前选中条目名（单选；空串表示未选中） */
const selectedName = ref('')

async function load(): Promise<void> {
  if (props.disabled) return
  if (props.side === 'remote' && !props.sessionId) {
    entries.value = []
    return
  }
  loading.value = true
  errorMsg.value = ''
  try {
    if (props.side === 'remote') {
      entries.value = ((await sftpList(props.sessionId, currentPath.value)) as unknown[]) as FileEntry[]
    } else {
      entries.value = ((await localList(currentPath.value)) as unknown[]) as FileEntry[]
    }
  } catch (e) {
    entries.value = []
    errorMsg.value = e instanceof Error ? e.message : String(e)
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
    selectedName.value = ''
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

// ---------- 选择与打开 ----------

function select(entry: FileEntry): void {
  selectedName.value = entry.name
}

function openEntry(entry: FileEntry): void {
  if (entry.is_dir) {
    emit('update:path', joinPath(props.side, currentPath.value, entry.name))
    return
  }
  // 文件双击暂无打开能力，给出友好反馈，避免零反馈
  errorMsg.value = `暂不支持打开文件「${entry.name}」`
  snackbar.value = true
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
  menu.entry = entry
  if (entry) selectedName.value = entry.name
  menu.x = event.clientX
  menu.y = event.clientY
  menu.visible = true
}

/** 本地侧传输到远程需要活动会话；远程侧传输到本地始终可用 */
const canTransfer = computed(() => props.side === 'remote' || !!props.sessionId)

function actionTransfer(): void {
  menu.visible = false
  if (!menu.entry || !canTransfer.value) return
  emit('transfer-request', [{ ...menu.entry }])
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
    notify(e instanceof Error ? e.message : String(e))
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
    notify(e instanceof Error ? e.message : String(e))
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
    notify(e instanceof Error ? e.message : String(e))
  }
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
    notify(e instanceof Error ? e.message : String(e))
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
    errorMsg.value = e instanceof Error ? e.message : String(e)
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
  selectedName.value = entry.name // 统一选中态，作为拖拽视觉锚点
  event.dataTransfer.setData(
    dragMime,
    JSON.stringify({ from: props.side, entries: [{ ...entry }] }),
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
  font-weight: 600;
  color: rgba(var(--v-theme-on-surface), 0.7);
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  user-select: none;
}

.file-pane__head .file-pane__cell {
  cursor: pointer;
}

.file-pane__arrow {
  margin-left: 2px;
  font-size: 11px;
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
  font-size: 13px;
  cursor: default;
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.06);
}

.file-pane__row:hover {
  background: rgba(var(--v-theme-primary), 0.08);
}

.file-pane__row--selected {
  background: rgba(var(--v-theme-primary), 0.18);
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
  font-weight: 600;
  color: rgba(var(--v-theme-on-surface), 0.6);
  user-select: none;
}

.chmod-grid__label {
  font-size: 13px;
  color: rgba(var(--v-theme-on-surface), 0.85);
}

.file-pane__hint {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  min-height: 120px;
  font-size: 13px;
  color: rgba(var(--v-theme-on-surface), 0.5);
}
</style>
