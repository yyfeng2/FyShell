<script setup lang="ts">
/**
 * FlexTabs —— 通用标签栏组件
 *
 * 特性：
 * - 新增（加号）/关闭按钮、中键关闭、点击切换
 * - HTML5 拖拽排序（拖动后 emit reorder，携带新 id 顺序）
 * - Tab 着色支持：tab.color（hex）作为激活态指示色（参考 Xshell 按连接着色）
 * - 右键菜单：复制名称 / 复制会话 / 固定 / 重命名 / 新窗口打开 / 关闭组（固定标签受关闭保护）
 *
 * 布局约定：紧凑行高，深色主题友好，可被任意视图复用。
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useUiStore } from '@/stores/ui'

/** Tab 数据结构（最小约定，父组件可传扩展字段） */
export interface FlexTabItem {
  /** 唯一标识（注意：不等于会话 ID） */
  id: string
  title: string
  icon?: string
  /** Tab 着色（hex），无着色传 undefined/null */
  color?: string | null
  /** 单个 Tab 是否可关闭（缺省回落到全局 showClose） */
  closable?: boolean
  /** 是否为会话终端 Tab（可「复制会话」；缺省不限制，由父级 handler 兜底判定） */
  duplicatable?: boolean
}

const props = withDefaults(
  defineProps<{
    /** Tab 列表 */
    tabs: FlexTabItem[]
    /** 当前激活 Tab 的 id（v-model） */
    modelValue: string | null
    /** 是否显示加号新增按钮 */
    showNew?: boolean
    /** 全局是否显示关闭按钮 */
    showClose?: boolean
  }>(),
  { modelValue: null, showNew: true, showClose: true },
)

const emit = defineEmits<{
  /** 切换激活 Tab */
  (e: 'update:modelValue', id: string): void
  /** 请求关闭某 Tab（中键或关闭按钮） */
  (e: 'close', id: string): void
  /** 点击加号新增 Tab */
  (e: 'create'): void
  /** 拖拽排序完成，携带按新顺序排列的 id 数组 */
  (e: 'reorder', ids: string[]): void
  /** Tab 拖出标签栏（P1：拖出新窗口），携带被拖出的 tab id */
  (e: 'drag-out', id: string): void
  /** 右键菜单重命名，携带目标 tab id 与新标题 */
  (e: 'rename', id: string, title: string): void
  /** 右键菜单打开 SSH 会话设置，携带目标 tab id */
  (e: 'session-settings', id: string): void
  /** 右键菜单复制会话：同一会话再开一个 Tab（独立连接），携带目标 tab id */
  (e: 'duplicate-session', id: string): void
}>()

/** 全局 UI store：右键菜单操作反馈走全局 toast */
const ui = useUiStore()

// ---- 点击切换 ----
function activate(id: string): void {
  if (id !== props.modelValue) emit('update:modelValue', id)
}

// ---- 中键关闭 ----
function onMouseDown(e: MouseEvent, id: string): void {
  if (e.button === 1) {
    // 阻止浏览器中键自动滚动
    e.preventDefault()
    // 固定标签受保护：中键不关闭
    if (!isFixed(id)) emit('close', id)
  }
}

// ---- 固定标签（右键菜单）：固定后带图钉标识，所有关闭路径受保护 ----
const fixedIds = ref<Set<string>>(new Set())

/** 该 tab 是否被固定（id 允许为 null，便于模板调用） */
function isFixed(id: string | null): boolean {
  return !!id && fixedIds.value.has(id)
}

/** 固定 / 取消固定（右键菜单入口） */
function toggleFixed(): void {
  const id = menu.value.tabId
  if (!id) return
  const next = new Set(fixedIds.value)
  if (next.has(id)) {
    next.delete(id)
  } else {
    next.add(id)
  }
  fixedIds.value = next
}

// 标签被移除后清理固定标识，避免残留
watch(
  () => props.tabs.map((t) => t.id),
  (ids) => {
    const alive = new Set(ids)
    for (const id of [...fixedIds.value]) {
      if (!alive.has(id)) fixedIds.value.delete(id)
    }
  },
)

// ---- 右键菜单 ----
const menu = ref({
  visible: false,
  x: 0,
  y: 0,
  /** 右键目标 Tab id */
  tabId: null as string | null,
})

/** 右键菜单目标 Tab（用于禁用态与预填） */
const menuTab = computed(() => props.tabs.find((t) => t.id === menu.value.tabId) ?? null)

/** 打开右键菜单（定位到鼠标位置） */
function openContextMenu(e: MouseEvent, id: string): void {
  menu.value.tabId = id
  menu.value.x = e.clientX
  menu.value.y = e.clientY
  menu.value.visible = true
}

