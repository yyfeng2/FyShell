<template>
  <v-dialog
    :model-value="modelValue"
    persistent
    width="960"
    @update:model-value="(v: boolean) => { if (!v) emit('update:modelValue', false) }"
  >
    <v-card class="table-designer" flat>
      <!-- 标题栏：新建表 / 设计表名 -->
      <v-card-title class="table-designer__title d-flex align-center">
        <v-icon size="small" class="mr-2">mdi-table-edit</v-icon>
        {{ isNew ? '新建表' : `设计表：${table}` }}
        <v-spacer />
        <v-btn
          size="small"
          variant="text"
          prepend-icon="mdi-refresh"
          :loading="loading"
          :disabled="isNew"
          title="重新加载表结构"
          @click="loadDesign"
        >
          刷新
        </v-btn>
      </v-card-title>
      <v-divider />

      <!-- 主体：编辑区（Tab 切换）+ 底部 DDL 预览 -->
      <div class="table-designer__body">
        <v-tabs v-model="activeTab" density="compact" color="primary">
          <v-tab value="columns" class="text-none">字段</v-tab>
          <v-tab value="indexes" class="text-none">索引</v-tab>
          <v-tab value="foreignKeys" class="text-none">外键</v-tab>
          <v-tab value="props" class="text-none">表属性</v-tab>
        </v-tabs>
        <v-divider />
        <div class="table-designer__edit-area">
          <!-- ================= 字段区 ================= -->
          <template v-if="activeTab === 'columns'">
            <div class="table-designer__toolbar">
              <v-btn size="small" variant="outlined" prepend-icon="mdi-plus" @click="addColumn">添加字段</v-btn>
              <span class="text-caption text-medium-emphasis ml-2">共 {{ columnRows.length }} 个字段</span>
            </div>
            <div class="table-designer__scroll">
              <div class="table-designer__head table-designer__head--cols">
                <span>顺序</span>
                <span>字段名</span>
                <span>类型</span>
                <span>NOT NULL</span>
                <span>默认值</span>
                <span>注释</span>
                <span>附加（AUTO_INCREMENT 等）</span>
                <span>操作</span>
              </div>
              <div v-for="(row, idx) in columnRows" :key="idx" class="table-designer__col-row">
                <span class="table-designer__order">{{ idx + 1 }}</span>
                <v-text-field
                  v-model="row.name"
                  density="compact"
                  variant="outlined"
                  hide-details
                  placeholder="column_name"
                />
                <v-combobox
                  v-model="row.data_type"
                  :items="DATA_TYPE_SUGGESTIONS"
                  density="compact"
                  variant="outlined"
                  hide-details
                  :persistent-hint="false"
                />
                <v-checkbox
                  v-model="row.is_nullable"
                  density="compact"
                  hide-details
                  class="table-designer__check"
                />
                <v-text-field
                  v-model="row.default_value"
                  density="compact"
                  variant="outlined"
                  hide-details
                  placeholder="NULL"
                />
                <v-text-field
                  v-model="row.comment"
                  density="compact"
                  variant="outlined"
                  hide-details
                  placeholder="字段注释"
                />
                <v-text-field
                  v-model="row.extra"
                  density="compact"
                  variant="outlined"
                  hide-details
                  placeholder="AUTO_INCREMENT"
                />
                <span class="table-designer__row-actions">
                  <v-btn
                    icon="mdi-arrow-up"
                    size="x-small"
                    variant="text"
                    :disabled="idx === 0"
                    title="上移"
                    @click="moveColumn(idx, -1)"
                  />
                  <v-btn
                    icon="mdi-arrow-down"
                    size="x-small"
                    variant="text"
                    :disabled="idx === columnRows.length - 1"
                    @click="moveColumn(idx, 1)"
                  />
                  <v-btn
                    icon="mdi-delete-outline"
                    size="x-small"
                    variant="text"
                    color="error"
                    title="删除字段"
                    @click="removeColumn(idx)"
                  />
                </span>
              </div>
              <div v-if="columnRows.length === 0" class="table-designer__empty">
                暂无字段，点击上方「添加字段」开始设计
              </div>
            </div>
          </template>

          <!-- ================= 索引区 ================= -->
          <template v-else-if="activeTab === 'indexes'">
            <div class="table-designer__toolbar">
              <v-btn size="small" variant="outlined" prepend-icon="mdi-plus" @click="addIndex">添加索引</v-btn>
              <span class="text-caption text-medium-emphasis ml-2">共 {{ indexRows.length }} 个索引</span>
            </div>
            <div class="table-designer__scroll">
              <div class="table-designer__head table-designer__head--idx">
                <span>索引名</span>
                <span>列（逗号分隔）</span>
                <span>唯一</span>
                <span>主键</span>
                <span>类型</span>
                <span>操作</span>
              </div>
              <div v-for="(row, idx) in indexRows" :key="idx" class="table-designer__col-row table-designer__col-row--index">
                <v-text-field
                  v-model="row.name"
                  density="compact"
                  variant="outlined"
                  hide-details
                  placeholder="idx_name"
                  :disabled="row.is_primary"
                />
                <v-text-field
                  v-model="row.columns_text"
                  density="compact"
                  variant="outlined"
                  hide-details
                  placeholder="col1, col2"
                />
                <v-checkbox v-model="row.is_unique" density="compact" hide-details class="table-designer__check" />
                <v-checkbox v-model="row.is_primary" density="compact" hide-details class="table-designer__check" />
                <v-text-field
                  v-model="row.index_type"
                  density="compact"
                  variant="outlined"
                  hide-details
                  placeholder="BTREE"
                />
                <span class="table-designer__row-actions">
                  <v-btn
                    icon="mdi-delete-outline"
                    size="x-small"
                    variant="text"
                    color="error"
                    title="删除索引"
                    @click="removeIndex(idx)"
                  />
                </span>
              </div>
              <div v-if="indexRows.length === 0" class="table-designer__empty">暂无索引</div>
            </div>
          </template>

          <!-- ================= 外键区 ================= -->
          <template v-else-if="activeTab === 'foreignKeys'">
            <div class="table-designer__toolbar">
              <v-btn size="small" variant="outlined" prepend-icon="mdi-plus" @click="addForeignKey">添加外键</v-btn>
              <span class="text-caption text-medium-emphasis ml-2">共 {{ foreignKeyRows.length }} 个外键</span>
            </div>
            <div class="table-designer__scroll">
              <div class="table-designer__head table-designer__head--foreign">
                <span>外键名</span>
                <span>本表列</span>
                <span>参照表</span>
                <span>参照列</span>
                <span>ON DELETE</span>
                <span>ON UPDATE</span>
                <span>操作</span>
              </div>
              <div v-for="(row, idx) in foreignKeyRows" :key="idx" class="table-designer__col-row--foreign">
                <v-text-field
                  v-model="row.name"
                  density="compact"
                  variant="outlined"
                  hide-details
                  placeholder="fk_name"
                />
                <v-text-field
                  v-model="row.columns_text"
                  density="compact"
                  variant="outlined"
                  hide-details
                  placeholder="col1, col2"
                />
                <v-text-field
                  v-model="row.ref_table"
                  density="compact"
                  variant="outlined"
                  hide-details
                  placeholder="ref_table"
                />
                <v-text-field
                  v-model="row.ref_columns_text"
                  density="compact"
                  variant="outlined"
                  hide-details
                  placeholder="ref col1, ref col2"
                />
                <v-select
                  v-model="row.on_delete"
                  :items="FK_ACTIONS"
                  density="compact"
                  variant="outlined"
                  hide-details
                />
                <v-select
                  v-model="row.on_update"
                  :items="FK_ACTIONS"
                  density="compact"
                  variant="outlined"
                  hide-details
                />
                <span class="table-designer__row-actions">
                  <v-btn
                    icon="mdi-delete-outline"
                    size="x-small"
                    variant="text"
                    color="error"
                    title="删除外键"
                    @click="removeForeignKey(idx)"
                  />
                </span>
              </div>
              <div v-if="foreignKeyRows.length === 0" class="table-designer__empty">暂无外键</div>
            </div>
          </template>

          <!-- ================= 表属性区 ================= -->
          <template v-else>
            <div class="table-designer__scroll table-designer__props">
              <v-row dense>
                <v-col cols="4">
                  <div class="fy-field-row">
                    <span class="fy-field-row__label">表名</span>
                    <v-text-field
                      v-model="tableName"
                      density="compact"
                      variant="outlined"
                      :disabled="!isNew"
                      hint="仅新建表时可编辑"
                    />
                  </div>
                </v-col>
                <v-col cols="4">
                  <div class="fy-field-row">
                    <span class="fy-field-row__label">存储引擎</span>
                    <v-combobox
                      v-model="engine"
                      :items="ENGINE_SUGGESTIONS"
                      density="compact"
                      variant="outlined"
                      :disabled="!isNew"
                      hint="仅新建表时可编辑"
                    />
                  </div>
                </v-col>
                <v-col cols="4">
                  <div class="fy-field-row">
                    <span class="fy-field-row__label">字符集</span>
                    <v-combobox
                      v-model="charset"
                      :items="CHARSET_SUGGESTIONS"
                      density="compact"
                      variant="outlined"
                      :disabled="!isNew"
                      hint="仅新建表时可编辑"
                    />
                  </div>
                </v-col>
                <v-col cols="12">
                  <div class="fy-field-row">
                    <span class="fy-field-row__label">表注释</span>
                    <v-text-field
                      v-model="comment"
                      density="compact"
                      variant="outlined"
                      :disabled="!isNew"
                    />
                  </div>
                </v-col>
              </v-row>
              <v-alert v-if="!isNew" type="info" variant="tonal" density="compact" class="mt-2">
                已有表的引擎/字符集/注释由后端在 DDL 中保留，如需修改请通过 SQL 执行。
              </v-alert>
            </div>
          </template>
        </div>

        <!-- 底部 DDL 预览面板 -->
        <v-divider />
        <div class="table-designer__ddl">
          <div class="d-flex align-center mb-1">
            <span class="text-body-2 mr-2">DDL 预览</span>
            <v-chip v-if="savedDdl" size="x-small" color="success" variant="tonal">
              已保存（后端生成）
            </v-chip>
            <v-chip v-else size="x-small" color="info" variant="tonal">本地估算</v-chip>
            <v-spacer />
          </div>
          <div class="table-designer__ddl-text">{{ ddlPreview }}</div>
        </div>
      </div>
      <v-divider />

      <!-- 操作栏 -->
      <v-card-actions>
        <span v-if="loadError" class="text-caption text-error mr-2">{{ loadError }}</span>
        <v-spacer />
        <v-btn variant="text" @click="close">取消</v-btn>
        <v-btn
          color="primary"
          prepend-icon="mdi-content-save-outline"
          :loading="saving"
          :disabled="!!loadError"
          @click="save"
        >
          保存
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import {
  mysqlTableDesignGet,
  mysqlTableDesignSave,
  type MySqlColumnAction,
  type MySqlDesignChange,
  type MySqlForeignKeyInfo,
  type MySqlIndexInfo,
  type MySqlTableDesign,
} from '@/api/mysqlDesign'
import { useUiStore } from '@/stores/ui'

