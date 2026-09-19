<template>
  <div class="redis-keys">
    <!-- 顶部筛选：pattern + 刷新 -->
    <div class="d-flex align-center px-2 py-1">
      <div class="fy-field-row">
        <span class="fy-field-row__label">过滤 key</span>
        <v-text-field
          v-model="pattern"
          placeholder="如 user:*，留空 = 全部"
          density="compact"
          single-line
          hide-details
          clearable
          prepend-inner-icon="mdi-magnify"
          class="mr-1"
          @keydown.enter="loadKeys"
          @click:clear="loadKeys"
        />
      </div>
      <v-btn
        icon="mdi-refresh"
        size="x-small"
        variant="text"
        :loading="store.keysLoading"
        title="刷新 Keys"
        @click="refreshKeys"
      />
    </div>
    <v-divider />

    <!-- key 列表 -->
    <div class="redis-keys__list">
      <v-list density="compact" nav>
        <v-list-item
          v-for="k in store.keys"
          :key="k"
          :active="k === selectedKey"
          @click="selectKey(k)"
        >
          <template #prepend>
            <v-icon size="small" color="primary">mdi-key-variant</v-icon>
          </template>
          <v-list-item-title class="text-body-2 redis-keys__mono">{{ k }}</v-list-item-title>
        </v-list-item>
        <v-list-item v-if="!store.keysLoading && store.keys.length === 0">
          <v-list-item-title class="text-caption text-medium-emphasis">没有匹配的 key</v-list-item-title>
        </v-list-item>
      </v-list>
    </div>
    <v-divider />

    <!-- 选中 key 详情：TYPE / TTL / 值 -->
    <div v-if="selectedKey" class="redis-keys__detail">
      <div class="redis-keys__detail-head">
        <v-icon size="small" color="primary" class="mr-1">mdi-key-variant</v-icon>
        <span class="text-body-2 redis-keys__mono mr-2">{{ selectedKey }}</span>
        <v-chip v-if="detailType" size="x-small" variant="tonal" color="primary" class="mr-1">
          {{ detailType }}
        </v-chip>
        <v-chip v-if="detailTtl !== null" size="x-small" variant="tonal" :color="ttlColor" class="mr-1">
          {{ ttlLabel }}
        </v-chip>
        <v-spacer />
        <v-btn
          size="x-small"
          variant="text"
          color="error"
          prepend-icon="mdi-delete-outline"
          :loading="deletingKey"
          @click="deleteKey"
        >
          删除
        </v-btn>
      </div>
      <div class="redis-keys__detail-body">
        <div v-if="detailLoading" class="text-caption text-medium-emphasis">加载值中…</div>
        <v-alert
          v-else-if="detailError"
          type="error"
          variant="tonal"
          density="compact"
          closable
          @click:close="detailError = ''"
        >
          {{ detailError }}
        </v-alert>
        <template v-else-if="detailResult">
          <RedisResultView :result="detailResult" :pairs="detailPairs" />
        </template>
        <div v-else-if="detailType && !detailLoading" class="text-caption text-medium-emphasis">
          {{ detailType === 'none' ? 'key 不存在' : '该类型暂不支持直接预览，可用命令执行条查看' }}
        </div>
      </div>
    </div>
    <div v-else class="redis-keys__detail-hint text-caption text-medium-emphasis">
      在左侧选择 key 查看 TYPE / TTL / 值
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * RedisKeysPanel —— Redis 工作台 Keys 浏览面板
 *
 * 结构：pattern 过滤行 / key 列表 / 选中 key 详情（TYPE·TTL·值）。
 * - 列表数据走 store.keys（契约），过滤用 store.pattern（契约 state）→ store.loadKeys()
 * - 详情读取一律用 store.exec 组装命令（TypeScript 契约）：
 *   type / ttl / get / hgetall（配对键值表）/ lrange / smembers / zrange withscores
 * - 删除 key：ui.confirm 强确认 → exec(['del', key]) → store.refreshKeys()
 * - DB 切换后 store.db 变化，watch 复位选中与详情（重新加载由工作台 selectDb 流程负责）
 */
import { computed, ref, watch } from 'vue'
import { useRedisStore } from '@/stores/redis'
import { useUiStore } from '@/stores/ui'
import type { RedisExecResult } from '@/api/types'
import RedisResultView from './RedisResultView.vue'

const store = useRedisStore()
const ui = useUiStore()

function errText(err: unknown): string {
  return typeof err === 'string' ? err : String(err)
}

/** pattern 双向绑定：直接读写 store.pattern（契约 state），loadKeys 以它过滤 */
const pattern = computed({
  get: () => store.pattern,
  set: (v: string) => {
    store.pattern = v
  },
})

const selectedKey = ref('')
const detailType = ref('')
const detailTtl = ref<number | null>(null)
const detailResult = ref<RedisExecResult | null>(null)
const detailPairs = ref(false)
const detailLoading = ref(false)
const detailError = ref('')
const deletingKey = ref(false)

