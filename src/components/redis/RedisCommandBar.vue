<template>
  <div class="redis-cmdbar">
    <div class="d-flex align-center px-2 py-1">
      <div class="fy-field-row">
        <span class="fy-field-row__label">Redis 命令</span>
        <v-text-field
          v-model="input"
          placeholder="如 GET key / SET k v / HGETALL h"
          density="compact"
          single-line
          hide-details
          prepend-inner-icon="mdi-console-line"
          class="mr-1"
          :disabled="cmdRunning"
          @keydown.enter="submitCmd"
        />
      </div>
      <v-btn
        icon="mdi-play"
        size="small"
        variant="tonal"
        color="primary"
        title="执行（Enter）"
        :loading="cmdRunning"
        :disabled="!input.trim()"
        @click="submitCmd"
      />
      <v-btn
        v-if="history.length"
        icon="mdi-delete-outline"
        size="small"
        variant="text"
        title="清空执行历史"
        class="ml-1"
        @click="history = []"
      />
    </div>
    <v-divider />
    <div class="redis-cmdbar__body">
      <!-- 结果空态：尚未执行任何命令（升级：现代空态组件） -->
      <EmptyState
        v-if="history.length === 0"
        size="compact"
        icon="mdi-chevron-right"
        title="执行结果将显示在这里"
        desc="输入 Redis 命令后按 Enter 执行"
      />
      <div v-for="h in history" :key="h.id" class="redis-cmdbar__item">
        <div class="redis-cmdbar__cmd">
          <span class="text-caption text-medium-emphasis mr-1">›</span>
          <span class="redis-cmdbar__raw redis-cmdbar__mono">{{ h.raw }}</span>
          <v-spacer />
          <v-btn
            icon="mdi-close"
            size="x-small"
            variant="text"
            title="移除该条"
            @click="removeItem(h.id)"
          />
        </div>
        <div class="pa-2 pt-0">
          <v-alert v-if="h.error" type="error" variant="tonal" density="compact">
            {{ h.error }}
          </v-alert>
          <RedisResultView v-else-if="h.result" :result="h.result" />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * RedisCommandBar —— 工作台底部命令执行条
 *
 * - 单命令输入 + Enter/按钮执行，store.exec(args) 走后端 redis_exec
 * - 简单分词器：空格分隔，支持单引号/双引号包裹的含空格参数（如 SET k "hello world"）
 * - 命令名与参数照原样透传后端（Redis 命令名大小写不敏感，后端 regquery 无需补全）
 * - 结果按 kind 渲染（RedisResultView），kind==='error' 呈现为 v-alert
 * - 保留最近 HISTORY_LIMIT 条执行历史（仅内存，不持久化）+ 清空/逐条移除
 */
import { ref } from 'vue'
import { useRedisStore } from '@/stores/redis'
import type { RedisExecResult } from '@/api/types'
import RedisResultView from './RedisResultView.vue'
import EmptyState from '@/components/common/EmptyState.vue'
import { friendlyError as errText } from '@/utils/errors'

const store = useRedisStore()

const HISTORY_LIMIT = 30

interface CmdHistoryItem {
  id: number
  raw: string
  result: RedisExecResult | null
  error: string
}

const input = ref('')
const cmdRunning = ref(false)
const history = ref<CmdHistoryItem[]>([])
let historySeq = 0

/**
 * 分词器：`"([^"]*)"` 双引号 / `'([^']*)'` 单引号（均允许含空格与空串），
 * 其余按空白分 token。命令名与参数原样保留（不强制大写，Redis 大小写不敏感）。
 */
function tokenize(line: string): string[] {
  const args: string[] = []
  const re = /"([^"]*)"|'([^']*)'|([^\s]+)/g
  let m: RegExpExecArray | null
  while ((m = re.exec(line))) {
    args.push(m[1] ?? m[2] ?? m[3])
  }
  return args
}

function removeItem(id: number): void {
  history.value = history.value.filter((h) => h.id !== id)
}

async function submitCmd(): Promise<void> {
  const raw = input.value.trim()
  if (!raw || cmdRunning.value) return
  const entry: CmdHistoryItem = { id: ++historySeq, raw, result: null, error: '' }
  history.value.unshift(entry)
  if (history.value.length > HISTORY_LIMIT) history.value.pop()
  cmdRunning.value = true
  try {
    const result = await store.exec(tokenize(raw))
    const target = history.value.find((h) => h.id === entry.id)
    if (target) {
      target.result = result
      // 后端对 ERR 类回复返回 kind='error'（正常 resolve），这里转成 v-alert 展示
      target.error = result.kind === 'error' ? String(result.value) : ''
    }
  } catch (err) {
    const target = history.value.find((h) => h.id === entry.id)
    if (target) target.error = errText(err)
  } finally {
    cmdRunning.value = false
  }
}
</script>

<style scoped>
.redis-cmdbar {
  display: flex;
  flex-direction: column;
  min-height: 0;
  /* 高度交由父级 .redis-ws__cmdbar 的 flex: 0 1 220px 控制，此处不重复声明
     （父子同时声明 flex 会让胜者取决于 CSS import 顺序，覆盖设计意图） */
}

/* 历史区：超高滚动（最多撑到工作台 40% 高度，压缩 keys 面板） */
.redis-cmdbar__body {
  flex: 1 1 auto;
  overflow-y: auto;
  min-height: 0;
}

.redis-cmdbar__item {
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.12);
}

.redis-cmdbar__cmd {
  display: flex;
  align-items: center;
  padding: 4px 8px 0;
  min-width: 0;
}

.redis-cmdbar__raw {
  word-break: break-all;
  white-space: pre-wrap;
  font-size: 12px;
  min-width: 0;
}

.redis-cmdbar__mono {
  font-family: var(--fy-mono);
}

</style>