/** 复制当前标签名到剪贴板 */
async function copyTabName(): Promise<void> {
  const title = menuTab.value?.title
  if (!title) return
  try {
    await navigator.clipboard.writeText(title)
    ui.toast('已复制标签名称', 'success')
  } catch {
    ui.toast('复制失败：剪贴板不可用', 'error')
  }
}

// ---- 重命名（右键菜单） ----
const renameVisible = ref(false)
const renameValue = ref('')

/** 打开重命名对话框（预填当前标题） */
function openRename(): void {
  const tab = menuTab.value
  if (!tab) return
  renameValue.value = tab.title
  renameVisible.value = true
}

/** 确认重命名：标题真源在父组件，emit 通知更新 */
function confirmRename(): void {
  const id = menu.value.tabId
  const title = renameValue.value.trim()
  if (id && title) emit('rename', id, title)
  renameVisible.value = false
}

// ---- 关闭操作（右键菜单）：复用 close 事件，由父组件走既有断开/清理流程 ----

/** 批量关闭一批 tab */
function closeTabs(ids: string[]): void {
  for (const id of ids) emit('close', id)
}

/** 关闭其他：除右键目标与固定标签外全部关闭 */
function closeOthers(): void {
  const id = menu.value.tabId
  if (!id) return
  closeTabs(props.tabs.filter((t) => t.id !== id && !isFixed(t.id)).map((t) => t.id))
  // 批量关闭后聚焦右键目标 tab（HexHub 行为）
  emit('update:modelValue', id)
}

/** 关闭所有：固定标签受保护保留 */
function closeAllTabs(): void {
  closeTabs(props.tabs.filter((t) => !isFixed(t.id)).map((t) => t.id))
  // 关闭后聚焦第一个存活的固定标签（若有）
  const firstAlive = props.tabs.find((t) => isFixed(t.id))
  if (firstAlive) emit('update:modelValue', firstAlive.id)
}

/** 关闭左边：右键目标左侧全部关闭（固定标签受保护） */
function closeLeft(): void {
  const id = menu.value.tabId
  if (!id) return
  const index = props.tabs.findIndex((t) => t.id === id)
  closeTabs(
    props.tabs
      .slice(0, index)
      .filter((t) => !isFixed(t.id))
      .map((t) => t.id),
  )
  emit('update:modelValue', id)
}

/** 关闭右边：右键目标右侧全部关闭（固定标签受保护） */
function closeRight(): void {
  const id = menu.value.tabId
  if (!id) return
  const index = props.tabs.findIndex((t) => t.id === id)
  closeTabs(
    props.tabs
      .slice(index + 1)
      .filter((t) => !isFixed(t.id))
      .map((t) => t.id),
  )
  emit('update:modelValue', id)
}

/** 关闭右键目标 tab；固定标签受保护（菜单项已禁用，此处兜底拦截） */
function closeTarget(): void {
  const id = menu.value.tabId
  if (!id || isFixed(id)) return
  emit('close', id)
}

/** 新窗口打开：复用拖出新窗口机制（HexHub 行为：固定标签不可移动，禁用态一致） */
function openInWindow(): void {
  const id = menu.value.tabId
  if (!id || isFixed(id)) return
  emit('drag-out', id)
}

/** 会话设置：打开 SSH 选项对话框（全局生效） */
function openSessionSettings(): void {
  const id = menu.value.tabId
  if (!id) return
  emit('session-settings', id)
}

/** 复制会话：同一会话再开一个 Tab（emit 给父级走 openTerminal 连接路由） */
function duplicateSession(): void {
  const id = menu.value.tabId
  if (!id) return
  emit('duplicate-session', id)
}

// 菜单禁用态：按当前 tabs 快照计算各关闭项是否可执行
/** 关闭其他禁用态：除目标外无可关闭（非固定）tab */
const hasClosableOthers = computed(
  () => !!menu.value.tabId && props.tabs.some((t) => t.id !== menu.value.tabId && !isFixed(t.id)),
)

/** 关闭所有禁用态：无可关闭（非固定）tab */
const hasClosableAll = computed(() => props.tabs.some((t) => !isFixed(t.id)))

/** 关闭左边禁用态：目标左侧无可关闭（非固定）tab */
const hasClosableLeft = computed(() => {
  const id = menu.value.tabId
  if (!id) return false
  const index = props.tabs.findIndex((t) => t.id === id)
  return props.tabs.slice(0, index).some((t) => !isFixed(t.id))
})

