<template>
  <v-card class="mysql-grid" flat>
    <!-- 顶部状态条：当前连接 + 断开按钮 -->
    <div class="mysql-grid__toolbar">
      <v-icon size="small" class="mr-2">mdi-database-outline</v-icon>
      <span v-if="store.isConnected" class="text-body-2">{{ store.connLabel }}</span>
      <span v-else class="text-body-2 text-medium-emphasis">未连接</span>
      <v-spacer />
      <v-chip v-if="store.inTransaction" size="x-small" color="warning" variant="tonal" class="mr-2">
        事务进行中
      </v-chip>
      <v-btn
        size="small"
        variant="outlined"
        prepend-icon="mdi-database-plus"
        @click="showConnForm = true"
      >
        连接
      </v-btn>
      <v-btn
        size="small"
        variant="text"
        prepend-icon="mdi-lan-disconnect"
        :disabled="!store.isConnected"
        @click="doDisconnect"
      >
        断开
      </v-btn>
      <!-- P2 入口：表设计（编辑选中表 / 新建表）、导入导出 -->
      <v-btn
        size="small"
        variant="text"
        prepend-icon="mdi-table-edit"
        :disabled="!store.isConnected || !selectedTable"
        title="设计选中表"
        @click="openDesigner(selectedTable, false)"
      >
        表设计
      </v-btn>
      <v-btn
        size="small"
        variant="text"
        prepend-icon="mdi-table-plus"
        :disabled="!store.isConnected"
        title="新建表"
        @click="openDesigner('', true)"
      >
        新建表
      </v-btn>
      <v-btn
        size="small"
        variant="text"
        prepend-icon="mdi-swap-vertical"
        :disabled="!store.isConnected"
        title="导入 / 导出"
        @click="showIo = true"
      >
        导入导出
      </v-btn>
    </div>
    <v-divider />

    <div class="mysql-grid__body">
      <!-- 左侧：表列表侧栏（表名/行数/引擎/注释） -->
      <div class="mysql-grid__sidebar">
        <div class="d-flex align-center px-2 py-1">
          <v-text-field
            v-model="tableFilter"
            label="筛选表"
            density="compact"
            single-line
            hide-details
            clearable
            prepend-inner-icon="mdi-magnify"
            class="mr-1"
          />
          <v-btn
            icon="mdi-refresh"
            size="x-small"
            variant="text"
            :loading="store.tablesLoading"
            :disabled="!store.isConnected"
            title="刷新表列表"
            @click="refreshTables"
          />
        </div>
        <v-divider />
        <div class="mysql-grid__table-list">
          <v-list density="compact" nav>
            <v-list-item
              v-for="t in filteredTables"
              :key="t.name"
              :active="t.name === selectedTable"
              @click="selectTable(t.name)"
            >
              <template #prepend>
                <v-icon size="small">mdi-table-outline</v-icon>
              </template>
              <v-list-item-title class="text-body-2">{{ t.name }}</v-list-item-title>
              <v-list-item-subtitle class="text-caption">
                {{ t.rows.toLocaleString() }} 行 · {{ t.engine || '-' }}<template v-if="t.comment"> · {{ t.comment }}</template>
              </v-list-item-subtitle>
            </v-list-item>
            <v-list-item v-if="!store.tablesLoading && filteredTables.length === 0">
              <v-list-item-title class="text-caption text-medium-emphasis">
                {{ store.isConnected ? '没有匹配的表' : '连接后显示表列表' }}
              </v-list-item-title>
            </v-list-item>
          </v-list>
        </div>
      </div>

      <!-- 右侧：SQL 编辑区 + 结果网格 -->
      <div class="mysql-grid__main">
        <!-- SQL 编辑区 -->
        <div class="mysql-grid__editor">
          <div class="d-flex align-center mb-1">
            <span class="text-body-2 mr-2">SQL</span>
            <v-btn
              size="small"
              color="primary"
              variant="flat"
              prepend-icon="mdi-play"
              :loading="store.executing || store.queryLoading"
              :disabled="!store.isConnected"
              @click="runSql"
            >
              执行
            </v-btn>
            <!-- P2 入口：执行计划 / 查询历史 -->
            <v-btn
              size="small"
              variant="text"
              prepend-icon="mdi-chart-tree"
              :disabled="!store.isConnected || !sql.trim()"
              title="查看执行计划"
              @click="showExplain = true"
            >
              执行计划
            </v-btn>
            <v-btn
              size="small"
              variant="text"
              prepend-icon="mdi-history"
              title="查询历史"
              @click="showHistory = true"
            >
              历史
            </v-btn>
            <v-spacer />
            <!-- 事务按钮组 -->
            <v-btn
              size="small"
              variant="outlined"
              prepend-icon="mdi-plus-circle-outline"
              :disabled="!store.isConnected || store.inTransaction"
              @click="doBegin"
            >
              BEGIN
            </v-btn>
            <v-btn
              size="small"
              class="ml-1"
              variant="outlined"
              prepend-icon="mdi-check"
              :disabled="!store.isConnected || !store.inTransaction"
              @click="doCommit"
            >
              COMMIT
            </v-btn>
            <v-btn
              size="small"
              class="ml-1"
              variant="outlined"
              color="error"
              prepend-icon="mdi-undo"
              :disabled="!store.isConnected || !store.inTransaction"
              @click="doRollback"
            >
              ROLLBACK
            </v-btn>
          </div>
          <SqlEditor v-model="sql" :tables="tableNames" placeholder="输入 SQL 语句…" @execute="runSql" />
        </div>

        <!-- 错误 / 结果提示 -->
        <v-alert v-if="store.queryError" type="error" variant="tonal" density="compact" closable class="mt-2">
          {{ store.queryError }}
        </v-alert>
        <v-alert v-else-if="store.executeError" type="error" variant="tonal" density="compact" closable class="mt-2">
          {{ store.executeError }}
        </v-alert>
        <v-alert v-else-if="store.executeMessage" type="success" variant="tonal" density="compact" closable class="mt-2">
          {{ store.executeMessage }}
        </v-alert>

        <!-- 编辑管道工具条：待提交集管理 + 选中行删除（均走预览 -> 确认 -> 执行） -->
        <div v-if="store.lastResult" class="d-flex align-center mb-1">
          <v-chip v-if="pendingEdits.size" size="x-small" color="warning" variant="tonal" class="mr-2">
            {{ pendingEdits.size }} 处待提交修改
          </v-chip>
          <v-btn
            size="small"
            color="primary"
            variant="tonal"
            prepend-icon="mdi-check-all"
            :disabled="!pendingEdits.size"
            :loading="previewLoading"
            @click="commitEdits"
          >
            提交修改
          </v-btn>
          <v-btn size="small" variant="text" class="ml-1" :disabled="!pendingEdits.size" @click="discardEdits">
            放弃修改
          </v-btn>
          <v-spacer />
          <span v-if="!canEdit && store.lastResult" class="text-caption text-medium-emphasis mr-2">
            编辑需单表查询且可解析主键
          </span>
          <v-btn
            size="small"
            color="error"
            variant="outlined"
            prepend-icon="mdi-delete-outline"
            :disabled="!selectedRows.size"
            @click="deleteSelected"
          >
            删除选中行
          </v-btn>
        </div>

        <!-- 结果网格：NULL 显示为灰色斜体；双击编辑，右键设/清 NULL -->
        <div v-if="store.lastResult" class="mysql-grid__result">
          <v-table density="compact" fixed-header class="mysql-grid__result-table">
            <thead>
              <tr>
                <th style="width: 36px"><!-- 行选择复选框列 --></th>
                <th v-for="col in resultColumns" :key="col" class="text-left">
                  <v-icon v-if="col === pkColumn" size="x-small" class="mr-1" title="主键列">mdi-key</v-icon>{{ col }}
                </th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="(row, ri) in displayRows"
                :key="ri"
                :class="{ 'mysql-grid__row--selected': selectedRows.has(ri) }"
              >
                <td>
                  <v-checkbox-btn
                    :model-value="selectedRows.has(ri)"
                    density="compact"
                    hide-details
                    @update:model-value="(v: unknown) => toggleRow(ri, v)"
                  />
                </td>
                <td
                  v-for="(cell, ci) in row"
                  :key="ci"
                  class="text-body-2 mysql-grid__cell"
                  :class="{
                    'mysql-grid__cell--edited': isEdited(ri, ci),
                    'mysql-grid__cell--editable': canEdit,
                  }"
                  title="双击编辑；右键设为/清除 NULL"
                  @dblclick="startEdit(ri, ci)"
                  @contextmenu.prevent="openCtxMenu($event, ri, ci)"
                >
                  <input
                    v-if="editingCell && editingCell.ri === ri && editingCell.ci === ci"
                    v-model="editingValue"
                    class="mysql-grid__cell-input"
                    autofocus
                    @keyup.enter="finalizeEdit"
                    @keyup.esc="cancelEdit"
                    @blur="finalizeEdit"
                  />
                  <template v-else>
                    <span
                      v-if="cell === null"
                      class="mysql-grid__null"
                      :class="{ 'mysql-grid__null--edited': isEdited(ri, ci) }"
                    >NULL</span>
                    <template v-else>{{ cell }}</template>
                  </template>
                </td>
              </tr>
              <tr v-if="!displayRows.length">
                <td :colspan="columnCount" class="text-caption text-medium-emphasis text-center py-2">
                  空结果集
                </td>
              </tr>
            </tbody>
          </v-table>
        </div>
        <div v-else class="mysql-grid__placeholder text-caption text-medium-emphasis">
          在左侧选择表，或在上方输入 SQL 执行
        </div>

        <!-- 分页控件（服务端分页：page/pageSize 来自查询结果） -->
        <div v-if="store.lastResult" class="mysql-grid__pager">
          <span class="text-caption text-medium-emphasis mr-2">
            共 {{ store.lastResult.total.toLocaleString() }} 行
          </span>
          <v-pagination
            :model-value="page"
            :length="pageCount"
            :total-visible="7"
            size="small"
            density="comfortable"
            @update:model-value="onPageChange"
          />
          <v-select
            :model-value="pageSize"
            :items="PAGE_SIZES"
            label="每页"
            density="compact"
            single-line
            hide-details
            style="max-width: 90px"
            @update:model-value="onPageSizeChange"
          />
        </div>
      </div>
    </div>

    <!-- 危险 SQL 二次确认框（由 store.pendingConfirm 驱动） -->
    <v-dialog
      :model-value="!!store.pendingConfirm"
      width="480"
      persistent
      @update:model-value="(v: boolean) => { if (!v) store.cancelConfirm() }"
    >
      <v-card>
        <v-card-title class="d-flex align-center">
          <v-icon size="small" color="warning" class="mr-2">mdi-alert-outline</v-icon>
          危险操作确认
        </v-card-title>
        <v-divider />
        <v-card-text>
          <v-alert type="warning" variant="tonal" density="compact" class="mb-2">
            该语句可能删除数据或修改表结构，请确认是否执行。
          </v-alert>
          <div class="mysql-grid__sql-preview">{{ store.pendingConfirm?.sql }}</div>
        </v-card-text>
        <v-divider />
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="store.cancelConfirm()">取消</v-btn>
          <v-btn color="error" prepend-icon="mdi-alert" :loading="store.executing" @click="store.confirmExecute()">
            确认执行
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- 单元格右键菜单：编辑 / 设为 NULL / 清除 NULL（fixed 定位，覆盖层负责点击关闭） -->
    <template v-if="ctxMenu">
      <div
        class="mysql-grid__ctx-overlay"
        @click="ctxMenu = null"
        @contextmenu.prevent="ctxMenu = null"
      />
      <v-card
        class="mysql-grid__ctx-menu"
        :style="{ top: `${ctxMenu.y}px`, left: `${ctxMenu.x}px` }"
      >
        <v-list density="compact" nav>
          <v-list-item prepend-icon="mdi-pencil" @click="ctxAction('edit')">编辑单元格</v-list-item>
          <v-list-item prepend-icon="mdi-null" @click="ctxAction('null')">设为 NULL</v-list-item>
          <v-list-item prepend-icon="mdi-eraser" @click="ctxAction('clear')">清除 NULL（写空串）</v-list-item>
        </v-list>
      </v-card>
    </template>

    <!-- 编辑预览对话框：展示将执行的 SQL 与估算影响行数，确认后执行 -->
    <v-dialog v-model="showPreview" width="640">
      <v-card>
        <v-card-title class="d-flex align-center">
          <v-icon size="small" class="mr-2">mdi-eye-outline</v-icon>
          {{ previewMode === 'edit' ? '编辑预览' : '删除预览' }}
        </v-card-title>
        <v-divider />
        <v-card-text>
          <div v-for="(item, i) in previewItems" :key="i" class="mysql-grid__sql-preview mb-2">
            <div class="mysql-grid__sql-preview-sql">{{ item.sql }}</div>
            <div class="text-caption text-medium-emphasis">估算影响行数：{{ item.estimate }}</div>
          </div>
          <v-alert
            v-if="previewDanger || previewMaxEstimate > 1"
            type="warning"
            variant="tonal"
            density="compact"
          >
            本次操作预计影响 {{ previewMaxEstimate }} 行（超过 1 行）或命中危险特征，确认后将二次确认。
          </v-alert>
        </v-card-text>
        <v-divider />
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="showPreview = false">取消</v-btn>
          <v-btn
            color="primary"
            prepend-icon="mdi-check"
            :loading="executing"
            @click="confirmPreview"
          >
            确认执行
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- 连接对话框 -->
    <MysqlConnectionForm v-model="showConnForm" @connected="onConnected" />

    <!-- P2：表设计器 / 查询历史 / 执行计划 / 导入导出对话框 -->
    <TableDesigner
      v-model="showDesigner"
      :conn-id="store.connId ?? ''"
      :table="designerTable"
      :is-new="designerIsNew"
      @saved="onDesignerSaved"
    />
    <HistoryDrawer v-model="showHistory" @recall="recallSql" />
    <ExplainPanel v-model="showExplain" :conn-id="store.connId ?? ''" :sql="sql" />
    <ImportExportDialog v-model="showIo" :conn-id="store.connId ?? ''" />
  </v-card>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useMysqlStore } from '@/stores/mysql'