const props = defineProps<{
  /** v-model：对话框显隐 */
  modelValue: boolean
  /** 当前 MySQL 连接 ID */
  connId: string
  /** 表名（空串 = 新建表） */
  table: string
  /** 是否为新建表 */
  isNew: boolean
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  /** 保存成功后通知父组件（用于刷新表列表等） */
  (e: 'saved'): void
}>()

/** 关闭对话框（取消按钮） */
function close(): void {
  emit('update:modelValue', false)
}

const ui = useUiStore()

// ---------------- 常量选项 ----------------

/** 常用数据类型下拉建议（v-combobox 支持自由输入） */
const DATA_TYPE_SUGGESTIONS = [
  'INT',
  'BIGINT',
  'TINYINT',
  'SMALLINT',
  'VARCHAR(255)',
  'CHAR(36)',
  'TEXT',
  'DECIMAL(10,2)',
  'FLOAT',
  'DOUBLE',
  'DATE',
  'DATETIME',
  'TIMESTAMP',
  'JSON',
  'BLOB',
  'BOOLEAN',
]

const ENGINE_SUGGESTIONS = ['InnoDB', 'MyISAM', 'MEMORY']
const CHARSET_SUGGESTIONS = ['utf8mb4', 'utf8', 'latin1', 'gbk']
/** ON DELETE / ON UPDATE 行为选项（空串 = 不指定，跟随数据库默认） */
const FK_ACTIONS = ['', 'RESTRICT', 'CASCADE', 'SET NULL', 'NO ACTION']

