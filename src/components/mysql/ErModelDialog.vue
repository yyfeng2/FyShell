<template>
  <v-dialog
    :model-value="modelValue"
    width="95%"
    scrollable
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <v-card class="er-model">
      <!-- 顶部：标题 + 关闭 -->
      <div class="er-model__header">
        <v-icon size="small" class="mr-1">mdi-file-tree-outline</v-icon>
        <span class="er-model__title">{{ tableName ? `表模型（${tableName}）` : `逆向数据库到模型（${dbName}）` }}</span>
        <v-spacer />
        <v-btn
          size="small"
          variant="text"
          prepend-icon="mdi-refresh"
          :loading="loading"
          @click="load"
        >
          重新生成
        </v-btn>
      </div>
      <v-divider />

      <v-alert v-if="error" type="error" variant="tonal" density="compact" class="mx-4 mt-3">
        {{ error }}
      </v-alert>

      <!-- 结果区：mermaid 渲染的 ER 图（可缩放滚动） -->
      <div class="er-model__body">
        <div v-if="loading" class="er-model__loading">
          <v-progress-circular indeterminate size="small" />
          <span class="text-caption text-medium-emphasis ml-2">正在读取表结构并生成模型…</span>
        </div>
        <div v-else class="er-model__svg" v-html="svg"></div>
      </div>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
/**
 * ErModelDialog —— 逆向数据库到模型（右键菜单「逆向数据库到模型...」入口）
 *
 * 经 mysqlQuery 读 information_schema.COLUMNS（列/主键）与 KEY_COLUMN_USAGE（外键），
 * 拼 mermaid erDiagram 定义渲染为 SVG。搜索范围固定为右键的库。
 * theme 暂固定 neutral（应用主题联动后续再做）；列/表名非单词字符替换为下划线。
 */
import { ref, watch } from 'vue'
import mermaid from 'mermaid'
import { mysqlQuery } from '@/api/mysql'

const props = defineProps<{
  modelValue: boolean
  connId: string
  dbName: string
  /** 可选：单表模式（表右键「逆向表到模型...」），只渲染该表及其直接外键关系 */
  tableName?: string
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', v: boolean): void
}>()

function errText(err: unknown): string {
  return typeof err === 'string' ? err : String(err)
}

/** SQL 字符串字面量（单引号翻倍转义） */
function sqlStr(value: string): string {
  return `'${value.replace(/'/g, "''")}'`
}

/** mermaid 标识符净化：非单词字符（含点/反引号等）替换为下划线 */
function sanitize(name: string): string {
  return name.replace(/\W/g, '_')
}

/** mermaid 属性类型净化：取类型基础词（varchar(255) → varchar），替换非单词字符 */
function typeOf(columnType: string): string {
  return sanitize(columnType.replace(/\(.*\)$/, ''))
}

mermaid.initialize({ startOnLoad: false, theme: 'neutral' })

const loading = ref(false)
const error = ref('')
const svg = ref('')