import { useUiStore } from '@/stores/ui'
import {
  mysqlDeleteRow,
  mysqlEditPreview,
  mysqlTableDesignGet,
  mysqlUpdateRows,
} from '@/api/mysqlEdit'
import type { MySqlRowUpdate } from '@/api/mysqlEdit'
import MysqlConnectionForm from './MysqlConnectionForm.vue'
import SqlEditor from './SqlEditor.vue'
import TableDesigner from '@/views/mysql/TableDesigner.vue'
import HistoryDrawer from './HistoryDrawer.vue'
import ExplainPanel from './ExplainPanel.vue'
import ImportExportDialog from './ImportExportDialog.vue'

const store = useMysqlStore()
const ui = useUiStore()

/** 错误归一化：Rust 侧 AppError 以字符串形式 reject */
function errText(err: unknown): string {
  return typeof err === 'string' ? err : String(err)
}

/** 只读 SELECT 路由到 mysqlQuery（分页），其余语句走 mysqlExecute（写操作） */
const SELECT_RE = /^\s*SELECT\b/i

const PAGE_SIZES = [10, 50, 100, 200]

// ---------- P2 入口：表设计器 / 查询历史 / 执行计划 / 导入导出 ----------
const showDesigner = ref(false)
const designerTable = ref('')
const designerIsNew = ref(false)
const showHistory = ref(false)
const showExplain = ref(false)
const showIo = ref(false)