// ---------------- 编辑状态 ----------------

interface ColumnRow {
  /** 原始字段名（保存时据此判定 add/modify/drop，新建行为空） */
  originalName: string
  name: string
  data_type: string
  is_nullable: boolean
  default_value: string | null
  comment: string
  extra: string
}

interface IndexRow {
  name: string
  columns_text: string
  is_unique: boolean
  is_primary: boolean
  index_type: string
}

interface ForeignKeyRow {
  name: string
  columns_text: string
  ref_table: string
  ref_columns_text: string
  on_delete: string
  on_update: string
}

const columnRows = ref<ColumnRow[]>([])
const indexRows = ref<IndexRow[]>([])
const foreignKeyRows = ref<ForeignKeyRow[]>([])

/** 表属性：仅 is_new = true 时可编辑 */
const tableName = ref('')
const engine = ref('InnoDB')
const charset = ref('utf8mb4')
const comment = ref('')

/** 原始设计快照（保存时用于 diff 出被删除的索引/外键） */
const original = ref<MySqlTableDesign | null>(null)

const activeTab = ref('columns')
const loading = ref(false)
const saving = ref(false)
const loadError = ref('')

/** 保存成功后由后端返回的真实 DDL 文本（非空时替换本地预览） */
const savedDdl = ref('')

