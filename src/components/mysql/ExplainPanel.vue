<template>
  <v-dialog
    :model-value="modelValue"
    width="680"
    scrollable
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <v-card class="explain-panel">
      <!-- 顶部：标题 + ANALYZE 按钮 -->
      <div class="explain-panel__header">
        <v-icon size="small" class="mr-1">mdi-file-tree</v-icon>
        <span class="explain-panel__title">执行计划</span>
        <v-spacer />
        <v-tooltip text="仅 SELECT 语句支持 EXPLAIN ANALYZE" location="bottom">
          <template #activator="{ props: activatorProps }">
            <v-btn
              v-bind="activatorProps"
              size="small"
              variant="outlined"
              prepend-icon="mdi-play-circle-outline"
              :loading="loading"
              :disabled="!isSelectSql"
              @click="load(true)"
            >
              ANALYZE 执行
            </v-btn>
          </template>
        </v-tooltip>
      </div>
      <v-divider />

      <!-- SQL 预览 -->
      <div class="explain-panel__sql">{{ sqlPreview }}</div>

      <!-- 错误提示 -->
      <v-alert v-if="error" type="error" variant="tonal" density="compact" class="mx-3 mt-2">
        {{ error }}
      </v-alert>

      <!-- 结果区：rows 表格 / tree 等宽文本树 -->
      <v-card-text class="explain-panel__body">
        <!-- 表格模式：format="rows" 时渲染 columns/rows -->
        <v-table
          v-if="result && result.format === 'rows' && !hasTree"
          density="compact"
          fixed-header
          class="explain-panel__table"
        >
          <thead>
            <tr>
              <th v-for="col in result.columns" :key="col" class="text-left">{{ col }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(row, ri) in result.rows" :key="ri">
              <td v-for="(cell, ci) in row" :key="ci" class="text-body-2">
                <span v-if="cell === null" class="explain-panel__null">NULL</span>
                <template v-else>{{ cell }}</template>
              </td>
            </tr>
            <tr v-if="result.rows.length === 0">
              <td :colspan="result.columns.length || 1" class="text-caption text-medium-emphasis text-center py-2">
                空结果集
              </td>
            </tr>
          </tbody>
        </v-table>

        <!-- 树模式：format="tree" 或 tree 字段非空时渲染等宽文本树 -->
        <pre v-else-if="treeText" class="explain-panel__tree"><code>{{ treeText }}</code></pre>

        <div v-else-if="!loading" class="explain-panel__empty text-caption text-medium-emphasis">
          暂无执行计划数据
        </div>
      </v-card-text>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { mysqlExplain, type MySqlExplainResult } from '@/api/mysqlConsole'

const props = defineProps<{
  /** 显隐（由父级 v-model 控制） */
  modelValue: boolean
  /** MySQL 连接 ID（mysqlConnect 返回值） */
  connId: string
  /** 待分析的 SQL 语句 */
  sql: string
}>()

const emit = defineEmits<{
  /** 显隐变化（v-model） */
  (e: 'update:modelValue', value: boolean): void
}>()

// ---------- 加载 ----------
const result = ref<MySqlExplainResult | null>(null)
const loading = ref(false)
const error = ref('')

/** 打开对话框时加载执行计划（analyze 默认 false） */
watch(
  () => props.modelValue,
  (open) => {
    if (open) void load(false)
  },
)

async function load(analyze: boolean): Promise<void> {
  const text = props.sql.trim()
  if (!text) return
  loading.value = true
  error.value = ''
  try {
    result.value = await mysqlExplain(props.connId, text, analyze)
  } catch (err) {
    error.value = `执行计划获取失败: ${String(err)}`
  } finally {
    loading.value = false
  }
}

// ---------- ANALYZE 前端预检：仅 SELECT 可用 ----------
const SELECT_RE = /^\s*SELECT\b/i

const isSelectSql = computed(() => SELECT_RE.test(props.sql))

// ---------- 展示 ----------
/** 树文本：format="tree" 或 tree 字段非空时优先展示 */
const hasTree = computed(() => !!result.value?.tree)

const treeText = computed<string | null>(() => result.value?.tree ?? null)

/** SQL 预览（超长截断） */
const sqlPreview = computed<string>(() => {
  const text = props.sql.trim()
  return text.length > 120 ? `${text.slice(0, 120)}…` : text
})
</script>

<style scoped>
.explain-panel__header {
  display: flex;
  align-items: center;
  padding: 10px 12px;
}

.explain-panel__title {
  font-weight: 400;
  font-size: 14px;
}

.explain-panel__sql {
  padding: 0 12px 8px;
  font-family: var(--fy-font);
  font-size: 14px;
  color: rgba(var(--v-theme-on-surface), 0.7);
  word-break: break-all;
}

.explain-panel__body {
  max-height: 480px;
  overflow: auto;
}

.explain-panel__table {
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
}

/* SQL NULL：灰色斜体（与 MysqlDataGrid 一致） */
.explain-panel__null {
  font-style: italic;
  opacity: 0.55;
  font-size: 14px;
}

/* 等宽文本树（EXPLAIN FORMAT=TREE 输出） */
.explain-panel__tree {
  margin: 0;
  padding: 8px;
  font-family: var(--fy-font);
  font-size: 14px;
  line-height: 1.6;
  white-space: pre;
  overflow-x: auto;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
}

.explain-panel__empty {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 80px;
}
</style>