/** 打开表设计器：选中表进入编辑模式，否则新建表 */
function openDesigner(table: string, isNew: boolean): void {
  designerTable.value = table
  designerIsNew.value = isNew
  showDesigner.value = true
}

/** 设计器保存成功后刷新表列表（新建表 / 结构变更后行数变化） */
function onDesignerSaved(): void {
  void refreshTables()
}

/** 查询历史快召回：插入 SQL 编辑器（已含换行则追加，否则换行分隔） */
function recallSql(text: string): void {
  sql.value = sql.value.trimEnd() ? `${sql.value.trimEnd()}\n${text}` : text
}

// ---------- 左侧表列表 ----------
const tableFilter = ref<string | null>(null)
const selectedTable = ref('')

const filteredTables = computed(() => {
  const kw = (tableFilter.value ?? '').trim().toLowerCase()
  if (!kw) return store.tables
  return store.tables.filter((t) => t.name.toLowerCase().includes(kw))
})

async function refreshTables(): Promise<void> {
  try {
    await store.loadTables()
  } catch {
    // 错误已写入 store，由面板 v-alert 展示
  }
}

/** 点击表：填入 SELECT 语句并查询第一页 */
function selectTable(name: string): void {
  selectedTable.value = name
  sql.value = `SELECT * FROM \`${name}\``
  void runQuery(1, pageSize.value)
}