// ---------------- 加载表结构 ----------------

/** 打开对话框时拉取表结构；isNew 时跳过，使用空白设计 */
function loadDesign(): void {
  if (props.isNew) {
    columnRows.value = []
    indexRows.value = []
    foreignKeyRows.value = []
    tableName.value = ''
    engine.value = 'InnoDB'
    charset.value = 'utf8mb4'
    comment.value = ''
    original.value = null
    loadError.value = ''
    savedDdl.value = ''
    return
  }
  void doLoadDesign()
}

async function doLoadDesign(): Promise<void> {
  loading.value = true
  loadError.value = ''
  savedDdl.value = ''
  try {
    const design = await mysqlTableDesignGet(props.connId, props.table)
    original.value = design
    tableName.value = design.table
    columnRows.value = design.columns.map((col) => ({
      originalName: col.name,
      name: col.name,
      data_type: col.data_type,
      is_nullable: col.is_nullable,
      default_value: col.default_value,
      comment: col.comment,
      extra: col.extra,
    }))
    indexRows.value = design.indexes.map((idx) => ({
      name: idx.name,
      columns_text: idx.columns.join(', '),
      is_unique: idx.is_unique,
      is_primary: idx.is_primary,
      index_type: idx.index_type,
    }))
    foreignKeyRows.value = design.foreign_keys.map((fk) => ({
      name: fk.name,
      columns_text: fk.columns.join(', '),
      ref_table: fk.ref_table,
      ref_columns_text: fk.ref_columns.join(', '),
      on_delete: fk.on_delete,
      on_update: fk.on_update,
    }))
  } catch (e) {
    loadError.value = String(e)
    ui.toast(`加载表结构失败：${String(e)}`, 'error')
  } finally {
    loading.value = false
  }
}

/** 对话框每次打开时重置并拉取 */
watch(
  () => props.modelValue,
  (open) => {
    if (open) {
      activeTab.value = 'columns'
      loadDesign()
    }
  },
)

// ---------------- 字段行操作 ----------------

function addColumn(): void {
  columnRows.value.push({
    originalName: '',
    name: '',
    data_type: 'VARCHAR(255)',
    is_nullable: true,
    default_value: null,
    comment: '',
    extra: '',
  })
}

function removeColumn(idx: number): void {
  columnRows.value.splice(idx, 1)
}

function moveColumn(idx: number, dir: -1 | 1): void {
  const target = idx + dir
  if (target < 0 || target >= columnRows.value.length) return
  const rows = columnRows.value
  const tmp = rows[idx]
  rows[idx] = rows[target]
  rows[target] = tmp
}

// ---------------- 索引行操作 ----------------

function addIndex(): void {
  indexRows.value.push({
    name: '',
    columns_text: '',
    is_unique: false,
    is_primary: false,
    index_type: 'BTREE',
  })
}

function removeIndex(idx: number): void {
  indexRows.value.splice(idx, 1)
}

