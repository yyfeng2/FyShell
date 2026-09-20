<template>
  <v-dialog
    :model-value="modelValue"
    width="860"
    scrollable
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <v-card class="find-db">
      <!-- 顶部：标题 -->
      <div class="find-db__header">
        <v-icon size="small" class="mr-1">mdi-magnify</v-icon>
        <span class="find-db__title">在数据库中查找（{{ dbName }}）</span>
      </div>
      <v-divider />

      <!-- 搜索条：关键字输入 + 查找按钮 -->
      <div class="find-db__bar">
        <v-text-field
          v-model="keyword"
          density="compact"
          variant="outlined"
          single-line
          hide-details
          clearable
          placeholder="输入要查找的文本（在全部表的字符串列中搜索）"
          class="find-db__input"
          @keyup.enter="doFind"
        />
        <v-btn
          color="primary"
          variant="tonal"
          prepend-icon="mdi-magnify"
          :loading="loading"
          :disabled="!keyword?.trim()"
          @click="doFind"
        >
          查找
        </v-btn>
      </div>

      <v-alert v-if="error" type="error" variant="tonal" density="compact" class="mx-4 mt-2">
        {{ error }}
      </v-alert>

      <v-divider />

      <!-- 结果区：按表分组的命中行 -->
      <div class="find-db__body">
        <template v-if="hits.length">
          <div class="text-caption text-medium-emphasis mb-2">
            {{ hits.length }} 张表命中（每表最多显示 {{ maxPerTable }} 行）
          </div>
          <div v-for="hit in hits" :key="hit.table" class="find-db__group">
            <div class="find-db__table">
              <v-icon size="x-small" color="primary" class="mr-1">mdi-table</v-icon>
              <span class="text-body-2">{{ hit.table }}</span>
              <span class="text-caption text-medium-emphasis ml-2">{{ hit.rows.length }} 行</span>
            </div>
            <table class="find-db__grid">
              <thead>
                <tr>
                  <th v-for="col in hit.columns" :key="col">{{ col }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="(row, ri) in hit.rows" :key="ri">
                  <td v-for="(cell, ci) in row" :key="ci">
                    {{ cell === null ? 'NULL' : cell }}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </template>
        <div v-else-if="searched" class="text-caption text-medium-emphasis pa-4">
          未找到匹配的记录
        </div>
        <div v-else class="text-caption text-medium-emphasis pa-4">
          输入关键字后点击「查找」，将在数据库全部表的字符串列中搜索。
        </div>
      </div>

      <v-divider />
      <div class="find-db__footer">
        <v-btn variant="text" @click="emit('update:modelValue', false)">关闭</v-btn>
      </div>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
/**
 * FindInDbDialog —— 在数据库中查找（右键菜单「在数据库中查找」入口）
 *
 * 关键字经 mysqlDbFind 在全部表的字符串列中 LIKE 搜索（后端逐表执行、
 * 每表限量），结果按表分组展示。搜索范围固定为右键的库，与连接默认库无关。
 */
import { ref } from 'vue'
import { mysqlDbFind } from '@/api/mysql'
import type { MySqlDbFindHit } from '@/api/types'

const props = defineProps<{
  modelValue: boolean
  connId: string
  dbName: string
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', v: boolean): void
}>()

function errText(err: unknown): string {
  return typeof err === 'string' ? err : String(err)
}

const maxPerTable = 20
const keyword = ref<string>('')
const loading = ref(false)
const error = ref('')
const hits = ref<MySqlDbFindHit[]>([])
const searched = ref(false)

async function doFind(): Promise<void> {
  const kw = keyword.value?.trim()
  if (!props.connId || !props.dbName || !kw) return
  loading.value = true
  error.value = ''
  try {
    hits.value = await mysqlDbFind(props.connId, props.dbName, kw, maxPerTable)
    searched.value = true
  } catch (e) {
    error.value = errText(e)
  } finally {
    loading.value = false
  }
}
</script>

<style scoped>
.find-db__header {
  display: flex;
  align-items: center;
  padding: 10px 16px;
}

.find-db__title {
  font-size: 16px;
}

.find-db__bar {
  display: flex;
  align-items: center;
  gap: 0.5em;
  padding: 8px 16px;
}

.find-db__input {
  flex: 1 1 auto;
}

.find-db__body {
  min-height: 240px;
}

.find-db__group {
  margin-bottom: 14px;
}

.find-db__table {
  display: flex;
  align-items: center;
  margin-bottom: 4px;
}

/* 命中行网格：紧凑边框表 */
.find-db__grid {
  width: 100%;
  border-collapse: collapse;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  font-size: 12px;
}

.find-db__grid th,
.find-db__grid td {
  border: 1px solid rgba(var(--v-theme-on-surface), 0.08);
  padding: 3px 8px;
  text-align: left;
}

.find-db__grid th {
  background: rgba(var(--v-theme-on-surface), 0.04);
  font-weight: 500;
}

.find-db__footer {
  display: flex;
  justify-content: flex-end;
  gap: 0.5em;
  padding: 10px 16px;
}
</style>