/** 关闭右边禁用态：目标右侧无可关闭（非固定）tab */
const hasClosableRight = computed(() => {
  const id = menu.value.tabId
  if (!id) return false
  const index = props.tabs.findIndex((t) => t.id === id)
  return props.tabs.slice(index + 1).some((t) => !isFixed(t.id))
})

// ---- 拖拽排序 ----
const dragIndex = ref<number | null>(null)
const overIndex = ref<number | null>(null)
/** 标签栏元素引用（拖出检测时用于判定松手坐标是否在栏外） */
const stripRef = ref<HTMLElement | null>(null)
/** “+”新增按钮引用（位于标签栏外，需一并纳入拖出判定范围，避免误判拖出新窗口） */
const newBtnRef = ref<HTMLElement | null>(null)

// ---- 横向溢出检测 & 滚动（滚动条被隐藏，用左右按钮提供可发现性） ----
const canScroll = ref(false)
let resizeObserver: ResizeObserver | null = null

function updateOverflow(): void {
  const el = stripRef.value
  canScroll.value = !!el && el.scrollWidth > el.clientWidth + 1
}

function scrollStrip(dir: 1 | -1): void {
  stripRef.value?.scrollBy({ left: dir * 200, behavior: 'smooth' })
}

onMounted(() => {
  updateOverflow()
  if (stripRef.value && typeof ResizeObserver !== 'undefined') {
    resizeObserver = new ResizeObserver(updateOverflow)
    resizeObserver.observe(stripRef.value)
  }
})

onBeforeUnmount(() => resizeObserver?.disconnect())

watch(
  () => props.tabs.length,
  () => nextTick(updateOverflow),
)

function onDragStart(index: number): void {
  dragIndex.value = index
}

function onDragOver(index: number): void {
  overIndex.value = index
}

function onDrop(index: number): void {
  const from = dragIndex.value
  dragIndex.value = null
  overIndex.value = null
  if (from == null || from === index) return
  const ids = props.tabs.map((t) => t.id)
  const [moved] = ids.splice(from, 1)
  // 右向拖动：splice(from) 删除源项后，目标 index 会前移一位，需先补偿；
  // 例如 [A,B,C,D] 把 A 拖到 C 上，期望 [B,A,C,D]，不补偿会得到 [B,C,A,D]
  ids.splice(from < index ? index - 1 : index, 0, moved)
  emit('reorder', ids)
}

function onDragEnd(e: DragEvent): void {
  const from = dragIndex.value
  // 拖出检测（P1）：未在标签栏内落下（onDrop 未触发，dragIndex 仍保留）且
  // 松手坐标已移出标签栏边界 → 视为拖出新标签；Esc 取消时坐标仍在栏内，不会误判
  if (from != null && stripRef.value) {
    const rect = stripRef.value.getBoundingClientRect()
    // 把位于标签栏外的“+”按钮也纳入栏内判定，避免拖到新增按钮上松手被误判为拖出新窗口
    let left = rect.left
    let right = rect.right
    let top = rect.top
    let bottom = rect.bottom
    const newRect = newBtnRef.value?.getBoundingClientRect()
    if (newRect) {
      left = Math.min(left, newRect.left)
      right = Math.max(right, newRect.right)
      top = Math.min(top, newRect.top)
      bottom = Math.max(bottom, newRect.bottom)
    }
    const outside =
      e.clientX < left - 8 || e.clientX > right + 8 || e.clientY < top - 8 || e.clientY > bottom + 8
    const target = props.tabs[from]
    // 固定标签受保护：不可拖出新窗口（与菜单"新窗口打开"禁用态一致）
    if (outside && target && !isFixed(target.id)) {
      emit('drag-out', target.id)
    }
  }
  dragIndex.value = null
  overIndex.value = null
}
</script>