// ---------- SQL 编辑与执行 ----------
const sql = ref('')
const page = ref(1)
const pageSize = ref(10)

/** 表名列表（CodeMirror schema 表名补全数据源，随表列表刷新） */
const tableNames = computed(() => store.tables.map((t) => t.name))

/** 最近一次查询的 SQL（翻页/改每页大小时复用） */
const lastQuerySql = ref('')

// ---------- 行编辑状态（P2：编辑管道 + 显式 NULL） ----------

/** 待提交修改条目 */
interface PendingEdit {
  /** 当页行索引（随查询/翻页失效，届时清空待提交集） */
  rowIndex: number
  /** 目标列名 */
  column: string
  /** 新值（isNull=true 时忽略） */
  value: string | null
  /** true = 显式写 NULL */
  isNull: boolean
}

/** 待提交修改集：key = "行索引:列名"（不直接写库，提交时统一走预览 -> 确认 -> 执行） */
const pendingEdits = ref(new Map<string, PendingEdit>())

/** 正在编辑的单元格（null = 无编辑态） */
const editingCell = ref<{ ri: number; ci: number } | null>(null)

/** 编辑中的临时值 */
const editingValue = ref('')

/** 行选择集：key = 当页行索引 */
const selectedRows = ref(new Set<number>())