/** 打开时加载表结构并渲染 ER 图 */
async function load(): Promise<void> {
  if (!props.connId || !props.dbName) return
  loading.value = true
  error.value = ''
  try {
    // 单表模式（表右键入口）：列只取该表，外键取该表参与的直接关系
    const tableFilter = props.tableName ? ` AND TABLE_NAME = ${sqlStr(props.tableName)}` : ''
    // 列/主键：TABLE_NAME/COLUMN_NAME/COLUMN_TYPE/COLUMN_KEY，按表/列序
    const colResult = await mysqlQuery(
      props.connId,
      `SELECT TABLE_NAME, COLUMN_NAME, COLUMN_TYPE, COLUMN_KEY FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = ${sqlStr(props.dbName)}${tableFilter} ORDER BY TABLE_NAME, ORDINAL_POSITION`,
      1,
      100000,
    )
    // 外键：子表/子列/父表/父列（单表模式取该表作为子表或父表的关系）
    const fkFilter = props.tableName
      ? ` AND (TABLE_NAME = ${sqlStr(props.tableName)} OR REFERENCED_TABLE_NAME = ${sqlStr(props.tableName)})`
      : ''
    const fkResult = await mysqlQuery(
      props.connId,
      `SELECT TABLE_NAME, COLUMN_NAME, REFERENCED_TABLE_NAME, REFERENCED_COLUMN_NAME FROM information_schema.KEY_COLUMN_USAGE WHERE TABLE_SCHEMA = ${sqlStr(props.dbName)} AND REFERENCED_TABLE_NAME IS NOT NULL${fkFilter}`,
      1,
      100000,
    )

    // 实体块：每表 { type name [PK/FK] }
    const entities: string[] = []
    let currentTable = ''
    const byTable = new Map<string, string[]>()
    colResult.rows.forEach((row) => {
      const table = row[0] ?? ''
      const col = row[1] ?? ''
      const ctype = row[2] ?? ''
      const key = row[3] ?? ''
      if (!table || !col) return
      if (!byTable.has(table)) {
        byTable.set(table, [])
        currentTable = table
      }
      const marks = key === 'PRI' ? ' PK' : key === 'UNI' ? ' UK' : ''
      const line = `${typeOf(ctype)} ${sanitize(col)}${marks}`
      if (currentTable === table) {
        byTable.get(table)!.push(line)
      }
    })
    byTable.forEach((lines, table) => {
      entities.push(`  ${sanitize(table)} {\n${lines.map((l) => `    ${l}`).join('\n')}\n  }`)
    })

    // 关系线：父表 ||--o{ 子表（FK 多对一），去重
    const relSet = new Set<string>()
    const rels: string[] = []
    // 单表模式下：列查询只取了目标表，直接关联的父/子表以裸实体（无属性块）参与连线
    const relatedTables = new Set<string>()
    fkResult.rows.forEach((row) => {
      const childTable = row[0] ?? ''
      const parentTable = row[2] ?? ''
      const child = sanitize(childTable)
      const fkCol = sanitize(row[1] ?? '')
      const parent = sanitize(parentTable)
      if (!(parent && child)) return
      if (!byTable.has(childTable)) relatedTables.add(childTable)
      if (!byTable.has(parentTable)) relatedTables.add(parentTable)
      const key = `${parent}|${child}|${fkCol}`
      if (relSet.has(key)) return
      relSet.add(key)
      rels.push(`  ${parent} ||--o{ ${child} : "${fkCol}"`)
    })
    relatedTables.forEach((table) => {
      entities.push(`  ${sanitize(table)}`)
    })

    if (entities.length === 0) {
      error.value = props.tableName
        ? '该表没有可渲染的列'
        : '该库中没有可渲染的表'
      svg.value = ''
      return
    }

    const definition = `erDiagram\n${entities.join('\n')}${rels.length ? '\n' + rels.join('\n') : ''}`
    const rendered = await mermaid.render(`er-${Date.now()}`, definition)
    svg.value = rendered.svg
  } catch (e) {
    error.value = errText(e)
    svg.value = ''
  } finally {
    loading.value = false
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
.er-model__header {
  display: flex;
  align-items: center;
  padding: 10px 16px;
}

.er-model__title {
  font-size: 16px;
}

.er-model__body {
  height: calc(100vh - 160px);
  overflow: auto;
}

.er-model__loading {
  display: flex;
  align-items: center;
  padding: 24px 16px;
}

/* mermaid SVG：留白居中，容器可缩放滚动 */
.er-model__svg {
  padding: 16px;
  min-height: 200px;
}

.er-model__svg :deep(svg) {
  max-width: 100%;
  height: auto;
}

/* 标题栏按钮继承标题 16px，统一压回主体档 14px */
.er-model__header :deep(.v-btn) {
  font-size: 14px;
}

/* rem 换算非整数档修复：text-caption 10.5px → 12px 整数档 */
.text-caption,
.text-subtitle-2,
.text-body-2 {
  font-size: 12px !important;
}
</style>
