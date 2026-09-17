<template>
  <v-navigation-drawer
    :model-value="modelValue"
    location="right"
    temporary
    width="420"
    class="history-drawer"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <!-- 顶部：标题 + 清空按钮 -->
    <div class="history-drawer__header">
      <v-icon size="small" class="mr-1">mdi-history</v-icon>
      <span class="history-drawer__title">查询历史</span>
      <v-spacer />
      <v-tooltip text="清空历史" location="bottom">
        <template #activator="{ props: activatorProps }">
          <v-btn
            v-bind="activatorProps"
            icon="mdi-delete-outline"
            size="x-small"
            variant="text"
            :disabled="items.length === 0"
            @click="doClear"
          />
        </template>
      </v-tooltip>
    </div>

    <!-- 搜索框：走 mysql_history_search -->
    <div class="history-drawer__search">
      <v-text-field
        v-model="keyword"
        placeholder="搜索 SQL 关键词"
        prepend-inner-icon="mdi-magnify"
        density="compact"
        variant="outlined"
        hide-details
        clearable
        autofocus
        @update:model-value="onSearchInput"
        @keyup.enter="doSearch"
      />
    </div>

    <!-- 历史列表 -->
    <div class="history-drawer__body">
      <v-list density="compact" class="history-drawer__list">
        <v-list-item
          v-for="item in items"
          :key="item.id"
          class="history-drawer__item"
          @click="emit('recall', item.sql)"
        >
          <template #prepend>
            <v-icon size="small" :color="item.success ? 'success' : 'error'">
              {{ item.success ? 'mdi-check-circle-outline' : 'mdi-alert-circle-outline' }}
            </v-icon>
          </template>
          <v-list-item-title class="history-drawer__sql">{{ sqlPreview(item.sql) }}</v-list-item-title>
          <v-list-item-subtitle class="history-drawer__meta">
            {{ formatTime(item.created_at) }} · {{ formatDuration(item.duration_ms) }}
          </v-list-item-subtitle>
        </v-list-item>
        <v-list-item v-if="!loading && items.length === 0">
          <v-list-item-title class="text-caption text-medium-emphasis">
            {{ keyword.trim() ? '无匹配记录' : '暂无查询历史' }}
          </v-list-item-title>
        </v-list-item>
      </v-list>
      <div v-if="error" class="history-drawer__error text-caption">{{ error }}</div>
    </div>
  </v-navigation-drawer>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import {
  mysqlHistoryClear,
  mysqlHistoryList,
  mysqlHistorySearch,
  type MySqlQueryHistoryItem,
} from '@/api/mysqlConsole'
import { useUiStore } from '@/stores/ui'

const props = defineProps<{
  /** 显隐（由父级 v-model 控制） */
  modelValue: boolean
}>()

const emit = defineEmits<{
  /** 显隐变化（v-model） */
  (e: 'update:modelValue', value: boolean): void
  /** 快召回：点击历史条目，由父级将 SQL 插入编辑器 */
  (e: 'recall', sql: string): void
}>()

const uiStore = useUiStore()

const DEFAULT_LIMIT = 100

// ---------- 列表加载与搜索 ----------
const items = ref<MySqlQueryHistoryItem[]>([])
const loading = ref(false)
const error = ref('')
const keyword = ref('')

/** 搜索输入防抖（300ms 后自动搜索，Enter 立即搜索） */
let searchTimer: number | null = null

function onSearchInput(): void {
  if (searchTimer !== null) window.clearTimeout(searchTimer)
  searchTimer = window.setTimeout(() => void doSearch(), 300)
}

async function doSearch(): Promise<void> {
  if (searchTimer !== null) {
    window.clearTimeout(searchTimer)
    searchTimer = null
  }
  const kw = keyword.value.trim()
  loading.value = true
  error.value = ''
  try {
    if (kw) {
      items.value = await mysqlHistorySearch(kw, DEFAULT_LIMIT)
    } else {
      items.value = await mysqlHistoryList(DEFAULT_LIMIT)
    }
  } catch (err) {
    error.value = `加载历史失败: ${String(err)}`
  } finally {
    loading.value = false
  }
}

/** 打开抽屉时加载历史（默认 100 条倒序） */
watch(
  () => props.modelValue,
  (open) => {
    if (open) void doSearch()
  },
)

// ---------- 清空（ui store 二次确认） ----------
async function doClear(): Promise<void> {
  try {
    const ok = await uiStore.confirm({
      title: '清空确认',
      message: '确定要清空全部查询历史吗？此操作不可恢复。',
      danger: true,
    })
    if (!ok) return
    await mysqlHistoryClear()
    uiStore.toast('查询历史已清空', 'success')
    await doSearch()
  } catch (err) {
    console.error('[history-drawer] 清空失败:', err)
    uiStore.toast('清空失败，请重试', 'error')
  }
}

// ---------- 展示格式化 ----------
/** SQL 单行摘要（首行，超长截断） */
function sqlPreview(sql: string): string {
  const text = sql.split('\n')[0] ?? ''
  return text.length > 48 ? `${text.slice(0, 48)}…` : text
}

/** 时间格式化：created_at 为 Rust 侧 ISO 字符串 */
function formatTime(createdAt: string): string {
  const d = new Date(createdAt)
  if (Number.isNaN(d.getTime())) return createdAt
  const pad = (n: number): string => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
}

/** 耗时格式化：毫秒 → 自适应单位 */
function formatDuration(durationMs: number | null): string {
  if (durationMs === null) return '-'
  if (durationMs < 1) return '<1ms'
  if (durationMs < 1000) return `${Math.round(durationMs)}ms`
  return `${(durationMs / 1000).toFixed(2)}s`
}
</script>

<style scoped>
.history-drawer__header {
  display: flex;
  align-items: center;
  padding: 10px 12px 6px;
}

.history-drawer__title {
  font-weight: 600;
  font-size: 0.9rem;
}

.history-drawer__search {
  padding: 0 12px 6px;
}

.history-drawer__body {
  flex: 1 1 auto;
  overflow-y: auto;
  min-height: 0;
}

.history-drawer__list {
  padding: 0 4px;
}

.history-drawer__item {
  min-height: 44px;
  user-select: none;
}

.history-drawer__item:hover {
  background: rgba(var(--v-theme-on-surface), 0.06);
}

/* SQL 单行摘要：等宽字体 + 溢出省略 */
.history-drawer__sql {
  font-family: 'Cascadia Mono', Consolas, monospace;
  font-size: 12px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.history-drawer__meta {
  font-size: 0.75rem;
}

.history-drawer__error {
  padding: 8px 12px;
  color: rgb(var(--v-theme-error));
}
</style>