<template>
  <!-- 根节点拦截浏览器右键菜单；tab 上的右键打开标签菜单 -->
  <div class="flex-tabs" role="tablist" @contextmenu.prevent>
    <v-btn
      v-if="canScroll"
      icon="mdi-chevron-left"
      variant="text"
      size="x-small"
      density="compact"
      title="向左滚动标签"
      class="flex-tabs__scroll"
      @click="scrollStrip(-1)"
    />
    <div ref="stripRef" class="flex-tabs__strip">
      <div
        v-for="(tab, index) in tabs"
        :key="tab.id"
        class="flex-tabs__tab"
        :class="{
          'flex-tabs__tab--active': tab.id === modelValue,
          'flex-tabs__tab--dragging': dragIndex === index || overIndex === index,
        }"
        :style="{ '--tab-color': tab.color || 'transparent' }"
        role="tab"
        :aria-selected="tab.id === modelValue"
        draggable="true"
        @click="activate(tab.id)"
        @mousedown="onMouseDown($event, tab.id)"
        @contextmenu.prevent="openContextMenu($event, tab.id)"
        @dragstart="onDragStart(index)"
        @dragover.prevent="onDragOver(index)"
        @drop.prevent="onDrop(index)"
        @dragend="onDragEnd"
      >
        <v-icon v-if="tab.icon" :icon="tab.icon" size="14" class="flex-tabs__icon" />
        <!-- 固定标识：图钉图标（固定后关闭按钮隐藏，标签受关闭保护） -->
        <v-icon v-if="isFixed(tab.id)" icon="mdi-pin" color="warning" size="12" class="flex-tabs__pin" />
        <span class="flex-tabs__title" :title="tab.title">{{ tab.title }}</span>
        <v-btn
          v-if="(tab.closable ?? showClose) && !isFixed(tab.id)"
          icon="mdi-close"
          variant="text"
          size="x-small"
          density="compact"
          :ripple="false"
          :aria-label="`关闭 ${tab.title}`"
          title="关闭"
          class="flex-tabs__close"
          draggable="false"
          @mousedown.stop
          @click.stop="emit('close', tab.id)"
        />
      </div>
    </div>
    <v-btn
      v-if="canScroll"
      icon="mdi-chevron-right"
      variant="text"
      size="x-small"
      density="compact"
      title="向右滚动标签"
      class="flex-tabs__scroll"
      @click="scrollStrip(1)"
    />
    <v-btn
      v-if="showNew"
      ref="newBtnRef"
      icon="mdi-plus"
      size="18"
      variant="text"
      density="comfortable"
      title="新建标签 (Ctrl+T)"
      class="flex-tabs__new"
      @click="emit('create')"
    />

    <!-- 右键菜单：复制名称 / 固定 / 重命名 / 新窗口打开 / 关闭组（固定标签受关闭保护） -->
    <v-menu
      v-model="menu.visible"
      :target="[menu.x, menu.y]"
      location="bottom start"
      origin="auto"
      :close-on-content-click="true"
    >
      <v-list density="compact" min-width="180">
        <v-list-item prepend-icon="mdi-content-copy" @click="copyTabName">
          <v-list-item-title>复制名称</v-list-item-title>
        </v-list-item>
        <v-list-item
          prepend-icon="mdi-content-duplicate"
          :disabled="menuTab?.duplicatable === false"
          @click="duplicateSession"
        >
          <v-list-item-title>复制会话</v-list-item-title>
        </v-list-item>
        <v-list-item
          :prepend-icon="isFixed(menu.tabId) ? 'mdi-pin-off' : 'mdi-pin'"
          @click="toggleFixed"
        >
          <v-list-item-title>{{ isFixed(menu.tabId) ? '取消固定' : '固定标签' }}</v-list-item-title>
        </v-list-item>
        <v-list-item prepend-icon="mdi-pencil" @click="openRename">
          <v-list-item-title>重命名</v-list-item-title>
        </v-list-item>
        <v-list-item
          prepend-icon="mdi-open-in-new"
          :disabled="isFixed(menu.tabId)"
          @click="openInWindow"
        >
          <v-list-item-title>新窗口打开</v-list-item-title>
        </v-list-item>
        <v-list-item prepend-icon="mdi-tune-vertical" @click="openSessionSettings">
          <v-list-item-title>会话设置</v-list-item-title>
        </v-list-item>
        <v-divider />
        <v-list-item
          prepend-icon="mdi-tab-minus"
          :disabled="!hasClosableOthers"
          @click="closeOthers"
        >
          <v-list-item-title>关闭其他</v-list-item-title>
        </v-list-item>
        <v-list-item prepend-icon="mdi-tab-remove" :disabled="!hasClosableAll" @click="closeAllTabs">
          <v-list-item-title>关闭所有</v-list-item-title>
        </v-list-item>
        <v-list-item
          prepend-icon="mdi-close-circle-multiple-outline"
          :disabled="!hasClosableLeft"
          @click="closeLeft"
        >
          <v-list-item-title>关闭左边</v-list-item-title>
        </v-list-item>
        <v-list-item
          prepend-icon="mdi-close-box-multiple-outline"
          :disabled="!hasClosableRight"
          @click="closeRight"
        >
          <v-list-item-title>关闭右边</v-list-item-title>
        </v-list-item>
        <v-divider />
        <v-list-item prepend-icon="mdi-close" :disabled="isFixed(menu.tabId)" @click="closeTarget">
          <v-list-item-title>关闭</v-list-item-title>
        </v-list-item>
      </v-list>
    </v-menu>

    <!-- 重命名标签对话框 -->
    <v-dialog v-model="renameVisible" width="400">
      <v-card>
        <v-card-title class="d-flex align-center text-subtitle-1">重命名标签
          <v-spacer />
          <v-btn
          icon="mdi-close"
          size="x-small"
          variant="text"
          title="关闭"
          @click="renameVisible = false"
          />
        </v-card-title>
        <v-divider />
        <v-card-text>
          <div class="fy-field-row">
            <span class="fy-field-row__label">标签名称</span>
            <v-text-field
              v-model="renameValue"
              density="compact"
              variant="outlined"
              autofocus
              @keyup.enter="confirmRename"
            />
          </div>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="renameVisible = false">取消</v-btn>
          <v-btn color="primary" @click="confirmRename">确定</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </div>
