<template>
  <v-dialog
    :model-value="modelValue"
    width="520"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <v-card class="edit-db">
      <!-- 顶部：标题 -->
      <div class="edit-db__header">
        <v-icon size="small" class="mr-1">mdi-pencil-outline</v-icon>
        <span class="edit-db__title">编辑数据库（{{ dbName }}）</span>
      </div>
      <v-divider />

      <v-alert v-if="error" type="error" variant="tonal" density="compact" class="mx-4 mt-3">
        {{ error }}
      </v-alert>

      <!-- 加载中 -->
      <div v-if="loading" class="edit-db__loading">
        <v-progress-circular indeterminate size="small" />
        <span class="text-caption text-medium-emphasis ml-2">正在加载数据库属性…</span>
      </div>

      <div v-else class="edit-db__body">
        <div class="fy-field-row">
          <span class="fy-field-row__label">字符集</span>
          <v-select
            v-model="charset"
            :items="charsetItems"
            density="compact"
            variant="outlined"
            single-line
            hide-details
            class="edit-db__select"
            @update:model-value="onCharsetChange"
          />
        </div>
        <div class="fy-field-row">
          <span class="fy-field-row__label">排序规则</span>
          <v-select
            v-model="collation"
            :items="collationItems"
            density="compact"
            variant="outlined"
            single-line
            hide-details
            class="edit-db__select"
          />
        </div>
      </div>

      <v-divider />
      <div class="edit-db__footer">
        <v-btn variant="text" @click="emit('update:modelValue', false)">取消</v-btn>
        <v-btn color="primary" :loading="saving" :disabled="loading" @click="save">保存</v-btn>
      </div>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
/**
 * EditDatabaseDialog —— 编辑数据库默认字符集/排序规则（右键菜单「编辑数据库...」入口）
 *
 * 打开时经 information_schema 加载当前字符集/排序规则与候选列表；
 * 保存走 mysqlDbEdit（ALTER DATABASE），成功后 emit saved 由父组件收尾。
 * 库名/字符集在 information_schema 查询中作字符串字面量（单引号翻倍转义）。
 */
import { ref, watch } from 'vue'
import { useUiStore } from '@/stores/ui'
import { mysqlDbEdit, mysqlQuery } from '@/api/mysql'
import { friendlyError as errText } from '@/utils/errors'

const props = defineProps<{
  modelValue: boolean
  connId: string
  dbName: string
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', v: boolean): void
  (e: 'saved'): void
}>()

const ui = useUiStore()

/** SQL 字符串字面量（单引号翻倍转义） */
function sqlStr(value: string): string {
  return `'${value.replace(/'/g, "''")}'`
}

const loading = ref(false)
const saving = ref(false)
const error = ref('')
const charset = ref('')
const collation = ref('')
const charsetItems = ref<string[]>([])
const collationItems = ref<string[]>([])

/** 查询并取首行指定列（information_schema 元数据查询共用） */
async function queryFirst(
  sql: string,
): Promise<Record<string, string | null> | null> {
  const result = await mysqlQuery(props.connId, sql, 1, 1)
  const row = result.rows[0]
  if (!row) return null
  const out: Record<string, string | null> = {}
  result.columns.forEach((col, i) => {
    out[col] = row[i] ?? null
  })
  return out
}

/** 打开时加载当前属性与候选列表 */
async function load(): Promise<void> {
  if (!props.connId || !props.dbName) return
  loading.value = true
  error.value = ''
  try {
    const cur = await queryFirst(
      `SELECT DEFAULT_CHARACTER_SET_NAME, DEFAULT_COLLATION_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = ${sqlStr(props.dbName)}`,
    )
    const csList: string[] = []
    const collResult = await mysqlQuery(
      props.connId,
      'SELECT DISTINCT CHARACTER_SET_NAME FROM information_schema.COLLATIONS ORDER BY CHARACTER_SET_NAME',
      1,
      10000,
    )
    collResult.rows.forEach((row) => {
      if (row[0]) csList.push(row[0])
    })
    charsetItems.value = csList
    charset.value = cur?.DEFAULT_CHARACTER_SET_NAME ?? csList[0] ?? 'utf8mb4'
    collationItems.value = await loadCollations(charset.value)
    collation.value = cur?.DEFAULT_COLLATION_NAME ?? collationItems.value[0] ?? ''
  } catch (e) {
    error.value = errText(e)
  } finally {
    loading.value = false
  }
}

/** 排序规则候选随字符集联动 */
async function loadCollations(cs: string): Promise<string[]> {
  const result = await mysqlQuery(
    props.connId,
    `SELECT COLLATION_NAME FROM information_schema.COLLATIONS WHERE CHARACTER_SET_NAME = ${sqlStr(cs)} ORDER BY COLLATION_NAME`,
    1,
    10000,
  )
  const out: string[] = []
  result.rows.forEach((row) => {
    if (row[0]) out.push(row[0])
  })
  return out
}

async function onCharsetChange(): Promise<void> {
  collationItems.value = await loadCollations(charset.value)
  if (!collationItems.value.includes(collation.value)) {
    collation.value = collationItems.value[0] ?? ''
  }
}

async function save(): Promise<void> {
  if (!props.connId || !props.dbName) return
  saving.value = true
  error.value = ''
  try {
    await mysqlDbEdit(props.connId, props.dbName, charset.value, collation.value || undefined)
    ui.toast(`已修改数据库「${props.dbName}」默认字符集为 ${charset.value}`, 'success')
    emit('saved')
    emit('update:modelValue', false)
  } catch (e) {
    error.value = errText(e)
  } finally {
    saving.value = false
  }
}

watch(
  () => props.modelValue,
  (open) => {
    if (open) void load()
  },
)
</script>

<style scoped>
.edit-db__header {
  display: flex;
  align-items: center;
  padding: 10px 16px;
}

.edit-db__title {
  font-size: 14px;
  min-width: 0;
  /* 长库名插值（如超长数据库名）不收敛会撑破 520px 卡片宽 */
  overflow-wrap: anywhere;
}

.edit-db__loading {
  display: flex;
  align-items: center;
  padding: 24px 16px;
}

.edit-db__body {
  padding: 12px 16px 16px;
}

.edit-db__select {
  flex: 0 0 260px;
  max-width: 260px;
}

.edit-db__footer {
  display: flex;
  justify-content: flex-end;
  gap: 0.5em;
  padding: 10px 16px;
}

</style>
