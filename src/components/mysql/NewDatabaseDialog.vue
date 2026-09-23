<template>
  <v-dialog
    :model-value="modelValue"
    width="520"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <v-card class="new-db">
      <!-- 顶部：标题 + 关闭 -->
      <div class="new-db__header">
        <v-icon size="small" class="mr-1">mdi-database-plus-outline</v-icon>
        <span class="new-db__title">新建数据库</span>
        <v-spacer />
        <v-btn
          icon="mdi-close"
          size="x-small"
          variant="text"
          title="关闭"
          @click="emit('update:modelValue', false)"
        />
      </div>
      <v-divider />

      <!-- 常规分区（Navicat 同款：数据库名称/字符集/排序规则） -->
      <div class="new-db__body">
        <div class="new-db__tab">常规</div>
        <div class="fy-field-row">
          <span class="fy-field-row__label">数据库名称</span>
          <v-text-field
            v-model="name"
            density="compact"
            variant="outlined"
            single-line
            hide-details
            autofocus
            class="new-db__input"
            @keyup.enter="save"
          />
        </div>
        <div class="fy-field-row">
          <span class="fy-field-row__label">字符集</span>
          <v-select
            v-model="charset"
            :items="charsetItems"
            density="compact"
            variant="outlined"
            single-line
            hide-details
            class="new-db__select"
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
            class="new-db__select"
          />
        </div>
      </div>

      <v-divider />
      <div class="new-db__footer">
        <v-btn variant="text" @click="emit('update:modelValue', false)">取消</v-btn>
        <v-btn color="primary" :loading="saving" :disabled="!name?.trim()" @click="save">确定</v-btn>
      </div>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
/**
 * NewDatabaseDialog —— 新建数据库（右键菜单「新建数据库...」入口）
 *
 * 常规分区：数据库名称 + 字符集/排序规则候选（information_schema 读服务器默认值，
 * 排序规则随字符集联动）；确认走 mysqlDbCreate（CREATE DATABASE 带库默认字符集/排序规则），
 * 成功后 emit saved 由父组件收尾。
 */
import { ref, watch } from 'vue'
import { useUiStore } from '@/stores/ui'
import { mysqlDbCreate } from '@/api/mysqlDb'
import { mysqlQuery } from '@/api/mysql'
import { friendlyError as errText } from '@/utils/errors'

const props = defineProps<{
  modelValue: boolean
  connId: string
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

const saving = ref(false)
const name = ref('')
const charset = ref('')
const collation = ref('')
const charsetItems = ref<string[]>([])
const collationItems = ref<string[]>([])

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

/** 打开时加载服务器默认字符集/排序规则与候选列表 */
async function load(): Promise<void> {
  if (!props.connId) return
  name.value = ''
  try {
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
    // 默认取服务器默认字符集/排序规则
    let serverCharset = 'utf8mb4'
    let serverCollation = ''
    const defResult = await mysqlQuery(
      props.connId,
      'SELECT @@character_set_server, @@collation_server',
      1,
      1,
    )
    const row = defResult.rows[0]
    if (row) {
      serverCharset = row[0] ?? serverCharset
      serverCollation = row[1] ?? ''
    }
    charset.value = csList.includes(serverCharset) ? serverCharset : (csList[0] ?? 'utf8mb4')
    collationItems.value = await loadCollations(charset.value)
    collation.value = collationItems.value.includes(serverCollation)
      ? serverCollation
      : (collationItems.value[0] ?? '')
  } catch (e) {
    ui.toast(errText(e), 'error')
  }
}

async function save(): Promise<void> {
  const dbName = name.value?.trim()
  if (!props.connId || !dbName) return
  saving.value = true
  try {
    await mysqlDbCreate(props.connId, dbName, charset.value, collation.value || undefined)
    ui.toast(`数据库「${dbName}」已创建`, 'success')
    emit('saved')
    emit('update:modelValue', false)
  } catch (e) {
    ui.toast(errText(e), 'error')
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
.new-db__header {
  display: flex;
  align-items: center;
  padding: 10px 16px;
}

.new-db__title {
  font-size: 12px;
}

.new-db__body {
  padding: 12px 16px 16px;
}

/* 常规分区标记（Navicat 同款 Tab 条） */
.new-db__tab {
  display: inline-block;
  font-size: 12px;
  font-weight: 500;
  padding: 4px 14px;
  margin-bottom: 10px;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-bottom-color: transparent;
  border-radius: 4px 4px 0 0;
  background: rgba(var(--v-theme-on-surface), 0.04);
}

.new-db__input {
  flex: 0 0 260px;
  max-width: 260px;
}

.new-db__select {
  flex: 0 0 260px;
  max-width: 260px;
}

.new-db__footer {
  display: flex;
  justify-content: flex-end;
  gap: 0.5em;
  padding: 10px 16px;
}

/* 底部按钮：字号 14px（与全局主体档一致）+ 边框到文字上下 0.3em、左右 0.5em */
.new-db__footer .v-btn {
  --v-btn-size: 14px;
  --v-btn-height: auto;
  height: auto;
  padding: 0.3em 0.5em;
}

</style>