</template>

<style scoped>
.flex-tabs {
  display: flex;
  align-items: center;
  height: 36px;
  min-height: 36px;
  background: var(--fy-chrome-bg);
  border-bottom: 1px solid var(--fy-chrome-border);
  user-select: none;
}

.flex-tabs__strip {
  display: flex;
  align-items: stretch;
  flex: 1 1 auto;
  min-width: 0;
  overflow-x: auto;
  scrollbar-width: none; /* 紧凑信息密度：隐藏横向滚动条 */
}

.flex-tabs__strip::-webkit-scrollbar {
  display: none;
}

.flex-tabs__tab {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 0 8px 0 10px;
  max-width: 220px;
  cursor: pointer;
  font-size: 12px;
  color: rgb(var(--v-theme-on-surface));
  opacity: 0.75;
  border-left: 1px solid transparent;
  border-right: 1px solid transparent;
  border-top: 2px solid transparent;
  white-space: nowrap;
  /* 批2：激活/悬停底色与顶边沿色过渡使用统一动效令牌（克制、非跳变） */
  transition:
    background var(--fy-dur-fast) var(--fy-ease),
    border-top-color var(--fy-dur-fast) var(--fy-ease);
  /* 新增标签入场：透明 + 微缩放（Vue v-for key 复用 DOM，仅插入节点触发，重渲染不重播） */
  animation: flex-tabs-in 130ms var(--fy-ease);
}

@keyframes flex-tabs-in {
  from {
    opacity: 0;
    transform: scale(0.98);
  }
}

.flex-tabs__tab:hover {
  opacity: 0.85;
  background: var(--fy-hover-bg);
}

/* 激活 Tab：顶边按连接着色 */
.flex-tabs__tab--active {
  opacity: 1;
  background: rgb(var(--v-theme-surface));
  border-top-color: var(--tab-color);
}

.flex-tabs__tab--dragging {
  outline: 1px dashed rgb(var(--v-theme-primary, 82 132 255));
  outline-offset: -2px;
}

.flex-tabs__title {
  overflow: hidden;
  text-overflow: ellipsis;
  flex: 1 1 auto;
  min-width: 0;
}

.flex-tabs__close {
  margin-left: 2px;
  border-radius: 4px;
  opacity: 0.4; /* 普通态弱化显示（可发现、触屏可达），hover/焦点时强调 */
  transition:
    opacity var(--fy-dur-fast) var(--fy-ease),
    background var(--fy-dur-fast) var(--fy-ease);
  flex: 0 0 auto;
}

.flex-tabs__tab:hover .flex-tabs__close,
.flex-tabs__tab--active .flex-tabs__close,
.flex-tabs__close:focus-visible {
  opacity: 1;
}

.flex-tabs__close:hover {
  opacity: 1;
  background: rgba(var(--v-theme-on-surface), 0.12);
}

/* 固定标识：图钉图标（固定后显示，关闭按钮隐藏） */
.flex-tabs__pin {
  flex: 0 0 auto;
  opacity: 0.85;
}

/* 溢出滚动按钮：竖直居中，弱化显示 */
.flex-tabs__scroll {
  flex: 0 0 auto;
  opacity: 0.6;
  transition:
    opacity var(--fy-dur-fast) var(--fy-ease),
    background var(--fy-dur-fast) var(--fy-ease);
}

.flex-tabs__scroll:hover {
  opacity: 1;
  background: var(--fy-hover-bg);
}

/* 按下（pressed）反馈：微缩 + 完全显形（批2 补滚动箭头按压态） */
.flex-tabs__scroll:active {
  opacity: 1;
  transform: scale(0.92);
}

.flex-tabs__new {
  flex: 0 0 auto;
  margin: 0 4px;
}
</style>