// ---------------- 外键行操作 ----------------

function addForeignKey(): void {
  foreignKeyRows.value.push({
    name: '',
    columns_text: '',
    ref_table: '',
    ref_columns_text: '',
    on_delete: '',
    on_update: '',
  })
}

function removeForeignKey(idx: number): void {
  foreignKeyRows.value.splice(idx, 1)
}

// ---------------- 本地 DDL 预览生成 ----------------

/** SQL 字符串字面量转义（单引号与反斜杠） */
function escapeStr(v: string): string {
  return `'${v.replace(/\\/g, '\\\\').replace(/'/g, "''")}'`
}

/** 反引号包裹标识符 */
function quote(v: string): string {
  return `\`${v.replace(/`/g, '``')}\``
}

/** 拆分逗号分隔的列输入为去空白的列名数组 */
function parseCols(text: string): string[] {
  return text
    .split(',')
    .map((c) => c.trim())
    .filter((c) => c.length > 0)
}

/** 根据当前编辑状态在客户端拼接简化版 DDL；保存成功后由真实 DDL 替换 */
const ddlPreview = computed<string>(() => {
  if (savedDdl.value) return savedDdl.value
  const target = props.isNew ? tableName.value.trim() || '<new_table>' : props.table
  const lines: string[] = []
  if (props.isNew) {
    // 新建表：CREATE TABLE 完整拼接
    lines.push(`CREATE TABLE ${quote(target)} (`)

    for (const row of columnRows.value) {
      if (!row.name.trim()) continue
      lines.push(`  ${columnLine(row)},`)
    }
    for (const idx of indexRows.value) {
      const cols = parseCols(idx.columns_text)
      if (!cols.length) continue
      if (idx.is_primary) {
        lines.push(`  PRIMARY KEY (${cols.map(quote).join(', ')}),`)
      } else {
        const unique = idx.is_unique ? 'UNIQUE ' : ''
        lines.push(`  ${unique}KEY ${quote(idx.name)} (${cols.map(quote).join(', ')}),`)
      }
    }
    for (const fk of foreignKeyRows.value) {
      const cols = parseCols(fk.columns_text)
      const refCols = parseCols(fk.ref_columns_text)
      if (!cols.length || !fk.ref_table) continue
      let line = `  CONSTRAINT ${quote(fk.name)} FOREIGN KEY (${cols.map(quote).join(', ')}) REFERENCES ${quote(fk.ref_table)} (${refCols.map(quote).join(', ')})`
      if (fk.on_delete) line += ` ON DELETE ${fk.on_delete}`
      if (fk.on_update) line += ` ON UPDATE ${fk.on_update}`
      lines.push(`${line},`)
    }
    // 去掉最后一行末尾逗号
    if (lines.length > 1) {
      const last = lines.pop() as string
      lines.push(last.replace(/,$/, ''))
    }
    lines.push(')')
    const attrs: string[] = []
    if (engine.value) attrs.push(`ENGINE=${engine.value}`)
    if (charset.value) attrs.push(`DEFAULT CHARSET=${charset.value}`)
    if (comment.value) attrs.push(`COMMENT=${escapeStr(comment.value)}`)
    if (attrs.length) lines[lines.length - 1] += ` ${attrs.join(' ')};`
    else lines[lines.length - 1] += ';'
  } else {
    // 修改表：ALTER TABLE 拼接各子句
    lines.push(`ALTER TABLE ${quote(target)}`)
    for (const row of columnRows.value) {
      if (!row.name.trim()) continue
      if (!row.originalName) {
        lines.push(`  ADD COLUMN ${columnLine(row)},`)
      } else {
        lines.push(`  MODIFY COLUMN ${columnLine(row)},`)
      }
    }
    // diff 出被删除的字段
    if (original.value) {
      const kept = new Set(columnRows.value.map((r) => r.originalName))
      for (const col of original.value.columns) {
        if (!kept.has(col.name)) {
          lines.push(`  DROP COLUMN ${quote(col.name)},`)
        }
      }
    }
    for (const idx of indexRows.value) {
      const cols = parseCols(idx.columns_text)
      if (!cols.length) continue
      if (idx.is_primary) {
        lines.push(`  ADD PRIMARY KEY (${cols.map(quote).join(', ')}),`)
      } else {
        const unique = idx.is_unique ? 'UNIQUE ' : ''
        lines.push(`  ADD ${unique}INDEX ${quote(idx.name)} (${cols.map(quote).join(', ')}),`)
      }
    }
    if (original.value) {
      const kept = new Set(indexRows.value.map((r) => r.name))
      for (const idx of original.value.indexes) {
        if (!idx.is_primary && !kept.has(idx.name)) {
          lines.push(`  DROP INDEX ${quote(idx.name)},`)
        }
      }
    }
    for (const fk of foreignKeyRows.value) {
      const cols = parseCols(fk.columns_text)
      const refCols = parseCols(fk.ref_columns_text)
      if (!cols.length || !fk.ref_table) continue
      let line = `  ADD CONSTRAINT ${quote(fk.name)} FOREIGN KEY (${cols.map(quote).join(', ')}) REFERENCES ${quote(fk.ref_table)} (${refCols.map(quote).join(', ')})`
      if (fk.on_delete) line += ` ON DELETE ${fk.on_delete}`
      if (fk.on_update) line += ` ON UPDATE ${fk.on_update}`
      lines.push(`${line},`)
    }
    if (original.value) {
      const kept = new Set(foreignKeyRows.value.map((r) => r.name))
      for (const fk of original.value.foreign_keys) {
        if (!kept.has(fk.name)) {
          lines.push(`  DROP FOREIGN KEY ${quote(fk.name)},`)
        }
      }
    }
    // 去掉最后一个子句末尾的逗号
    if (lines.length > 1) {
      const last = lines.pop() as string
      lines.push(last.replace(/,$/, ''))
    } else {
      lines.push('  -- 无变更')
    }
    lines.push(';')
  }
  return lines.join('\n')
})