/** TTL 语义：-1 无过期、-2 不存在、其余为剩余秒数（-2 文案与正文"key 不存在"口径一致） */
const ttlLabel = computed(() => {
  const t = detailTtl.value
  if (t === null) return ''
  if (t === -1) return 'TTL 永不过期'
  if (t === -2) return 'key 不存在'
  return `TTL ${t} 秒`
})

/** TTL chip 语义色：-1 success / -2 error / 临期（<60s）warning / 其余默认主题色 */
const ttlColor = computed((): string | undefined => {
  const t = detailTtl.value
  if (t === null) return undefined
  if (t === -1) return 'success'
  if (t === -2) return 'error'
  return t < 60 ? 'warning' : undefined
})

function resetDetail(): void {
  selectedKey.value = ''
  detailType.value = ''
  detailTtl.value = null
  detailResult.value = null
  detailPairs.value = false
  detailLoading.value = false
  detailError.value = ''
  deletingKey.value = false
}

/** DB 切换后复位选中与详情（工作台 selectDb 完成后刷新 keys） */
watch(
  () => store.db,
  () => {
    resetDetail()
  },
)

/** 过滤/回车：store.loadKeys()（内部以 pattern || '*' 调 redisKeys） */
async function loadKeys(): Promise<void> {
  await store.loadKeys()
}

/** 刷新按钮：store.refreshKeys() */
async function refreshKeys(): Promise<void> {
  await store.refreshKeys()
}

let detailSeq = 0
/** 选中 key：TYPE / TTL / 值 三段拉取（序列号防连点竞态），值按类型用 exec 组装命令 */
async function selectKey(key: string): Promise<void> {
  if (key === selectedKey.value) return
  const seq = ++detailSeq
  selectedKey.value = key
  detailType.value = ''
  detailTtl.value = null
  detailResult.value = null
  detailPairs.value = false
  detailError.value = ''
  detailLoading.value = true
  try {
    const typeRes = await store.exec(['type', key])
    if (seq !== detailSeq) return
    const type = typeRes.kind === 'string' ? String(typeRes.value) : ''
    detailType.value = type
    let res: RedisExecResult | null = null
    let pairs = false
    switch (type) {
      case 'string':
        res = await store.exec(['get', key])
        break
      case 'hash':
        // HGETALL 返回扁平 array：[field, value, field, value, …]，配对渲染键值表
        res = await store.exec(['hgetall', key])
        pairs = true
        break
      case 'list':
        res = await store.exec(['lrange', key, '0', '-1'])
        break
      case 'set':
        res = await store.exec(['smembers', key])
        break
      case 'zset':
        // ZRANGE ... WITHSCORES 返回扁平 array：[member, score, …]，配对渲染「成员 分数」
        res = await store.exec(['zrange', key, '0', '-1', 'withscores'])
        pairs = true
        break
      default:
        // none（已过期）/stream 等：置 null，下方展示占位提示
        res = null
    }
    // 值命令本身报错（如类型在两次命令间变化）时，转入错误提示而非结果渲染
    if (res && res.kind === 'error') {
      detailError.value = String(res.value)
      res = null
    }
    const ttlRes = await store.exec(['ttl', key])
    if (seq !== detailSeq) return
    detailTtl.value = ttlRes.kind === 'int' ? Number(ttlRes.value) : null
    if (!detailError.value) {
      detailPairs.value = pairs
      detailResult.value = res
    }
  } catch (err) {
    if (seq !== detailSeq) return
    detailError.value = errText(err)
  } finally {
    if (seq === detailSeq) detailLoading.value = false
  }
}

/** 删除选中 key：ui.confirm（danger）→ exec(['del', key]) → refreshKeys */
async function deleteKey(): Promise<void> {
  const key = selectedKey.value
  if (!key) return
  const ok = await ui.confirm({
    title: '删除 Key',
    message: `确定删除 key「${key}」吗？该操作不可恢复。`,
    confirmText: '删除',
    danger: true,
  })
  if (!ok) return
  deletingKey.value = true
  try {
    await store.exec(['del', key])
    ui.toast(`key「${key}」已删除`, 'success')
    resetDetail()
    await store.refreshKeys()
  } catch (err) {
    ui.toast(errText(err), 'error')
  } finally {
    deletingKey.value = false
  }
}
</script>

<style scoped>
.redis-keys {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.redis-keys__mono {
  font-family: var(--fy-mono);
}

.redis-keys__list {
  flex: 1 1 auto;
  overflow-y: auto;
  min-height: 80px;
}

/* 详情区：头部固定，值区独立滚动（超高时压缩列表高度） */
.redis-keys__detail {
  flex: 0 1 45%;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.redis-keys__detail-head {
  display: flex;
  align-items: center;
  padding: 6px 12px 4px;
  flex: 0 0 auto;
  min-width: 0;
}

.redis-keys__detail-body {
  flex: 1 1 auto;
  overflow-y: auto;
  min-height: 0;
  padding: 4px 12px 8px;
}

.redis-keys__detail-hint {
  flex: 0 0 auto;
  padding: 8px 12px;
  text-align: center;
}
</style>
