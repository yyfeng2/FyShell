<template>
  <div class="redis-result">
    <!-- status：绿色状态行 -->
    <div v-if="result.kind === 'status'" class="redis-result__status">
      <v-icon size="small" color="success" class="mr-1">mdi-check-circle</v-icon>
      <span class="text-body-2 text-success redis-result__mono">{{ cell(result.value) }}</span>
    </div>

    <!-- blob：二进制值（后端非 UTF-8 转 hex 字符串，较长时截断展示） -->
    <div v-else-if="result.kind === 'blob'" class="redis-result__block">
      <div class="text-caption text-medium-emphasis mb-1">二进制值（blob，hex 片段）</div>
      <div class="redis-result__code" :title="blobText">{{ blobSnippet }}</div>
    </div>

    <!-- 单值：string / int -->
    <div v-else-if="result.kind === 'string' || result.kind === 'int'" class="redis-result__code">
      {{ cell(result.value) }}
    </div>

    <!-- nil -->
    <div v-else-if="result.kind === 'nil'" class="text-caption text-medium-emphasis">
      <span class="redis-result__nil">(nil)</span>
    </div>

    <!-- error（父组件通常单独呈现为 v-alert，此处兜底展示） -->
    <div v-else-if="result.kind === 'error'">
      <div class="text-error redis-result__mono">{{ cell(result.value) }}</div>
    </div>

    <!-- map：键值表 -->
    <div v-else-if="result.kind === 'map' && entries.length" class="redis-result__table-wrap">
      <v-table density="compact" class="redis-result__table">
        <tbody>
          <tr v-for="([k, v], i) in entries" :key="i">
            <td class="redis-result__mono text-medium-emphasis">{{ cell(k) }}</td>
            <td class="redis-result__mono">{{ cell(v) }}</td>
          </tr>
        </tbody>
      </v-table>
    </div>

    <!-- array：pairs 配对（hash 的 HGETALL 扁平数组 / zset 的 ZRANGE WITHSCORES），否则逐项列表 -->
    <div v-else-if="result.kind === 'array'" class="redis-result__block">
      <div v-if="pairs && pairsList.length" class="redis-result__table-wrap">
        <v-table density="compact" class="redis-result__table">
          <tbody>
            <tr v-for="([k, v], i) in pairsList" :key="i">
              <td class="redis-result__mono text-medium-emphasis">{{ cell(k) }}</td>
              <td class="redis-result__mono">{{ cell(v) }}</td>
            </tr>
          </tbody>
        </v-table>
      </div>
      <div v-else class="redis-result__list">
        <div v-for="(item, i) in arrItems" :key="i" class="redis-result__line">
          <span class="redis-result__mono">{{ cell(item) }}</span>
        </div>
      </div>
    </div>

    <!-- 未知 kind 兜底 -->
    <div v-else class="text-caption text-medium-emphasis">未知返回类型（{{ result.kind }}）</div>
  </div>
</template>

<script setup lang="ts">
/**
 * RedisResultView —— 按 RedisExecResult.kind 渲染执行结果
 *
 * kinds：status（绿色行）/ string / int / nil / blob / array / map / error（兜底）。
 * 后端 redis_exec 对 HGETALL / ZRANGE WITHSCORES 返回扁平 array（非 map kind），
 * 传入 pairs=true 时按相邻两项配对成键值表（hash=字段/值，zset=成员/分数）。
 */
import { computed } from 'vue'
import type { RedisExecResult } from '@/api/types'

const props = defineProps<{
  result: RedisExecResult
  /** 数组按相邻两项配对成键值表（hash 的 HGETALL / zset 的 ZRANGE WITHSCORES） */
  pairs?: boolean
}>()

/** 单元格文本：null/undefined 显示 (nil)，对象 JSON 序列化 */
function cell(v: unknown): string {
  if (v === null || v === undefined) return '(nil)'
  if (typeof v === 'object') {
    try {
      return JSON.stringify(v)
    } catch {
      return String(v)
    }
  }
  return String(v)
}

const arrItems = computed<unknown[]>(() =>
  Array.isArray(props.result.value) ? props.result.value : [],
)

const pairsList = computed<[unknown, unknown][]>(() => {
  const arr = arrItems.value
  const out: [unknown, unknown][] = []
  for (let i = 0; i + 1 < arr.length; i += 2) out.push([arr[i], arr[i + 1]])
  return out
})

const entries = computed<[unknown, unknown][]>(() => {
  const v = props.result.value
  if (v && typeof v === 'object' && !Array.isArray(v)) {
    return Object.entries(v as Record<string, unknown>)
  }
  return []
})

const blobText = computed(() => cell(props.result.value))
/** 过长 hex 片段截断，悬停看全文 */
const blobSnippet = computed(() => {
  const t = blobText.value
  return t.length > 120 ? `${t.slice(0, 120)}…` : t
})
</script>

<style scoped>
.redis-result__mono {
  font-family: var(--fy-mono);
}

.redis-result__status {
  display: flex;
  align-items: center;
}

.redis-result__code {
  font-family: var(--fy-mono);
  font-size: 12px;
  white-space: pre-wrap;
  word-break: break-all;
  padding: 4px 8px;
  background: rgba(var(--v-theme-on-surface), 0.04);
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
}

.redis-result__block {
  max-width: 100%;
}

.redis-result__nil {
  font-family: var(--fy-mono);
}

.redis-result__table-wrap {
  overflow: auto;
  max-height: 280px;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
}

/* 键值表：紧凑且不撑高，两列（字段=浅色 / 值=正文） */
.redis-result__table :deep(td) {
  padding: 2px 8px;
  font-size: 12px;
  word-break: break-all;
  white-space: pre-wrap;
}

.redis-result__list {
  max-height: 280px;
  overflow-y: auto;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
}

.redis-result__line {
  padding: 2px 8px;
  border-bottom: 1px dashed rgba(var(--v-theme-on-surface), 0.12);
  word-break: break-all;
  white-space: pre-wrap;
}

</style>
