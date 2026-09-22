<template>
  <v-navigation-drawer
    :model-value="modelValue"
    location="right"
    temporary
    width="420"
    class="history-drawer"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <!-- 顶部：标题 + 清空按钮（仅历史 tab） -->
    <div class="history-drawer__header">
      <v-icon size="small" class="mr-1">mdi-history</v-icon>
      <span class="history-drawer__title">SQL 控制台</span>
      <v-spacer />
      <v-tooltip v-if="tab === 'history'" text="清空历史" location="bottom">
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

    <!-- tab 切换：查询历史 / 已保存查询 -->
    <v-tabs v-model="tab" density="compact" color="primary">
      <v-tab value="history" class="text-caption">查询历史</v-tab>
      <v-tab value="saved" class="text-caption">已保存查询</v-tab>
    </v-tabs>
    <v-divider />

    <!-- 历史 tab：搜索框 + 列表 -->
    <template v-if="tab === 'history'">
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
    </template>

    <!-- 已保存 tab：命名查询列表（点击召回 / 重命名 / 删除） -->
    <div v-else class="history-drawer__body">
      <v-list density="compact" class="history-drawer__list">
        <v-list-item
          v-for="q in savedItems"
          :key="q.id"
          class="history-drawer__item"
          @click="emit('recall', q.sql)"
        >
          <template #prepend>
            <v-icon size="small" color="primary">mdi-bookmark-outline</v-icon>
          </template>
          <v-list-item-title class="history-drawer__name">{{ q.name }}</v-list-item-title>
          <v-list-item-subtitle class="history-drawer__meta">
            {{ sqlPreview(q.sql) }}
          </v-list-item-subtitle>
          <v-list-item-subtitle class="history-drawer__meta">
            {{ q.conn_id ? `绑定连接 · ${q.conn_id}` : '全局' }} · {{ formatTime(q.created_at) }}
          </v-list-item-subtitle>
          <template #append>
            <v-btn
              icon="mdi-pencil-outline"
              size="x-small"
              variant="text"
              title="重命名"
              @click.stop="openRename(q)"
            />
            <v-btn
              icon="mdi-delete-outline"
              size="x-small"
              variant="text"
              title="删除"
              @click.stop="doDeleteSaved(q)"
            />
          </template>
        </v-list-item>
        <v-list-item v-if="!savedLoading && savedItems.length === 0">
          <v-list-item-title class="text-caption text-medium-emphasis">
            暂无已保存查询
          </v-list-item-title>
        </v-list-item>
      </v-list>
      <div v-if="savedError" class="history-drawer__error text-caption">{{ savedError }}</div>
    </div>

    <!-- 重命名对话框（已保存查询） -->
    <v-dialog v-model="renameDialog" max-width="380">
      <v-card>
        <v-card-title class="d-flex align-center text-subtitle-1">重命名查询
          <v-spacer />
          <v-btn
          icon="mdi-close"
          size="x-small"
          variant="text"
          title="关闭"
          @click="renameDialog = false"
          />
        </v-card-title>
        <v-card-text>
          <div class="fy-field-row">
            <span class="fy-field-row__label">查询名称</span>
            <v-text-field
              v-model="renameName"
              density="compact"
              variant="outlined"
              autofocus
              counter="100"
              @keyup.enter="confirmRename"
            />
          </div>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="renameDialog = false">取消</v-btn>
          <v-btn color="primary" prepend-icon="mdi-check" @click="confirmRename">确认</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </v-navigation-drawer>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import {
  mysqlHistoryClear,
  mysqlHistoryList,
  mysqlHistorySearch,
  mysqlSavedQueryDelete,
  mysqlSavedQueryList,
  mysqlSavedQueryRename,
  type MySqlSavedQueryItem,
  type MySqlQueryHistoryItem,
} from '@/api/mysqlConsole'
import { useUiStore } from '@/stores/ui'
import { friendlyError } from '@/utils/errors'

const props = defineProps<{
  /** 显隐（由父级 v-model 控制） */
  modelValue: boolean
}>()

const emit = defineEmits<{
  /** 显隐变化（v-model） */
  (e: 'update:modelValue', value: boolean): void
  /** 快召回：点击历史/已保存条目，由父级将 SQL 插入编辑器 */
  (e: 'recall', sql: string): void
}>()

const uiStore = useUiStore()

const DEFAULT_LIMIT = 100

// ---------- tab 切换（查询历史 / 已保存查询） ----------
const tab = ref<'history' | 'saved'>('history')

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
    error.value = `加载历史失败: ${friendlyError(err)}`
  } finally {
    loading.value = false
  }
}

/** 打开抽屉时加载当前 tab 的列表 */
watch(
  () => props.modelValue,
  (open) => {
    if (open) {
      if (tab.value === 'history') void doSearch()
      else void loadSaved()
    }
  },
)

/** 切到已保存 tab 时加载列表 */
watch(tab, (t) => {
  if (t === 'saved') void loadSaved()
})

// ---------- 已保存查询：列表 / 重命名 / 删除 ----------
const savedItems = ref<MySqlSavedQueryItem[]>([])
const savedLoading = ref(false)
const savedError = ref('')
const renameDialog = ref(false)
const renameTarget = ref<MySqlSavedQueryItem | null>(null)
const renameName = ref('')

async function loadSaved(): Promise<void> {
  savedLoading.value = true
  savedError.value = ''
  try {
    savedItems.value = await mysqlSavedQueryList()
  } catch (err) {
    savedError.value = `加载已保存查询失败: ${friendlyError(err)}`
  } finally {
    savedLoading.value = false
  }
}

/** 打开重命名对话框（预填当前名称） */
function openRename(q: MySqlSavedQueryItem): void {
  renameTarget.value = q
  renameName.value = q.name
  renameDialog.value = true
}

async function confirmRename(): Promise<void> {
  const target = renameTarget.value
  const name = renameName.value.trim()
  if (!target || !name) return
  try {
    await mysqlSavedQueryRename(target.id, name)
    uiStore.toast(`已重命名为：${name}`, 'success')
    renameDialog.value = false
    await loadSaved()
  } catch (err) {
    console.error('[history-drawer] 重命名失败:', err)
    uiStore.toast(friendlyError(err), 'error')
  }
}

/** 删除已保存查询（uiStore 二次确认） */
async function doDeleteSaved(q: MySqlSavedQueryItem): Promise<void> {
  try {
    const ok = await uiStore.confirm({
      title: '删除确认',
      message: `确定要删除已保存查询「${q.name}」吗？此操作不可恢复。`,
      danger: true,
    })
    if (!ok) return
    await mysqlSavedQueryDelete(q.id)
    uiStore.toast('已删除', 'success')
    await loadSaved()
  } catch (err) {
    console.error('[history-drawer] 删除失败:', err)
    uiStore.toast('删除失败，请重试', 'error')
  }
}

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
  font-weight: 400;
  font-size: 14px;
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
  font-family: var(--fy-font);
  font-size: 14px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 已保存查询名称 */
.history-drawer__name {
  font-weight: 400;
  font-size: 14px;
}

.history-drawer__meta {
  font-size: 14px;
}

.history-drawer__error {
  padding: 8px 12px;
  color: rgb(var(--v-theme-error));
}

/* rem 换算非整数档修复：text-caption/body-2 10.5/12.25px → 12px 整数档 */
.text-caption,
.text-subtitle-2,
.text-body-2 {
  font-size: 12px !important;
}
</style>