/** 当前表的主键列名（空串 = 未解析到，编辑禁用） */
const pkColumn = ref('')

/** 解析当前查询的目标表名：支持 db.table 限定形式，取表名部分 */
const FROM_RE = /\bFROM\s+`?([\w$]+)`?(?:\s*\.\s*`?([\w$]+)`)?/i

function tableOfQuery(querySql: string): string {
  const m = FROM_RE.exec(querySql)
  if (!m) return ''
  return m[2] ?? m[1] // db.table 取表名部分
}

/** 当前查询是否为单表 SELECT（编辑管道仅对这类结果开放） */
const canEdit = computed(() => {
  return (
    store.isConnected &&
    !!store.lastResult &&
    SELECT_RE.test(lastQuerySql.value) &&
    !!tableOfQuery(lastQuerySql.value) &&
    !!pkColumn.value
  )
})

/**
 * 解析主键列：优先 mysql_table_design_get（COLUMN_KEY='PRI'，与 design 模块
 * 同源的后端命令，api 层独立封装了最小子集类型）；失败回退启发式：
 * 常见主键名 id/uuid，否则以结果第一列为主键。
 *
 * ⚠️ 边界：第一列兜底仅在第一列恰好为主键时正确定位，否则可能定位错行——
 * 预览管道的 COUNT(*) 估算与"影响行数 > 1 二次确认"会兜底提示该风险。
 */
async function resolvePk(fromTable: string): Promise<void> {
  pkColumn.value = ''
  if (!store.connId || !fromTable) return
  try {
    const design = await mysqlTableDesignGet(store.connId, fromTable)
    const pri = design.columns.find((c) => (c.key_type ?? '').toUpperCase() === 'PRI')
    if (pri) pkColumn.value = pri.name
  } catch {
    // 拉取失败（无权限/表不存在等），忽略并走启发式兜底
  }
  if (!pkColumn.value) {
    const cols = store.lastResult?.columns ?? []
    pkColumn.value = cols.find((c) => /^(id|uuid)$/i.test(c)) ?? cols[0] ?? ''
  }
}

/** 读取指定行的主键值（提交时构造 MySqlRowUpdate 用） */
function pkValueOf(rowIndex: number): string | null {
  const r = store.lastResult
  if (!r || !pkColumn.value) return null
  const idx = r.columns.indexOf(pkColumn.value)
  if (idx < 0) return null
  return r.rows[rowIndex]?.[idx] ?? null
}

/** 单元格是否处于待提交修改中（提供高亮样式） */
function isEdited(ri: number, ci: number): boolean {
  const col = resultColumns.value[ci]
  return col ? pendingEdits.value.has(`${ri}:${col}`) : false
}

/** 开始编辑单元格（仅 canEdit 时开放） */
function startEdit(ri: number, ci: number): void {
  if (!canEdit.value) return
  // 切换单元格前先落定上一格
  if (editingCell.value && (editingCell.value.ri !== ri || editingCell.value.ci !== ci)) {
    finalizeEdit()
  }
  editingCell.value = { ri, ci }
  const cell = displayRows.value[ri]?.[ci] ?? null
  editingValue.value = cell === null ? '' : String(cell)
}

/** 落定编辑：值有变化才加入待提交集（不直接写库） */
function finalizeEdit(): void {
  const cell = editingCell.value
  if (!cell) return
  editingCell.value = null
  const col = resultColumns.value[cell.ci]
  if (!col) return
  const original = store.lastResult?.rows[cell.ri]?.[cell.ci] ?? null
  // 边界：原值为 NULL 时留空 = 不变（保持 NULL）；NULL -> 空串请用右键"清除 NULL"
  if (editingValue.value === (original ?? '')) return
  pendingEdits.value = new Map(pendingEdits.value).set(`${cell.ri}:${col}`, {
    rowIndex: cell.ri,
    column: col,
    value: editingValue.value,
    isNull: false,
  })
}

/** 取消编辑（Esc），不记录任何修改 */
function cancelEdit(): void {
  editingCell.value = null
}

/** 右键菜单状态：屏幕坐标 + 目标单元格 */
const ctxMenu = ref<{ x: number; y: number; ri: number; ci: number } | null>(null)

