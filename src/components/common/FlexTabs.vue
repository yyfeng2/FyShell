<script setup lang="ts">
/**
 * FlexTabs —— 通用标签栏组件
 *
 * 特性：
 * - 新增（加号）/关闭按钮、中键关闭、点击切换
 * - HTML5 拖拽排序（拖动后 emit reorder，携带新 id 顺序）
 * - Tab 着色支持：tab.color（hex）作为激活态指示色（参考 Xshell 按连接着色）
 *
 * 布局约定：紧凑行高，深色主题友好，可被任意视图复用。
 */
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'

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
}>()

// ---- 点击切换 ----
function activate(id: string): void {
  if (id !== props.modelValue) emit('update:modelValue', id)
}

// ---- 中键关闭 ----
function onMouseDown(e: MouseEvent, id: string): void {
  if (e.button === 1) {
    // 阻止浏览器中键自动滚动
    e.preventDefault()
    emit('close', id)
  }
}

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
  ids.splice(index, 0, moved)
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
    if (outside && props.tabs[from]) {
      emit('drag-out', props.tabs[from].id)
    }
  }
  dragIndex.value = null
  overIndex.value = null
}
</script>

<template>
  <div class="flex-tabs" role="tablist">
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
        @dragstart="onDragStart(index)"
        @dragover.prevent="onDragOver(index)"
        @drop.prevent="onDrop(index)"
        @dragend="onDragEnd"
      >
        <v-icon v-if="tab.icon" :icon="tab.icon" size="14" class="flex-tabs__icon" />
        <span class="flex-tabs__title" :title="tab.title">{{ tab.title }}</span>
        <v-btn
          v-if="(tab.closable ?? showClose)"
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
  </div>
</template>

<style scoped>
.flex-tabs {
  display: flex;
  align-items: center;
  height: 36px;
  min-height: 36px;
  background: var(--fy-chrome-bg, #f0f2f5);
  border-bottom: 1px solid var(--fy-chrome-border, #d5d9de);
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
}

.flex-tabs__tab:hover {
  opacity: 0.85;
  background: rgb(var(--v-theme-on-surface) / 0.06);
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
  border-radius: 3px;
  opacity: 0.4; /* 普通态弱化显示（可发现、触屏可达），hover/焦点时强调 */
  transition: opacity 0.12s;
  flex: 0 0 auto;
}

.flex-tabs__tab:hover .flex-tabs__close,
.flex-tabs__tab--active .flex-tabs__close,
.flex-tabs__close:focus-visible {
  opacity: 1;
}

.flex-tabs__close:hover {
  opacity: 1;
  background: rgb(var(--v-theme-on-surface) / 0.12);
}

/* 溢出滚动按钮：竖直居中，弱化显示 */
.flex-tabs__scroll {
  flex: 0 0 auto;
  opacity: 0.6;
}

.flex-tabs__scroll:hover {
  opacity: 1;
}

.flex-tabs__new {
  flex: 0 0 auto;
  margin: 0 4px;
}
</style>
