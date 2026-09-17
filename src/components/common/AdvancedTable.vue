<script setup lang="ts">
/**
 * AdvancedTable —— 虚拟滚动表格（大数据不卡）
 *
 * 特性：
 * - 自实现虚拟滚动：只渲染可视区 + 上下缓冲行，万级行数据流畅
 * - 列定义 props（宽度/排序/对齐/自定义格式化）
 * - 表头点击排序（内部排序 + sort 事件通知父级）
 * - 紧凑行高（默认 30px）保证信息密度
 * - 行点击事件
 */
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'

/** 列定义 */
export interface AdvancedTableColumn {
  key: string
  title: string
  /** 列宽（px）；未指定则自动均分 */
  width?: number
  /** 是否可排序（表头点击） */
  sortable?: boolean
  align?: 'start' | 'center' | 'end'
  /** 自定义单元格格式化 */
  format?: (row: Record<string, unknown>) => string
}

const props = withDefaults(
  defineProps<{
    columns: AdvancedTableColumn[]
    /** 行数据（大数组亦可，内部虚拟滚动） */
    rows: Record<string, unknown>[]
    /** 行唯一标识字段名 */
    rowKey?: string
    /** 行高（px，紧凑默认 30） */
    rowHeight?: number
    loading?: boolean
    /** 虚拟滚动上下缓冲行数 */
    overscan?: number
    /** 是否启用行点击选中态（内部维护选中行索引） */
    selectable?: boolean
  }>(),
  { rowKey: 'id', rowHeight: 30, loading: false, overscan: 10, selectable: false },
)

const emit = defineEmits<{
  (e: 'row-click', row: Record<string, unknown>, index: number): void
  /** 排序变化通知（内部已排序，父级可选择自行同步） */
  (e: 'sort', payload: { key: string; order: 'asc' | 'desc' }): void
}>()

// ---------------- 排序 ----------------

const sortKey = ref<string | null>(null)
const sortOrder = ref<'asc' | 'desc'>('asc')

function toggleSort(key: string): void {
  if (sortKey.value === key) {
    sortOrder.value = sortOrder.value === 'asc' ? 'desc' : 'asc'
  } else {
    sortKey.value = key
    sortOrder.value = 'asc'
  }
  emit('sort', { key, order: sortOrder.value })
}

function compareValues(a: unknown, b: unknown): number {
  if (typeof a === 'number' && typeof b === 'number') return a - b
  if (typeof a === 'boolean' && typeof b === 'boolean') return (a ? 1 : 0) - (b ? 1 : 0)
  return String(a).localeCompare(String(b), 'zh-CN')
}

const sortedRows = computed(() => {
  const key = sortKey.value
  if (!key) return props.rows
  const order = sortOrder.value === 'asc' ? 1 : -1
  return [...props.rows].sort((a, b) => {
    const av = a[key]
    const bv = b[key]
    const aEmpty = av == null
    const bEmpty = bv == null
    // 空值恒排最后，置于 order 翻转之外，保证升序/降序行为一致
    if (aEmpty || bEmpty) {
      if (aEmpty && bEmpty) return 0
      return aEmpty ? 1 : -1
    }
    return compareValues(av, bv) * order
  })
})

// ---------------- 虚拟滚动 ----------------

const viewport = ref<HTMLElement | null>(null)
const scrollTop = ref(0)
const scrollLeft = ref(0)
const viewportHeight = ref(400)
/** 选中行索引（selectable 时有效） */
const selectedIndex = ref<number | null>(null)
let resizeObserver: ResizeObserver | null = null

function onScroll(): void {
  if (viewport.value) {
    scrollTop.value = viewport.value.scrollTop
    // 横向滚动同步表头（与表体同网格模板，平移使其对齐）
    scrollLeft.value = viewport.value.scrollLeft
  }
}

/**
 * 排序或数据规模变化后，可能使列表变短导致 startIndex 短暂越界。
 * 归零滚动位置，避免新列表未铺满时出现空白。
 */
watch(
  () => [sortKey.value, sortOrder.value, sortedRows.value.length],
  () => {
    scrollTop.value = 0
    scrollLeft.value = 0
    if (viewport.value) viewport.value.scrollTop = 0
  },
)

onMounted(() => {
  if (viewport.value) {
    viewportHeight.value = viewport.value.clientHeight
    resizeObserver = new ResizeObserver(() => {
      if (viewport.value) viewportHeight.value = viewport.value.clientHeight
    })
    resizeObserver.observe(viewport.value)
  }
})

onUnmounted(() => {
  resizeObserver?.disconnect()
  resizeObserver = null
})

/** 起始渲染行（含上缓冲） */
const startIndex = computed(() => {
  const first = Math.floor(scrollTop.value / props.rowHeight)
  return Math.max(0, first - props.overscan)
})

/** 渲染行数 = 可见行数 + 上下双侧各一份 overscan（避免快速滚到底缘闪空白） */
const renderCount = computed(() => {
  const visible = Math.ceil(viewportHeight.value / props.rowHeight)
  return visible + props.overscan * 2
})