/** 打开右键菜单（坐标钳制到视口内） */
function openCtxMenu(e: MouseEvent, ri: number, ci: number): void {
  if (!canEdit.value) return
  ctxMenu.value = {
    x: Math.min(e.clientX, window.innerWidth - 200),
    y: Math.min(e.clientY, window.innerHeight - 150),
    ri,
    ci,
  }
}

/** 右键菜单动作分发 */
function ctxAction(action: 'edit' | 'null' | 'clear'): void {
  const menu = ctxMenu.value
  ctxMenu.value = null
  if (!menu) return
  if (action === 'edit') startEdit(menu.ri, menu.ci)
  else if (action === 'null') setCellNull(menu.ri, menu.ci)
  else clearCellNull(menu.ri, menu.ci)
}

/** 设为 NULL：is_null=true 显式写 NULL（区分空串与 NULL 语义） */
function setCellNull(ri: number, ci: number): void {
  const col = resultColumns.value[ci]
  if (!col || !canEdit.value) return
  pendingEdits.value = new Map(pendingEdits.value).set(`${ri}:${col}`, {
    rowIndex: ri,
    column: col,
    value: null,
    isNull: true,
  })
}

/** 清除 NULL：写入空串（is_null=false + value=''，与 NULL 显式区分） */
function clearCellNull(ri: number, ci: number): void {
  const col = resultColumns.value[ci]
  if (!col || !canEdit.value) return
  pendingEdits.value = new Map(pendingEdits.value).set(`${ri}:${col}`, {
    rowIndex: ri,
    column: col,
    value: '',
    isNull: false,
  })
}

/** 行选择切换 */
function toggleRow(ri: number, v: unknown): void {
  const set = new Set(selectedRows.value)
  if (v) set.add(ri)
  else set.delete(ri)
  selectedRows.value = set
}

/** 放弃全部待提交修改 */
function discardEdits(): void {
  pendingEdits.value = new Map()
}

// ---------- 编辑管道：预览 -> 确认 -> 执行 ----------

/** 预览条目（SQL + 估算影响行数 + danger 标记） */
interface PreviewItem {
  sql: string
  estimate: number
  danger: boolean
}

const showPreview = ref(false)
const previewLoading = ref(false)
const executing = ref(false)
const previewMode = ref<'edit' | 'delete'>('edit')
const previewItems = ref<PreviewItem[]>([])

/** 批量估算影响行数上界（各条目估算的最大值） */
const previewMaxEstimate = computed(() =>
  Math.max(0, ...previewItems.value.map((i) => i.estimate)),
)

const previewDanger = computed(() => previewItems.value.some((i) => i.danger))

/**
 * 提交修改：对每条待提交修改调 mysql_edit_preview 生成 SQL 与估算，
 * 聚合后弹出预览对话框（确认环节在 confirmPreview 中）
 */
async function commitEdits(): Promise<void> {
  const connId = store.connId
  const table = tableOfQuery(lastQuerySql.value)
  if (!connId || !table || !pkColumn.value || pendingEdits.value.size === 0) return
  previewLoading.value = true
  try {
    const items: PreviewItem[] = []
    for (const edit of pendingEdits.value.values()) {
      // 后端逐条生成 UPDATE 预览并 COUNT(*) 估算，前端聚合展示
      const preview = await mysqlEditPreview(connId, {
        table,
        pk_column: pkColumn.value,
        pk_value: pkValueOf(edit.rowIndex),
        column: edit.column,
        value: edit.value,
        is_null: edit.isNull,
      })
      items.push({ sql: preview.sql, estimate: preview.affected_estimate, danger: preview.danger })
    }
    previewItems.value = items
    previewMode.value = 'edit'
    showPreview.value = true
  } catch (err) {
    ui.toast(errText(err), 'error')
  } finally {
    previewLoading.value = false
  }
}

/**
 * 删除选中行：后端契约无删除预览命令，DELETE SQL 由前端构造（仅用于展示，
 * 单引号/反斜杠成对转义）；实际执行走 mysql_delete_row，按主键定位恒含 WHERE
 */
async function deleteSelected(): Promise<void> {
  const table = tableOfQuery(lastQuerySql.value)
  if (!table || !pkColumn.value) {
    ui.toast('无法定位主键列，无法删除', 'warning')
    return
  }
  if (selectedRows.value.size === 0) return
  const items: PreviewItem[] = []
  for (const ri of selectedRows.value) {
    const pv = pkValueOf(ri)
    const predicate =
      pv === null ? 'IS NULL' : `= '${pv.replace('\\', '\\\\').replace('\'', '\'\'')}'`
    items.push({
      sql: `DELETE FROM \`${table}\` WHERE \`${pkColumn.value}\` ${predicate}`,
      estimate: 1,
      danger: false,
    })
  }
  previewItems.value = items
  previewMode.value = 'delete'
  showPreview.value = true
}