/** 拼接单个字段定义片段（不含 ADD/MODIFY 前缀） */
function columnLine(row: ColumnRow): string {
  const parts: string[] = [quote(row.name.trim()), row.data_type.trim() || 'VARCHAR(255)']
  if (!row.is_nullable) parts.push('NOT NULL')
  if (row.default_value !== null && row.default_value !== '') {
    parts.push(`DEFAULT ${escapeStr(row.default_value)}`)
  }
  if (row.comment) parts.push(`COMMENT ${escapeStr(row.comment)}`)
  if (row.extra) parts.push(row.extra)
  return parts.join(' ')
}

// ---------------- 保存 ----------------

/** 组装 MySqlDesignChange 并提交后端，成功后用真实 DDL 替换预览 */
async function save(): Promise<void> {
  // 基础校验：字段名必填
  if (columnRows.value.some((r) => !r.name.trim())) {
    ui.toast('存在未填写的字段名，请检查字段列表', 'warning')
    return
  }
  // 新建表时表名取自属性区输入框，编辑表时取自 props
  const targetTable = (props.isNew ? tableName.value : props.table).trim()
  const change: MySqlDesignChange = {
    table: targetTable,
    is_new: props.isNew,
    columns: columnRows.value
      .filter((r) => r.name.trim())
      .map((row) => ({
        action: (props.isNew || !row.originalName ? 'add' : 'modify') as MySqlColumnAction,
        name: row.name.trim(),
        data_type: row.data_type.trim(),
        is_nullable: row.is_nullable,
        default_value: row.default_value === '' ? null : row.default_value,
        comment: row.comment,
        extra: row.extra,
      })),
    indexes: indexRows.value
      .filter((idx) => !idx.is_primary && parseCols(idx.columns_text).length > 0)
      .map((idx) => ({
        name: idx.name.trim(),
        columns: parseCols(idx.columns_text),
        is_unique: idx.is_unique,
        is_primary: idx.is_primary,
        index_type: idx.index_type || 'BTREE',
      })),
    dropped_indexes: diffDropped(
      original.value?.indexes.map((i) => i.name) ?? [],
      indexRows.value.map((i) => i.name.trim()),
    ),
    foreign_keys: foreignKeyRows.value
      .filter((fk) => parseCols(fk.columns_text).length > 0 && fk.ref_table.trim())
      .map((fk) => ({
        name: fk.name.trim(),
        columns: parseCols(fk.columns_text),
        ref_table: fk.ref_table.trim(),
        ref_columns: parseCols(fk.ref_columns_text),
        on_delete: fk.on_delete,
        on_update: fk.on_update,
      })),
    dropped_foreign_keys: diffDropped(
      original.value?.foreign_keys.map((f) => f.name) ?? [],
      foreignKeyRows.value.map((f) => f.name.trim()),
    ),
    engine: props.isNew ? engine.value || null : null,
    charset: props.isNew ? charset.value || null : null,
    comment: props.isNew ? comment.value || null : null,
  }
  if (!change.table) {
    ui.toast('缺少表名，无法保存', 'warning')
    return
  }
  try {
    const ddl = await mysqlTableDesignSave(props.connId, change)
    // 保存成功：展示后端生成的真实 DDL
    savedDdl.value = ddl
    ui.toast('表设计已保存', 'success')
    emit('saved')
  } catch (e) {
    ui.toast(`保存表设计失败：${String(e)}`, 'error')
  }
}