/** 实际渲染的行（携带原索引供 row-click 使用） */
const displayRows = computed(() => {
  const start = startIndex.value
  const slice = sortedRows.value.slice(start, start + renderCount.value)
  return slice.map((row, i) => ({ row, index: start + i }))
})

/** 上方占位高度，决定渲染窗口的偏移 */
const offsetY = computed(() => startIndex.value * props.rowHeight)

/** 总高度占位 */
const totalHeight = computed(() => sortedRows.value.length * props.rowHeight)

// ---------------- 列对齐（表头与行共用同一网格模板） ----------------

const gridStyle = computed(() => ({
  gridTemplateColumns: props.columns
    .map((c) => (c.width ? `${c.width}px` : 'minmax(0, 1fr)'))
    .join(' '),
}))

/** 表头样式：与表体同网格并跟随横向滚动（反向平移以对齐） */
const headerStyle = computed(() => ({
  ...gridStyle.value,
  transform: `translateX(-${scrollLeft.value}px)`,
}))

function cellText(row: Record<string, unknown>, col: AdvancedTableColumn): string {
  if (col.format) return col.format(row)
  const v = row[col.key]
  return v == null ? '' : String(v)
}

/** 行点击：selectable 时同步内部选中状态并转发事件 */
function onRowClick(row: Record<string, unknown>, index: number): void {
  if (props.selectable) selectedIndex.value = index
  emit('row-click', row, index)
}
</script>

<template>
  <div class="adv-table">
    <div v-if="loading" class="adv-table__progress">
      <v-progress-linear indeterminate height="2" />
    </div>

    <!-- 表头（固定，跟随横向滚动反向平移对齐） -->
    <div class="adv-table__header" :style="headerStyle">
      <div
        v-for="col in columns"
        :key="col.key"
        class="adv-table__th"
        :class="[`adv-table__th--${col.align ?? 'start'}`, { 'adv-table__th--sortable': col.sortable }]"
        @click="col.sortable && toggleSort(col.key)"
      >
        <span class="adv-table__th-text">{{ col.title }}</span>
        <v-icon
          v-if="sortKey === col.key"
          size="12"
          class="adv-table__sort-icon"
          :icon="sortOrder === 'asc' ? 'mdi-arrow-up' : 'mdi-arrow-down'"
        />
      </div>
    </div>

    <!-- 表体（虚拟滚动） -->
    <div ref="viewport" class="adv-table__viewport" @scroll.passive="onScroll">
      <div class="adv-table__spacer" :style="{ height: `${totalHeight}px` }">
        <div class="adv-table__body" :style="{ transform: `translateY(${offsetY}px)`, ...gridStyle }">
          <div
            v-for="item in displayRows"
            :key="String(item.row[rowKey] ?? item.index)"
            class="adv-table__row"
            :class="{ 'adv-table__row--selected': selectable && selectedIndex === item.index }"
            :style="{ height: `${rowHeight}px` }"
            @click="onRowClick(item.row, item.index)"
          >
            <div
              v-for="col in columns"
              :key="col.key"
              class="adv-table__cell"
              :class="`adv-table__cell--${col.align ?? 'start'}`"
              :title="cellText(item.row, col)"
            >
              {{ cellText(item.row, col) }}
            </div>
          </div>
          <!-- 空数据占位 -->
          <div v-if="rows.length === 0 && !loading" class="adv-table__empty">暂无数据</div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.adv-table {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  overflow: hidden;
  font-size: 12px;
}

.adv-table__progress {
  flex: 0 0 auto;
}

/* 表头 */
.adv-table__header {
  display: grid;
  min-height: 28px;
  border-bottom: 1px solid rgb(var(--v-theme-surface-variant, 32 33 35));
  background: rgb(var(--v-theme-surface-variant, 32 33 35) / 0.25);
  font-weight: 600;
  user-select: none;
  will-change: transform;
}

.adv-table__th {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 0 8px;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.adv-table__th--sortable {
  cursor: pointer;
}

.adv-table__th--sortable:hover {
  background: rgb(var(--v-theme-on-surface) / 0.06);
}

.adv-table__th--center {
  justify-content: center;
}

.adv-table__th--end {
  justify-content: flex-end;
}

.adv-table__th-text {
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 表体 */
.adv-table__viewport {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overflow-x: auto;
  position: relative;
}

.adv-table__spacer {
  position: relative;
  width: 100%;
}

.adv-table__body {
  display: grid;
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  will-change: transform;
}

/* 紧凑行高保证信息密度 */
.adv-table__row {
  display: grid;
  align-items: center;
  cursor: default;
  border-bottom: 1px solid rgb(var(--v-theme-surface-variant, 32 33 35) / 0.25);
}

.adv-table__row:hover {
  background: rgb(var(--v-theme-on-surface) / 0.06);
}

.adv-table__row--selected {
  background: rgba(var(--v-theme-primary), 0.18);
}

.adv-table__cell {
  padding: 0 8px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.adv-table__cell--center {
  text-align: center;
}

.adv-table__cell--end {
  text-align: end;
}

.adv-table__empty {
  padding: 16px;
  text-align: center;
  color: rgb(var(--v-theme-on-surface) / 0.5);
  grid-column: 1 / -1;
}
</style>