/**
 * 预览确认后的执行入口：
 * - danger=true 或估算影响行数 > 1 时先弹 uiStore.confirm 二次确认
 * - 编辑批量走 mysql_update_rows（隐式事务包裹，失败整体回滚）
 * - 删除逐行走 mysql_delete_row
 */
async function confirmPreview(): Promise<void> {
  const connId = store.connId
  if (!connId) return
  if (previewDanger.value || previewMaxEstimate.value > 1) {
    const ok = await ui.confirm({
      title: '危险操作确认',
      message: `本次操作预计影响 ${previewMaxEstimate.value} 行（超过 1 行）或命中危险 SQL 特征，请确认是否执行。`,
      confirmText: '确认执行',
      danger: true,
    })
    if (!ok) return
  }
  const table = tableOfQuery(lastQuerySql.value)
  executing.value = true
  try {
    if (previewMode.value === 'edit') {
      const updates: MySqlRowUpdate[] = [...pendingEdits.value.values()].map((edit) => ({
        table,
        pk_column: pkColumn.value,
        pk_value: pkValueOf(edit.rowIndex),
        column: edit.column,
        value: edit.value,
        is_null: edit.isNull,
      }))
      const affected = await mysqlUpdateRows(connId, { table, updates }, true)
      ui.toast(`已提交 ${updates.length} 处修改，受影响行数：${affected}`, 'success')
      pendingEdits.value = new Map()
    } else {
      const rows = [...selectedRows.value]
      let affected = 0
      for (const ri of rows) {
        affected += await mysqlDeleteRow(connId, table, pkColumn.value, pkValueOf(ri), true)
      }
      ui.toast(`已删除 ${rows.length} 行，受影响行数：${affected}`, 'success')
      selectedRows.value = new Set()
    }
    showPreview.value = false
    // 执行成功后刷新当前页（写操作可能改变结果集）
    void runQuery(page.value, pageSize.value)
  } catch (err) {
    ui.toast(errText(err), 'error')
  } finally {
    executing.value = false
  }
}

/** 结果网格展示行：与列头按索引对齐，null 保持为 null（模板中渲染为灰色斜体 NULL）。
 * 待提交修改叠加展示：命中处显示新值/NULL，由 isEdited 提供高亮样式 */
const displayRows = computed<(string | null)[][]>(() => {
  const r = store.lastResult
  if (!r) return []
  return r.rows.map((row, ri) =>
    r.columns.map((_, ci) => {
      const col = r.columns[ci]
      const edit = pendingEdits.value.get(`${ri}:${col}`)
      if (edit) return edit.isNull ? null : (edit.value ?? '')
      return row[ci] ?? null
    }),
  )
})

const columnCount = computed(() => store.lastResult?.columns.length ?? 1)

/** 当前查询结果的列名列表（模板与 isEdited 共用） */
const resultColumns = computed(() => store.lastResult?.columns ?? [])

/** 分页总页数（由 total/pageSize 计算，向后端翻页） */
const pageCount = computed(() => {
  const r = store.lastResult
  if (!r) return 1
  return Math.max(1, Math.ceil(r.total / Math.max(1, r.page_size || pageSize.value)))
})

/** 执行入口：SELECT 走分页查询，其余走写操作（危险 SQL 自动进入确认流程） */
async function runSql(): Promise<void> {
  const text = sql.value.trim()
  if (!text) return
  if (SELECT_RE.test(text)) {
    await runQuery(1, pageSize.value, text)
  } else {
    try {
      const outcome = await store.executeSql(text)
      if (outcome.needsConfirm) return // 确认框由 store.pendingConfirm 驱动弹出
    } catch {
      // 错误已写入 store，由 v-alert 展示
    }
  }
}

/** 分页查询：page 从 1 开始 */
async function runQuery(p: number, size: number, querySql?: string): Promise<void> {
  const text = querySql ?? lastQuerySql.value
  if (!text) return
  try {
    await store.query(text, p, size)
    lastQuerySql.value = text
    page.value = p
    pageSize.value = size
    // 行索引随查询/翻页失效：清空待提交集、选中行与编辑态，避免错位写入
    pendingEdits.value = new Map()
    selectedRows.value = new Set()
    editingCell.value = null
    ctxMenu.value = null
    // 异步解析当前表主键列（失败回退启发式，不阻塞结果渲染）
    void resolvePk(tableOfQuery(text))
  } catch {
    // 错误已写入 store，由 v-alert 展示
  }
}