/** diff 两组索引/外键名称：原集合中存在而当前集合缺失的（即被删除的） */
function diffDropped(original: string[], current: string[]): string[] {
  const cur = new Set(current)
  return original.filter((name) => name && !cur.has(name))
}
</script>

<style scoped>
.table-designer {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.table-designer__title {
  gap: 8px;
}

.table-designer__body {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

/* 编辑区：字段/索引/外键/属性切换面板 */
.table-designer__edit-area {
  display: flex;
  flex-direction: column;
  height: 380px;
  min-height: 0;
}

.table-designer__toolbar {
  display: flex;
  align-items: center;
  padding: 6px 8px;
}

.table-designer__scroll {
  flex: 1 1 auto;
  overflow-y: auto;
  min-height: 0;
  padding: 0 8px 8px;
}

/* 表头通用样式：字段/索引/外键三个 Tab 共用 */
.table-designer__head {
  position: sticky;
  top: 0;
  z-index: 1;
  height: 32px;
  font-size: 14px;
  font-weight: 400;
  color: rgba(var(--v-theme-on-surface), 0.7);
  background: rgb(var(--v-theme-surface));
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  user-select: none;
  display: grid;
  gap: 8px;
  align-items: center;
}

/* 字段区 8 列布局 */
.table-designer__head--cols,
.table-designer__col-row {
  display: grid;
  grid-template-columns: 44px 160px 160px 88px 140px minmax(140px, 1fr) 180px 116px;
  gap: 6px;
  align-items: center;
}

/* 索引区表头 6 列布局 */
.table-designer__head--idx {
  grid-template-columns: 160px minmax(200px, 1fr) 88px 88px 120px 56px;
}

/* 外键区表头 7 列布局 */
.table-designer__head--foreign {
  grid-template-columns: 140px 140px 140px 140px 120px 120px 56px;
}

/* 索引/外键区 7 列布局 */
.table-designer__col-row--index {
  display: grid;
  grid-template-columns: 160px minmax(200px, 1fr) 88px 88px 120px 56px;
  gap: 8px;
  align-items: center;
  margin-bottom: 6px;
}

.table-designer__col-row {
  margin-bottom: 6px;
}

/* 索引/外键行复用同一样式（模板中同一 class） */
.table-designer__col-row--foreign {
  display: grid;
  grid-template-columns: 140px 140px 140px 140px 120px 120px 56px;
  gap: 8px;
  align-items: center;
  margin-bottom: 6px;
}

.table-designer__check {
  justify-content: center;
}

.table-designer__row-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 2px;
}

.table-designer__order {
  text-align: center;
  font-size: 14px;
  color: rgba(var(--v-theme-on-surface), 0.6);
}

.table-designer__empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 120px;
  font-size: 14px;
  color: rgba(var(--v-theme-on-surface), 0.5);
}

/* 表属性面板 */
.table-designer__props {
  padding: 12px;
}

/* DDL 预览面板 */
.table-designer__ddl {
  padding: 8px;
  max-height: 160px;
  display: flex;
  flex-direction: column;
}

.table-designer__ddl-text {
  flex: 1 1 auto;
  overflow: auto;
  min-height: 0;
  padding: 8px;
  font-family: var(--fy-font);
  font-size: 14px;
  white-space: pre-wrap;
  word-break: break-all;
  background: rgba(var(--v-theme-on-surface), 0.04);
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
}
</style>