function onPageChange(page: number): void {
  void runQuery(page, pageSize.value)
}

function onPageSizeChange(v: unknown): void {
  const size = Number(v) || 10
  void runQuery(1, size)
}

// ---------- 事务按钮组 ----------
async function doBegin(): Promise<void> {
  try {
    await store.beginTransaction()
  } catch {
    /* 错误提示由后续 alert 展示 */
  }
}

async function doCommit(): Promise<void> {
  try {
    await store.commit()
    store.executeMessage = '事务已提交（COMMIT）'
  } catch {
    /* 错误提示由后续 alert 展示 */
  }
}

async function doRollback(): Promise<void> {
  try {
    await store.rollback()
    store.executeMessage = '事务已回滚（ROLLBACK）'
  } catch {
    /* 错误提示由后续 alert 展示 */
  }
}

// ---------- 连接状态 ----------
const showConnForm = ref(false)

/** 断开当前连接 */
async function doDisconnect(): Promise<void> {
  try {
    await store.disconnect()
  } catch {
    /* 错误写入 store，由 v-alert 展示 */
  }
}

function onConnected(connLabel: string): void {
  // 连接成功后重置查询区（store 已自动加载表列表），并清空编辑态
  lastQuerySql.value = ''
  selectedTable.value = ''
  pendingEdits.value = new Map()
  selectedRows.value = new Set()
  editingCell.value = null
  ctxMenu.value = null
  pkColumn.value = ''
  void connLabel
}
</script>

<style scoped>
.mysql-grid {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.mysql-grid__body {
  display: flex;
  flex: 1 1 auto;
  min-height: 0;
}

/* 左侧表列表侧栏（Navicat 简化版） */
.mysql-grid__sidebar {
  width: 240px;
  flex: 0 0 240px;
  border-right: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.mysql-grid__table-list {
  flex: 1 1 auto;
  overflow-y: auto;
  min-height: 0;
}

/* 右侧主区：编辑区固定，结果区滚动 */
.mysql-grid__main {
  flex: 1 1 auto;
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  padding: 8px;
}

.mysql-grid__result {
  flex: 1 1 auto;
  overflow: auto;
  min-height: 0;
  margin-top: 8px;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
}

.mysql-grid__result-table {
  height: 100%;
}

/* SQL NULL：灰色斜体（深色主题下用主题 token 保证可读性） */
.mysql-grid__null {
  font-style: italic;
  opacity: 0.55;
  font-size: 12px;
}

/* 待提交修改中的 NULL：额外加下划线与警告色边框强调 */
.mysql-grid__null--edited {
  border-bottom: 1px dashed rgba(var(--v-theme-warning), 0.8);
}

/* 可编辑单元格：悬停提示可编辑；待提交修改：警告色左侧描边 */
.mysql-grid__cell--editable {
  cursor: pointer;
}

.mysql-grid__cell--editable:hover {
  outline: 1px solid rgba(var(--v-theme-primary), 0.4);
}

.mysql-grid__cell--edited {
  outline: 1px dashed rgba(var(--v-theme-warning), 0.8);
  background: rgba(var(--v-theme-warning), 0.08);
}

/* 选中行高亮 */
.mysql-grid__row--selected {
  background: rgba(var(--v-theme-primary), 0.08);
}

/* 单元格内联编辑输入框（原生 input，占满单元格） */
.mysql-grid__cell-input {
  width: 100%;
  border: 1px solid rgba(var(--v-theme-primary), 0.6);
  border-radius: 3px;
  padding: 2px 4px;
  font-size: 12px;
  background: transparent;
  color: rgb(var(--v-theme-on-surface));
  outline: none;
}

/* 右键菜单覆盖层与菜单本体（fixed 定位，跟随鼠标坐标） */
.mysql-grid__ctx-overlay {
  position: fixed;
  inset: 0;
  z-index: 2000;
}

.mysql-grid__ctx-menu {
  position: fixed;
  z-index: 2001;
  min-width: 180px;
}

/* 编辑预览对话框中的 SQL 片段（与危险确认框共用类名） */
.mysql-grid__sql-preview {
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
  padding: 6px 8px;
}

.mysql-grid__sql-preview-sql {
  font-family: 'Cascadia Mono', Consolas, monospace;
  font-size: 12px;
  word-break: break-all;
  white-space: pre-wrap;
}

.mysql-grid__placeholder {
  flex: 1 1 auto;
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 120px;
}

.mysql-grid__pager {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 8px;
}

.mysql-grid__pager .v-pagination {
  flex: 1 1 auto;
}
</style>
