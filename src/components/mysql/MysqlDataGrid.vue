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
        color="primary"
        prepend-icon="mdi-database-plus"
        :disabled="store.isConnected"
        :title="store.isConnected ? '已连接，请先断开' : '建立连接'"
        @click="showConnForm = true"
      >
        连接
      </v-btn>
      <v-btn
        size="small"
        variant="text"
        color="error"
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
        color="primary"
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
        color="primary"
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
      <div class="mysql-grid__sidebar" :style="{ width: sidebarWidth + 'px', flexBasis: sidebarWidth + 'px' }">
        <div class="d-flex align-center px-2 py-1">
          <div class="fy-field-row">
            <span class="fy-field-row__label">筛选表</span>
            <v-text-field
              v-model="tableFilter"
              density="compact"
              single-line
              hide-details
              clearable
              prepend-inner-icon="mdi-magnify"
              class="mr-1"
            />
          </div>
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
              @contextmenu.prevent="openTableCtx($event, t.name)"
            >
              <template #prepend>
                <v-icon size="small" color="primary">mdi-table</v-icon>
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

      <!-- 侧栏拖拽手柄：左右拖动调宽，双击恢复默认 -->
      <div
        class="mysql-grid__resizer"
        title="拖动调整宽度，双击恢复默认"
        @mousedown="startSideResize"
        @dblclick="sidebarWidth = 240"
      />

      <!-- 右侧：SQL 编辑区 + 结果网格 -->
      <div class="mysql-grid__main">
        <!-- SQL 编辑区 -->
        <div class="mysql-grid__editor">
          <div class="mysql-grid__editor-bar d-flex align-center mb-1">
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
            <v-btn
              size="small"
              variant="text"
              prepend-icon="mdi-content-save-outline"
              :disabled="!store.isConnected || !sql.trim()"
              title="将当前 SQL 保存为命名查询"
              @click="openSaveQuery"
            >
              保存查询
            </v-btn>
            <v-spacer />
            <!-- 事务按钮组 -->
            <v-btn
              size="small"
              variant="outlined"
              color="primary"
              prepend-icon="mdi-plus-circle-outline"
              :disabled="!store.isConnected || store.inTransaction"
              @click="doBegin"
            >
              开始事务
            </v-btn>
            <v-btn
              size="small"
              variant="outlined"
              color="success"
              prepend-icon="mdi-check"
              :disabled="!store.isConnected || !store.inTransaction"
              @click="doCommit"
            >
              提交事务
            </v-btn>
            <v-btn
              size="small"
              variant="outlined"
              color="error"
              prepend-icon="mdi-undo"
              :disabled="!store.isConnected || !store.inTransaction"
              @click="doRollback"
            >
              回滚事务
            </v-btn>
          </div>
          <SqlEditor
            v-model="sql"
            :tables="tableNames"
            placeholder="输入 SQL 语句…"
            @execute="runSql"
            @execute-selection="runSelection"
          />
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
        <div v-if="store.lastResult" class="mysql-grid__result-bar d-flex align-center mb-1">
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
          <v-btn size="small" variant="text" :disabled="!pendingEdits.size" @click="discardEdits">
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

        <!-- 结果网格：NULL 显示为灰色斜体；双击编辑，右键打开套件菜单；
             单击选中单元格（Shift+单击扩展矩形选区），列头单击选中整列 -->
        <div v-if="store.lastResult" ref="resultHost" class="mysql-grid__result">
          <v-table density="compact" fixed-header class="mysql-grid__result-table">
            <thead>
              <tr>
                <th class="mysql-grid__head-cell--locked" style="width: 36px; left: 0">
                  <!-- 行选择复选框列（sticky 冻结在左上角） -->
                </th>
                <th class="mysql-grid__head-cell--locked" style="width: 32px; left: 36px">
                  <!-- 行号 gutter（sticky 冻结，横向滚动时行号始终可见） -->
                </th>
                <th
                  v-for="(col, ci) in resultColumns"
                  :key="col"
                  class="text-left"
                  :class="{ 'mysql-grid__head-cell--locked': isLocked(ci) }"
                  :style="lockedStyle(ci)"
                  title="单击选中整列"
                  @click="selectColumn(ci)"
                >
                  <v-icon v-if="col === pkColumn" size="x-small" class="mr-1" title="主键列">mdi-key</v-icon>
                  <v-icon
                    v-if="sortState && sortState.ci === ci"
                    size="x-small"
                    class="mr-1"
                    :icon="sortState.dir === 'asc' ? 'mdi-sort-ascending' : 'mdi-sort-descending'"
                  />
                  {{ col }}
                </th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="row in displayRows"
                :key="row.originalIndex"
                :data-row-index="row.originalIndex"
                :class="{ 'mysql-grid__row--selected': selectedRows.has(row.originalIndex) }"
              >
                <td class="mysql-grid__cell--locked" style="left: 0">
                  <v-checkbox-btn
                    :model-value="selectedRows.has(row.originalIndex)"
                    density="compact"
                    hide-details
                    @update:model-value="(v: unknown) => toggleRow(row.originalIndex, v)"
                  />
                </td>
                <td class="mysql-grid__cell--locked mysql-grid__row-num" style="left: 36px">
                  <!-- 全局行号（Navicat 式 gutter）：服务端分页下 = (page-1)*pageSize + 页内索引 + 1 -->
                  {{ (page - 1) * pageSize + row.originalIndex + 1 }}
                </td>
                <td
                  v-for="(cell, ci) in row.cells"
                  :key="ci"
                  class="mysql-grid__cell"
                  :class="{
                    'mysql-grid__cell--edited': isEdited(row.originalIndex, ci),
                    'mysql-grid__cell--editable': canEdit,
                    'mysql-grid__cell--selected': isCellSelected(row.originalIndex, ci),
                    'mysql-grid__cell--locked': isLocked(ci),
                  }"
                  :style="lockedStyle(ci)"
                  title="双击编辑；右键打开操作菜单"
                  @click="onCellClick($event, row.originalIndex, ci)"
                  @dblclick="startEdit(row.originalIndex, ci)"
                  @contextmenu.prevent="openCtxMenu($event, row.originalIndex, ci)"
                >
                  <input
                    v-if="editingCell && editingCell.ri === row.originalIndex && editingCell.ci === ci"
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
                      :class="{ 'mysql-grid__null--edited': isEdited(row.originalIndex, ci) }"
                    >NULL</span>
                    <template v-else>{{ cell }}</template>
                  </template>
                </td>
              </tr>
              <tr v-if="!displayRows.length">
                <!-- 表头含复选框列，共 columnCount + 1 列 -->
                <td :colspan="columnCount + 1" class="text-caption text-medium-emphasis text-center py-2">
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
          <span v-if="lastCellPos" class="text-caption text-medium-emphasis mr-2">
            行 {{ (page - 1) * pageSize + lastCellPos.ri + 1 }} · 列 {{ lastCellPos.ci + 1 }}
          </span>
          <v-pagination
            :model-value="page"
            :length="pageCount"
            :total-visible="7"
            size="small"
            density="comfortable"
            @update:model-value="onPageChange"
          />
          <div class="fy-field-row">
            <span class="fy-field-row__label">每页</span>
            <v-select
              :model-value="pageSize"
              :items="PAGE_SIZES"
              density="compact"
              single-line
              hide-details
              style="max-width: 90px"
              @update:model-value="onPageSizeChange"
            />
          </div>
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

    <!-- 单元格右键菜单套件：编辑 / 复制为 / 填充 / 跳转行 / 排序 / 全选 / 列锁定 / 粘贴 / 行操作
         （fixed 定位，覆盖层负责点击关闭；v-list-group 内联展开子菜单） -->
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
          <v-list-item
            prepend-icon="mdi-pencil"
            :disabled="!canEdit"
            @click="ctxAction('edit')"
          >编辑单元格</v-list-item>
          <v-list-item
            prepend-icon="mdi-null"
            :disabled="!canEdit"
            @click="ctxAction('null')"
          >设为 NULL</v-list-item>
          <v-list-item
            prepend-icon="mdi-eraser"
            :disabled="!canEdit"
            @click="ctxAction('clear')"
          >清除 NULL（写空串）</v-list-item>

          <!-- 复制为：基于选中区域生成 SQL 文本并复制到剪贴板 -->
          <v-list-group value="copyAs">
            <template #activator="{ props: act }">
              <v-list-item v-bind="act" prepend-icon="mdi-content-copy" title="复制为" />
            </template>
            <v-list-item title="Where 条件" @click="ctxAction('copyWhere')" />
            <v-list-item title="InsertSQL（单条）" @click="ctxAction('copyInsert')" />
            <v-list-item title="InsertSQL（批量）" @click="ctxAction('copyInsertBatch')" />
            <v-list-item title="InsertOrUpdateSQL" @click="ctxAction('copyInsertOrUpdate')" />
            <v-list-item title="UpdateSQL" @click="ctxAction('copyUpdate')" />
            <v-list-item title="DeleteSQL" @click="ctxAction('copyDelete')" />
            <v-list-item title="表格文本（字段和数据）" @click="ctxAction('copyTextAll')" />
            <v-list-item title="表格文本（仅数据）" @click="ctxAction('copyTextData')" />
            <v-list-item title="表格文本（仅字段）" @click="ctxAction('copyTextHeader')" />
          </v-list-group>

          <!-- 填充：作用于选中区域（无选区时仅当前右键单元格） -->
          <v-list-group value="fill">
            <template #activator="{ props: act }">
              <v-list-item v-bind="act" prepend-icon="mdi-arrow-expand-vertical" title="填充" />
            </template>
            <v-list-item :disabled="!canEdit" title="填充 NULL" @click="ctxAction('fillNull')" />
            <v-list-item :disabled="!canEdit" title="填充当前日期时间" @click="ctxAction('fillNow')" />
            <v-list-item :disabled="!canEdit" title="填充当前日期" @click="ctxAction('fillDate')" />
            <v-list-item :disabled="!canEdit" title="填充 UUID" @click="ctxAction('fillUuid')" />
            <v-list-item :disabled="!canEdit" title="自定义…" @click="ctxAction('fillCustom')" />
          </v-list-group>

          <!-- 跳转行：服务端分页导航 -->
          <v-list-group value="goto">
            <template #activator="{ props: act }">
              <v-list-item v-bind="act" prepend-icon="mdi-arrow-up-down" title="跳转行" />
            </template>
            <v-list-item title="顶部" @click="ctxAction('gotoTop')" />
            <v-list-item title="底部" @click="ctxAction('gotoBottom')" />
            <v-list-item title="自定义行号…" @click="ctxAction('gotoRow')" />
          </v-list-group>

          <!-- 排序：对已加载页数据排序（右键单元格所在列） -->
          <v-list-group value="sort">
            <template #activator="{ props: act }">
              <v-list-item v-bind="act" prepend-icon="mdi-sort" title="排序" />
            </template>
            <v-list-item title="升序" @click="ctxAction('sortAsc')" />
            <v-list-item title="降序" @click="ctxAction('sortDesc')" />
            <v-list-item title="删除排序" @click="ctxAction('sortClear')" />
          </v-list-group>

          <!-- 全选 -->
          <v-list-group value="selectAll">
            <template #activator="{ props: act }">
              <v-list-item v-bind="act" prepend-icon="mdi-select-all" title="全选" />
            </template>
            <v-list-item title="行全选" @click="ctxAction('selectAllRows')" />
            <v-list-item title="列全选" @click="ctxAction('selectAllCols')" />
            <v-list-item title="取消选择" @click="ctxAction('clearSelection')" />
          </v-list-group>

          <!-- 列锁定/解锁：sticky 列冻结 -->
          <v-list-item
            prepend-icon="mdi-table-column"
            @click="ctxAction('toggleLock')"
          >{{ isLocked(ctxMenu.ci) ? '解锁此列' : '锁定到此列' }}</v-list-item>

          <!-- 粘贴：从剪贴板 TSV 解析 -->
          <v-list-group value="paste">
            <template #activator="{ props: act }">
              <v-list-item v-bind="act" prepend-icon="mdi-content-paste" title="粘贴" />
            </template>
            <v-list-item :disabled="!canEdit" title="粘贴到单元格" @click="ctxAction('pasteCell')" />
            <v-list-item :disabled="!canEdit" title="新建并粘贴…" @click="ctxAction('pasteNewRow')" />
          </v-list-group>

          <!-- 行操作：克隆行 / 插入 N 行（预览 -> 确认 -> 执行） -->
          <v-list-group value="rowOps">
            <template #activator="{ props: act }">
              <v-list-item v-bind="act" prepend-icon="mdi-table-row" title="行操作" />
            </template>
            <v-list-item :disabled="!canEdit" title="克隆行" @click="ctxAction('cloneRow')" />
            <v-list-item :disabled="!canEdit" title="插入 1 行" @click="ctxAction('insertRows1')" />
            <v-list-item :disabled="!canEdit" title="插入 3 行" @click="ctxAction('insertRows3')" />
            <v-list-item :disabled="!canEdit" title="插入 5 行" @click="ctxAction('insertRows5')" />
            <v-list-item :disabled="!canEdit" title="插入 10 行" @click="ctxAction('insertRows10')" />
          </v-list-group>
        </v-list>
      </v-card>
    </template>

    <!-- 表右键菜单套件（Navicat 表对象菜单）：打开/设计/删除/清空/截断/复制表/转储/打印/维护/逆向模型等
         （与单元格右键菜单同款 fixed 定位 + 覆盖层关闭） -->
    <template v-if="tableCtx">
      <div
        class="mysql-grid__ctx-overlay"
        @click="tableCtx = null"
        @contextmenu.prevent="tableCtx = null"
      />
      <v-card
        class="mysql-grid__ctx-menu mysql-grid__ctx-menu--table"
        :style="{ top: `${tableCtx.y}px`, left: `${tableCtx.x}px` }"
      >
        <v-list density="compact" nav>
          <v-list-item prepend-icon="mdi-table-arrow-right" @click="withCtxTable(openTable)">打开表</v-list-item>
          <v-list-item prepend-icon="mdi-pencil" @click="withCtxTable(designTable)">设计表</v-list-item>
          <v-list-item prepend-icon="mdi-table-plus" :disabled="!store.isConnected" @click="withCtxTable(() => openDesigner('', true))">新建表</v-list-item>
          <v-list-item prepend-icon="mdi-table-remove" :disabled="!store.isConnected" @click="withCtxTable(dropTable)">删除表</v-list-item>
          <v-list-item prepend-icon="mdi-eraser" :disabled="!store.isConnected" @click="withCtxTable(clearTable)">清空表</v-list-item>
          <v-list-item prepend-icon="mdi-arrow-collapse-vertical" :disabled="!store.isConnected" @click="withCtxTable(truncateTable)">截断表</v-list-item>

          <!-- 复制表：结构 / 结构和数据（新表名对话框） -->
          <v-list-group value="tcopy">
            <template #activator="{ props: act }">
              <v-list-item v-bind="act" prepend-icon="mdi-content-copy" title="复制表" />
            </template>
            <v-list-item title="复制表结构" @click="withCtxTable(copyStructure)" />
            <v-list-item title="复制表结构和数据" @click="withCtxTable(copyData)" />
          </v-list-group>

          <v-list-item disabled>
            <v-list-item-title title="开发中">设置权限</v-list-item-title>
          </v-list-item>

          <v-list-item prepend-icon="mdi-file-import-outline" :disabled="!store.isConnected" @click="withCtxTable(openImportWizard)">导入向导...</v-list-item>
          <v-list-item prepend-icon="mdi-file-export-outline" :disabled="!store.isConnected" @click="withCtxTable(openExportWizard)">导出向导...</v-list-item>
          <v-list-item disabled>
            <v-list-item-title title="开发中">数据生成...</v-list-item-title>
          </v-list-item>

          <!-- 转储 SQL 文件：预设 SELECT 当前表 + includeCreateTable（与库级转储同款语义） -->
          <v-list-group value="tdump">
            <template #activator="{ props: act }">
              <v-list-item v-bind="act" prepend-icon="mdi-database-export" title="转储 SQL 文件" />
            </template>
            <v-list-item title="结构和数据" @click="withCtxTable(dumpStructureData)" />
            <v-list-item title="仅结构" @click="withCtxTable(dumpStructureOnly)" />
          </v-list-group>

          <v-list-item prepend-icon="mdi-printer" :disabled="!store.isConnected" @click="withCtxTable(printTable)">打印表</v-list-item>

          <!-- 维护：OPTIMIZE/ANALYZE/CHECK/REPAIR TABLE -->
          <v-list-group value="tmaintain">
            <template #activator="{ props: act }">
              <v-list-item v-bind="act" prepend-icon="mdi-wrench" title="维护" />
            </template>
            <v-list-item title="优化表" @click="withCtxTable((n) => maintainTable(n, 'OPTIMIZE'))" />
            <v-list-item title="分析表" @click="withCtxTable((n) => maintainTable(n, 'ANALYZE'))" />
            <v-list-item title="检查表" @click="withCtxTable((n) => maintainTable(n, 'CHECK'))" />
            <v-list-item title="修复表" @click="withCtxTable((n) => maintainTable(n, 'REPAIR'))" />
          </v-list-group>

          <v-list-item prepend-icon="mdi-file-tree-outline" :disabled="!store.isConnected" @click="withCtxTable(openErModel)">逆向表到模型...</v-list-item>
          <v-list-item disabled>
            <v-list-item-title title="开发中">创建图表...</v-list-item-title>
          </v-list-item>
          <v-list-item disabled>
            <v-list-item-title title="开发中">管理组...</v-list-item-title>
          </v-list-item>

          <v-list-item prepend-icon="mdi-content-copy" @click="withCtxTable(copyTableName)">复制</v-list-item>
          <v-list-item prepend-icon="mdi-rename-box" :disabled="!store.isConnected" @click="withCtxTable(renameTable)">重命名</v-list-item>
          <v-list-item disabled>
            <v-list-item-title title="开发中">创建打开表快捷方式...</v-list-item-title>
          </v-list-item>

          <v-list-item prepend-icon="mdi-refresh" @click="withCtxTable(() => refreshTables())">刷新</v-list-item>
        </v-list>
      </v-card>
    </template>

    <!-- 自定义填充值对话框：对选中区域内所有单元格填充该值 -->
    <v-dialog v-model="showFillDialog" width="420">
      <v-card>
        <v-card-title class="d-flex align-center">
          <v-icon size="small" class="mr-2">mdi-arrow-expand-vertical</v-icon>
          自定义填充值
        <v-spacer />
        <v-btn
          icon="mdi-close"
          size="x-small"
          variant="text"
          title="关闭"
          @click="showFillDialog = false"
        />
        </v-card-title>
        <v-divider />
        <v-card-text>
          <div class="fy-field-row">
            <span class="fy-field-row__label">填充值（选中区域内所有单元格）</span>
            <v-text-field
              v-model="fillValue"
              density="compact"
              single-line
              hide-details
              autofocus
              @keyup.enter="confirmFill"
            />
          </div>
        </v-card-text>
        <v-divider />
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="showFillDialog = false">取消</v-btn>
          <v-btn color="primary" @click="confirmFill">确认</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- 跳转行对话框：跳转到结果集中指定行号（按每页行数换算页码） -->
    <v-dialog v-model="showGotoDialog" width="420">
      <v-card>
        <v-card-title class="d-flex align-center">
          <v-icon size="small" class="mr-2">mdi-arrow-up-down</v-icon>
          跳转到行
        <v-spacer />
        <v-btn
          icon="mdi-close"
          size="x-small"
          variant="text"
          title="关闭"
          @click="showGotoDialog = false"
        />
        </v-card-title>
        <v-divider />
        <v-card-text>
          <div class="fy-field-row">
            <span class="fy-field-row__label">行号（1 - 总行数）</span>
            <v-text-field
              v-model="gotoRowNo"
              type="number"
              density="compact"
              single-line
              hide-details
              autofocus
              @keyup.enter="confirmGoto"
            />
          </div>
        </v-card-text>
        <v-divider />
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="showGotoDialog = false">取消</v-btn>
          <v-btn color="primary" @click="confirmGoto">跳转</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- 编辑预览对话框：展示将执行的 SQL 与估算影响行数，确认后执行 -->
    <v-dialog v-model="showPreview" width="640">
      <v-card>
        <v-card-title class="d-flex align-center">
          <v-icon size="small" class="mr-2">mdi-eye-outline</v-icon>
          {{ previewMode === 'edit' ? '编辑预览' : previewMode === 'delete' ? '删除预览' : '插入预览' }}
        <v-spacer />
        <v-btn
          icon="mdi-close"
          size="x-small"
          variant="text"
          title="关闭"
          @click="showPreview = false"
        />
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
    <ImportExportDialog
      v-model="showIo"
      :conn-id="store.connId ?? ''"
      :initial-mode="ioModePreset ?? undefined"
      :initial-export-sql="ioExportSqlPreset ?? undefined"
      :initial-import-table="ioImportTablePreset ?? undefined"
    />
    <ErModelDialog v-model="showEr" :conn-id="store.connId ?? ''" :db-name="erDbName" :table-name="erTableName" />

    <!-- 复制表/重命名对话框：新表名输入（复制表结构/结构和数据/重命名共用） -->
    <v-dialog v-model="showCopyDialog" width="420">
      <v-card>
        <v-card-title class="d-flex align-center text-subtitle-1">
          {{ copyMode === 'rename' ? '重命名表' : '复制表' }}
          <v-spacer />
          <v-btn icon="mdi-close" size="x-small" variant="text" title="关闭" @click="showCopyDialog = false" />
        </v-card-title>
        <v-divider />
        <v-card-text>
          <div class="text-caption text-medium-emphasis mb-1">
            {{ copyMode === 'rename' ? `将表「${copySource}」重命名为：` : `源表：${copySource}` }}
          </div>
          <v-text-field
            v-model="copyName"
            density="compact"
            variant="outlined"
            single-line
            hide-details
            autofocus
            label="新表名"
            @keyup.enter="confirmCopy"
          />
        </v-card-text>
        <v-divider />
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="showCopyDialog = false">取消</v-btn>
          <v-btn color="primary" :loading="copyRunning" @click="confirmCopy">确认</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- 保存查询对话框：命名保存当前 SQL（可选绑定当前连接） -->
    <v-dialog v-model="showSaveQuery" max-width="420">
      <v-card>
        <v-card-title class="d-flex align-center text-subtitle-1">保存查询
          <v-spacer />
          <v-btn
          icon="mdi-close"
          size="x-small"
          variant="text"
          title="关闭"
          @click="showSaveQuery = false"
          />
        </v-card-title>
        <v-card-text>
          <div class="fy-field-row">
            <span class="fy-field-row__label">查询名称</span>
            <v-text-field
              v-model="saveQueryName"
              density="compact"
              variant="outlined"
              autofocus
              counter="100"
              @keyup.enter="confirmSaveQuery"
            />
          </div>
          <v-checkbox-btn
            v-model="saveBindConn"
            density="compact"
            label="绑定当前连接（仅该连接可见）"
            hide-details
          />
          <div class="mysql-grid__sql-preview text-caption text-medium-emphasis mt-2">
            {{ saveQueryPreview }}
          </div>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="showSaveQuery = false">取消</v-btn>
          <v-btn color="primary" prepend-icon="mdi-check" :loading="savingQuery" @click="confirmSaveQuery">
            保存
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </v-card>
</template>

<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { useMysqlStore } from '@/stores/mysql'
import { mysqlQuery } from '@/api/mysql'
import type { MySqlQueryResult } from '@/api/types'
import { useUiStore } from '@/stores/ui'
import {
  mysqlDeleteRow,
  mysqlEditPreview,
  mysqlInsertRows,
  mysqlTableDesignGet,
  mysqlUpdateRows,
} from '@/api/mysqlEdit'
import type { MySqlRowUpdate } from '@/api/mysqlEdit'
import { mysqlSavedQueryList, mysqlSavedQuerySave } from '@/api/mysqlConsole'
import MysqlConnectionForm from './MysqlConnectionForm.vue'
import SqlEditor from './SqlEditor.vue'
import { splitSqlStatements } from './sql-format'
import TableDesigner from '@/views/mysql/TableDesigner.vue'
import HistoryDrawer from './HistoryDrawer.vue'
import ExplainPanel from './ExplainPanel.vue'
import ImportExportDialog from './ImportExportDialog.vue'
import ErModelDialog from './ErModelDialog.vue'

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

// ---------- 已保存查询：命名保存当前 SQL ----------
const showSaveQuery = ref(false)
const saveQueryName = ref('')
const saveBindConn = ref(true)
const savingQuery = ref(false)

/** 对话框内的 SQL 单行摘要（预览用） */
const saveQueryPreview = computed(() => {
  const text = sql.value.trim()
  return text.length > 60 ? `${text.slice(0, 60)}…` : text || '（当前 SQL 为空）'
})

function openSaveQuery(): void {
  showSaveQuery.value = true
}

/** 保存当前 SQL 为命名查询：重名时经 uiStore.confirm 覆盖确认（后端 UPSERT 保留原 id） */
async function confirmSaveQuery(): Promise<void> {
  const name = saveQueryName.value.trim()
  if (!name) {
    ui.toast('请输入查询名称', 'warning')
    return
  }
  const text = sql.value.trim()
  if (!text) {
    ui.toast('当前 SQL 为空，无法保存', 'warning')
    return
  }
  savingQuery.value = true
  try {
    // 同名覆盖确认：先查列表判重（本地 SQLite，开销可忽略）
    const saved = await mysqlSavedQueryList()
    let willOverwrite = false
    if (saved.some((q) => q.name === name)) {
      const ok = await ui.confirm({
        title: '覆盖确认',
        message: `已存在同名查询「${name}」，确定覆盖吗？`,
        confirmText: '覆盖',
      })
      if (!ok) return
      willOverwrite = true
    }
    // 覆盖时带 overwrite=true（后端 exists>0 && !overwrite 会 reject，确认后必须传）
    const overwritten = await mysqlSavedQuerySave(
      name,
      saveBindConn.value ? (store.connId ?? null) : null,
      text,
      willOverwrite,
    )
    ui.toast(overwritten ? `已覆盖同名查询：${name}` : `已保存查询：${name}`, 'success')
    showSaveQuery.value = false
  } catch (err) {
    ui.toast(errText(err), 'error')
  } finally {
    savingQuery.value = false
  }
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

/** 点击表：选中并填入 SELECT 语句；400ms 内重复点击视为双击，自动执行该表查询。
    （v-list-item 不透传 dblclick 事件，双击检测放在 click 内按时间戳判断） */
let lastClickName = ''
let lastClickAt = 0
function selectTable(name: string): void {
  selectedTable.value = name
  sql.value = `SELECT * FROM \`${name}\``
  const now = Date.now()
  const isDblClick = lastClickName === name && now - lastClickAt < 400
  lastClickName = name
  lastClickAt = now
  if (isDblClick) void runQuery(1, pageSize.value, sql.value)
}

/** 侧栏宽度可拖拽（Navicat 风格：拖手柄调宽，双击恢复默认 240px） */
const sidebarWidth = ref(240)

/** 按下手柄开始拖拽：跟随鼠标横移更新侧栏宽度（160-420px），松开结束 */
function startSideResize(e: MouseEvent): void {
  e.preventDefault()
  const startX = e.clientX
  const startW = sidebarWidth.value
  const onMove = (ev: MouseEvent): void => {
    sidebarWidth.value = Math.min(420, Math.max(160, startW + (ev.clientX - startX)))
  }
  const onUp = (): void => {
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
}

// ---------- 表右键菜单（Navicat 表对象菜单） ----------

/** 表右键菜单状态：屏幕坐标 + 目标表名 */
const tableCtx = ref<{ x: number; y: number; name: string } | null>(null)

/** 打开表右键菜单（坐标钳制到视口内；菜单含多组子菜单，预留足够高度） */
function openTableCtx(e: MouseEvent, name: string): void {
  tableCtx.value = {
    x: Math.min(e.clientX, window.innerWidth - 220),
    y: Math.min(e.clientY, window.innerHeight - 460),
    name,
  }
}

/** 菜单动作统一入口：读取目标表名 → 关闭菜单 → 执行 */
function withCtxTable(fn: (name: string) => void): void {
  const name = tableCtx.value?.name
  tableCtx.value = null
  if (name) fn(name)
}

/** 打开表：选中并填入 SELECT（与单击列表项一致） */
function openTable(name: string): void {
  selectTable(name)
}

/** 复制：表名到剪贴板 */
function copyTableName(name: string): void {
  void navigator.clipboard
    .writeText(name)
    .then(() => ui.toast(`表名「${name}」已复制到剪贴板`, 'success'))
    .catch((e) => ui.toast(`复制失败：${errText(e)}`, 'error'))
}

/** 设计表：打开设计器编辑该表 */
function designTable(name: string): void {
  openDesigner(name, false)
}

/** 删除/清空/截断表：执行表级 SQL（危险 SQL 由 store.pendingConfirm 确认框二次确认） */
function dropTable(name: string): void {
  void execTableSql(`DROP TABLE \`${name}\``, `表「${name}」已删除`, { clearSelected: true })
}

function clearTable(name: string): void {
  void execTableSql(`DELETE FROM \`${name}\``, `表「${name}」已清空`)
}

function truncateTable(name: string): void {
  void execTableSql(`TRUNCATE TABLE \`${name}\``, `表「${name}」已截断`)
}

/** 执行表级 SQL：needsConfirm=true 时确认框由 store 驱动弹出，此处只返回不报成功 */
async function execTableSql(
  sqlText: string,
  successMsg: string,
  opts?: { clearSelected?: boolean },
): Promise<void> {
  try {
    const outcome = await store.executeSql(sqlText)
    if (outcome.needsConfirm) return
    ui.toast(successMsg, 'success')
    if (opts?.clearSelected && selectedTable.value) selectedTable.value = ''
    await refreshTables()
  } catch (e) {
    ui.toast(errText(e), 'error')
  }
}

/** 维护：OPTIMIZE/ANALYZE/CHECK/REPAIR TABLE（非危险语句直接执行） */
function maintainTable(name: string, op: 'OPTIMIZE' | 'ANALYZE' | 'CHECK' | 'REPAIR'): void {
  void execTableSql(`${op} TABLE \`${name}\``, `表「${name}」${op} 完成`)
}

// 复制表 / 重命名：新表名对话框（structure=仅结构 data=含数据 rename=重命名）
type CopyMode = 'structure' | 'data' | 'rename'
const showCopyDialog = ref(false)
const copyMode = ref<CopyMode>('structure')
const copySource = ref('')
const copyName = ref('')
const copyRunning = ref(false)

function openCopyDialog(mode: CopyMode, name: string): void {
  copyMode.value = mode
  copySource.value = name
  copyName.value = mode === 'rename' ? name : `${name}_copy`
  showCopyDialog.value = true
}

function copyStructure(name: string): void {
  openCopyDialog('structure', name)
}

function copyData(name: string): void {
  openCopyDialog('data', name)
}

function renameTable(name: string): void {
  openCopyDialog('rename', name)
}

/** 确认复制/重命名：CREATE TABLE LIKE（+ INSERT SELECT）/ RENAME TABLE */
async function confirmCopy(): Promise<void> {
  const newName = copyName.value.trim()
  if (!newName || newName.includes('`')) {
    ui.toast('请填写合法的表名（不含反引号）', 'warning')
    return
  }
  copyRunning.value = true
  try {
    if (copyMode.value === 'rename') {
      await store.executeSql(`RENAME TABLE \`${copySource.value}\` TO \`${newName}\``)
      ui.toast(`表「${copySource.value}」已重命名为「${newName}」`, 'success')
    } else {
      await store.executeSql(`CREATE TABLE \`${newName}\` LIKE \`${copySource.value}\``)
      if (copyMode.value === 'data') {
        await store.executeSql(`INSERT INTO \`${newName}\` SELECT * FROM \`${copySource.value}\``)
      }
      ui.toast(
        copyMode.value === 'data'
          ? `已复制表「${copySource.value}」到「${newName}」（含数据）`
          : `已复制表「${copySource.value}」到「${newName}」（仅结构）`,
        'success',
      )
    }
    showCopyDialog.value = false
    await refreshTables()
  } catch (e) {
    ui.toast(errText(e), 'error')
  } finally {
    copyRunning.value = false
  }
}

// ---------- 表右键：导入/导出向导 + 转储 SQL 文件 ----------

/** 向导打开预设（null = 不预设；表右键菜单入口设置，工具条导入导出按钮保持 null 不干扰已填 SQL） */
const ioModePreset = ref<'export' | 'import' | null>(null)
const ioExportSqlPreset = ref<string | null>(null)
const ioImportTablePreset = ref<string | null>(null)

function openImportWizard(name: string): void {
  ioModePreset.value = 'import'
  ioExportSqlPreset.value = null
  ioImportTablePreset.value = name
  showIo.value = true
}

function openExportWizard(name: string): void {
  ioModePreset.value = 'export'
  ioExportSqlPreset.value = null
  ioImportTablePreset.value = null
  showIo.value = true
}

/** 转储 SQL 文件：预设 SELECT 当前表（结构和数据=带建表语句 / 仅结构），与库级转储同款语义 */
function dumpStructureData(name: string): void {
  ioModePreset.value = 'export'
  ioExportSqlPreset.value = `SELECT * FROM \`${name}\``
  ioImportTablePreset.value = null
  showIo.value = true
}

function dumpStructureOnly(name: string): void {
  dumpStructureData(name)
}

// 向导关闭后清除预设（避免下次工具条打开时误用上次菜单的预设）
watch(showIo, (open) => {
  if (!open) {
    ioModePreset.value = null
    ioExportSqlPreset.value = null
    ioImportTablePreset.value = null
  }
})

// ---------- 表右键：打印表 / 逆向表到模型 ----------

/** SQL 字符串字面量（单引号翻倍转义） */
function sqlStr(value: string): string {
  return `'${value.replace(/'/g, "''")}'`
}

/** 右键菜单"打印表"：查列定义生成单表结构报告并调起 WebView 打印 */
async function printTable(name: string): Promise<void> {
  const connId = store.connId
  if (!connId) return
  try {
    const colResult = await mysqlQuery(
      connId,
      `SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_KEY, COLUMN_COMMENT FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ${sqlStr(name)} ORDER BY ORDINAL_POSITION`,
      1,
      100000,
    )
    printTableReport(name, colResult)
  } catch (e) {
    ui.toast(`打印表失败：${errText(e)}`, 'error')
  }
}

/** 拼单表结构报告 HTML（打印表）：表名/行数/引擎/注释 + 列定义表格 */
function printTableReport(name: string, colResult: MySqlQueryResult): string {
  const esc = (s: string): string =>
    s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  const info = store.tables.find((t) => t.name === name)
  const colRows = colResult.rows
    .map((row) => {
      const nullable = row[2] === 'YES' ? 'NULL' : 'NOT NULL'
      const key = row[3] === 'PRI' ? ' PRI' : row[3] === 'UNI' ? ' UK' : row[3] === 'MUL' ? ' MUL' : ''
      const line = `${row[0] ?? ''}  ${row[1] ?? ''}  ${nullable}${key}${row[4] ? `  ${row[4]}` : ''}`
      return `<tr><td>${esc(line)}</td></tr>`
    })
    .join('')
  return `<!DOCTYPE html>
<html lang="zh-CN"><head><meta charset="utf-8"><title>表结构 - ${esc(name)}</title>
<style>
  body { font-family: "Microsoft YaHei", sans-serif; font-size: 12px; color: #222; margin: 24px; }
  h1 { font-size: 16px; margin: 0 0 4px; }
  .gen { color: #666; margin: 0 0 14px; }
  table { border-collapse: collapse; width: 100%; }
  th, td { border: 1px solid #bbb; padding: 3px 8px; text-align: left; }
  th { background: #f0f0f0; }
</style></head><body>
<h1>表结构：${esc(name)}</h1>
<p class="gen">生成时间：${new Date().toLocaleString('zh-CN')}　行数（预估）：${info?.rows ?? 0}　引擎：${esc(info?.engine ?? '-')}${info?.comment ? `　注释：${esc(info.comment)}` : ''}</p>
<table><thead><tr><th>列定义</th></tr></thead><tbody>${colRows}</tbody></table>
</body></html>`
}

/** 隐藏 iframe 写入 HTML 并调起打印（WebView2 内打印，不弹新窗口） */
function printHtml(html: string): void {
  const iframe = document.createElement('iframe')
  iframe.style.position = 'fixed'
  iframe.style.width = '0'
  iframe.style.height = '0'
  iframe.style.border = 'none'
  document.body.appendChild(iframe)
  const doc = iframe.contentDocument
  if (!doc) {
    document.body.removeChild(iframe)
    return
  }
  doc.open()
  doc.write(html)
  doc.close()
  iframe.contentWindow?.focus()
  iframe.contentWindow?.print()
  window.setTimeout(() => document.body.removeChild(iframe), 60_000)
}

// 逆向表到模型：单表模式 ER 图对话框
const showEr = ref(false)
const erDbName = ref('')
const erTableName = ref('')

/** 右键菜单"逆向表到模型..."：查当前库名后打开 ER 图对话框（单表模式，含直接外键关系） */
async function openErModel(name: string): Promise<void> {
  const connId = store.connId
  if (!connId) return
  try {
    const r = await mysqlQuery(connId, 'SELECT DATABASE() AS db', 1, 10)
    const dbName = r.rows[0]?.[0] ?? ''
    if (!dbName) {
      ui.toast('当前连接未选择数据库', 'warning')
      return
    }
    erDbName.value = dbName
    erTableName.value = name
    showEr.value = true
  } catch (e) {
    ui.toast(`打开表模型失败：${errText(e)}`, 'error')
  }
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
  const cell = displayCell(ri, ci)
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

/** 打开右键菜单（坐标钳制到视口内；菜单含多组子菜单，预留足够高度） */
function openCtxMenu(e: MouseEvent, ri: number, ci: number): void {
  if (!store.lastResult) return
  ctxMenu.value = {
    x: Math.min(e.clientX, window.innerWidth - 220),
    y: Math.min(e.clientY, window.innerHeight - 320),
    ri,
    ci,
  }
}

// ---------- 单元格选区（矩形，含端点） ----------

/** 选区锚点（Shift+单击扩展矩形选区） */
const selAnchor = ref<{ ri: number; ci: number } | null>(null)

/** 选中区域（矩形范围，含端点；null = 无选区） */
const cellSelection = ref<{ r1: number; c1: number; r2: number; c2: number } | null>(null)

/** 单元格是否在选中区域内 */
function isCellSelected(ri: number, ci: number): boolean {
  const s = cellSelection.value
  if (!s) return false
  return (
    ri >= Math.min(s.r1, s.r2) &&
    ri <= Math.max(s.r1, s.r2) &&
    ci >= Math.min(s.c1, s.c2) &&
    ci <= Math.max(s.c1, s.c2)
  )
}

/** 单击单元格：设为锚点；Shift+单击扩展矩形选区 */
/** 当前点击单元格位置（状态条"行 X · 列 Y"展示；查询变化时随选区清除） */
const lastCellPos = ref<{ ri: number; ci: number } | null>(null)

function onCellClick(e: MouseEvent, ri: number, ci: number): void {
  const anchor = selAnchor.value
  if (e.shiftKey && anchor) {
    cellSelection.value = {
      r1: Math.min(anchor.ri, ri),
      c1: Math.min(anchor.ci, ci),
      r2: Math.max(anchor.ri, ri),
      c2: Math.max(anchor.ci, ci),
    }
  } else {
    selAnchor.value = { ri, ci }
    cellSelection.value = { r1: ri, c1: ci, r2: ri, c2: ci }
  }
  lastCellPos.value = { ri, ci }
}

/** 列头单击：选中整列（全行 × 该列） */
function selectColumn(ci: number): void {
  selAnchor.value = { ri: 0, ci }
  const last = Math.max(0, (store.lastResult?.rows.length ?? 1) - 1)
  cellSelection.value = { r1: 0, c1: ci, r2: last, c2: ci }
}

/** 清除选区与锚点 */
function clearCellSelection(): void {
  selAnchor.value = null
  cellSelection.value = null
  lastCellPos.value = null
}

/**
 * 解析复制的作用区域（列索引表示）：
 * 矩形选区优先，其次选中行 × 全列；均无时由调用方回退到右键单元格所在行
 */
function selectedRegion(fallbackRow?: number): { rows: number[]; cols: number[] } | null {
  const r = store.lastResult
  if (!r) return null
  const s = cellSelection.value
  if (s) {
    const rows: number[] = []
    for (let i = Math.min(s.r1, s.r2); i <= Math.max(s.r1, s.r2); i++) rows.push(i)
    const cols: number[] = []
    for (let j = Math.min(s.c1, s.c2); j <= Math.max(s.c1, s.c2); j++) cols.push(j)
    return { rows, cols }
  }
  if (selectedRows.value.size) {
    return { rows: [...selectedRows.value].sort((a, b) => a - b), cols: r.columns.map((_, i) => i) }
  }
  if (fallbackRow === undefined) return null
  return { rows: [fallbackRow], cols: r.columns.map((_, i) => i) }
}

/** 读取展示值（含待提交修改叠加）：原始行索引 + 列索引 */
function displayCell(ri: number, ci: number): string | null {
  const r = store.lastResult
  if (!r) return null
  const row = r.rows[ri]
  if (!row) return null
  const edit = pendingEdits.value.get(`${ri}:${r.columns[ci]}`)
  if (edit) return edit.isNull ? null : (edit.value ?? '')
  return row[ci] ?? null
}

// ---------- 右键菜单动作分发 ----------

/** 全部右键菜单动作 */
type CtxAction =
  | 'edit'
  | 'null'
  | 'clear'
  | 'copyWhere'
  | 'copyInsert'
  | 'copyInsertBatch'
  | 'copyInsertOrUpdate'
  | 'copyUpdate'
  | 'copyDelete'
  | 'copyTextAll'
  | 'copyTextData'
  | 'copyTextHeader'
  | 'fillNull'
  | 'fillNow'
  | 'fillDate'
  | 'fillUuid'
  | 'fillCustom'
  | 'gotoTop'
  | 'gotoBottom'
  | 'gotoRow'
  | 'sortAsc'
  | 'sortDesc'
  | 'sortClear'
  | 'selectAllRows'
  | 'selectAllCols'
  | 'clearSelection'
  | 'toggleLock'
  | 'pasteCell'
  | 'pasteNewRow'
  | 'cloneRow'
  | 'insertRows1'
  | 'insertRows3'
  | 'insertRows5'
  | 'insertRows10'

/** 右键菜单动作分发（先捕获菜单目标，再关闭菜单并执行） */
function ctxAction(action: CtxAction): void {
  const menu = ctxMenu.value
  ctxMenu.value = null
  if (!menu) return
  switch (action) {
    case 'edit':
      startEdit(menu.ri, menu.ci)
      break
    case 'null':
      setCellNull(menu.ri, menu.ci)
      break
    case 'clear':
      clearCellNull(menu.ri, menu.ci)
      break
    case 'copyWhere':
      void copyAs('where', menu.ri)
      break
    case 'copyInsert':
      void copyAs('insert', menu.ri)
      break
    case 'copyInsertBatch':
      void copyAs('insertBatch', menu.ri)
      break
    case 'copyInsertOrUpdate':
      void copyAs('insertOrUpdate', menu.ri)
      break
    case 'copyUpdate':
      void copyAs('update', menu.ri)
      break
    case 'copyDelete':
      void copyAs('delete', menu.ri)
      break
    case 'copyTextAll':
      void copyAs('textAll', menu.ri)
      break
    case 'copyTextData':
      void copyAs('textData', menu.ri)
      break
    case 'copyTextHeader':
      void copyAs('textHeader', menu.ri)
      break
    case 'fillNull':
      fillCells(null, true, menu.ri, menu.ci)
      break
    case 'fillNow':
      fillCells(nowDateTime(), false, menu.ri, menu.ci)
      break
    case 'fillDate':
      fillCells(nowDateTime().slice(0, 10), false, menu.ri, menu.ci)
      break
    case 'fillUuid':
      fillCells(genUuid(), false, menu.ri, menu.ci)
      break
    case 'fillCustom':
      fillValue.value = ''
      fillFallback.value = { ri: menu.ri, ci: menu.ci }
      showFillDialog.value = true
      break
    case 'gotoTop':
      void gotoTop()
      break
    case 'gotoBottom':
      void gotoBottom()
      break
    case 'gotoRow':
      gotoRowNo.value = ''
      showGotoDialog.value = true
      break
    case 'sortAsc':
      sortState.value = { ci: menu.ci, dir: 'asc' }
      break
    case 'sortDesc':
      sortState.value = { ci: menu.ci, dir: 'desc' }
      break
    case 'sortClear':
      sortState.value = null
      break
    case 'selectAllRows': {
      const set = new Set<number>()
      for (let i = 0; i < (store.lastResult?.rows.length ?? 0); i++) set.add(i)
      selectedRows.value = set
      break
    }
    case 'selectAllCols': {
      const rowsN = store.lastResult?.rows.length ?? 0
      const colsN = resultColumns.value.length
      if (!rowsN || !colsN) break
      selAnchor.value = { ri: 0, ci: 0 }
      cellSelection.value = { r1: 0, c1: 0, r2: rowsN - 1, c2: colsN - 1 }
      break
    }
    case 'clearSelection':
      clearCellSelection()
      break
    case 'toggleLock':
      toggleLock(menu.ci)
      break
    case 'pasteCell':
      void pasteToCell(menu.ri, menu.ci)
      break
    case 'pasteNewRow':
      void pasteNewRow()
      break
    case 'cloneRow':
      cloneRow(menu.ri)
      break
    case 'insertRows1':
      void insertDefaultRows(1)
      break
    case 'insertRows3':
      void insertDefaultRows(3)
      break
    case 'insertRows5':
      void insertDefaultRows(5)
      break
    case 'insertRows10':
      void insertDefaultRows(10)
      break
  }
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

// ---------- 排序：对已加载页数据的前端排序 ----------

/** 排序状态：列索引 + 方向（null = 未排序；查询/翻页时清除） */
const sortState = ref<{ ci: number; dir: 'asc' | 'desc' } | null>(null)

// ---------- 列锁定/解锁（sticky 列冻结） ----------

/** 锁定列名集合（左偏移按列顺序累计） */
const lockedCols = ref<string[]>([])

/** 结果区宿主元素（测量列宽用） */
const resultHost = ref<HTMLElement | null>(null)

/** 各列宽度（与 resultColumns 对齐，渲染后测量） */
const colWidths = ref<number[]>([])

/** 渲染后测量列宽（sticky 左偏移需要像素值） */
function measureColumns(): void {
  const el = resultHost.value
  if (!el) {
    colWidths.value = []
    return
  }
  // 第一列为行选择复选框列（固定 36px），不入宽度表
  const widths: number[] = []
  el.querySelectorAll<HTMLElement>('thead th').forEach((th, i) => {
    if (i > 0) widths.push(th.getBoundingClientRect().width)
  })
  colWidths.value = widths
}

/** 列是否已锁定 */
function isLocked(ci: number): boolean {
  const col = resultColumns.value[ci]
  return !!col && lockedCols.value.includes(col)
}

/** 锁定列的 sticky 样式：left = 36px（复选框列）+ 其前已锁列宽累计 */
function lockedStyle(ci: number): Record<string, string> {
  if (!isLocked(ci)) return {}
  const cols = resultColumns.value
  let left = 36
  for (let i = 0; i < ci && i < cols.length; i++) {
    if (lockedCols.value.includes(cols[i])) left += colWidths.value[i] ?? 0
  }
  return { position: 'sticky', left: `${left}px` }
}

/** 锁定/解锁指定列 */
function toggleLock(ci: number): void {
  const col = resultColumns.value[ci]
  if (!col) return
  const next = new Set(lockedCols.value)
  if (next.has(col)) next.delete(col)
  else next.add(col)
  lockedCols.value = [...next]
  void nextTick().then(measureColumns)
}

// ---------- 复制为：基于选中区域生成 SQL 文本并复制 ----------

/** SQL 字面量转义：单引号/反斜杠成对转义（仅用于展示与复制） */
function sqlLiteral(v: string | null): string {
  if (v === null) return 'NULL'
  return `'${v.replace(/\\/g, '\\\\').replace(/'/g, "''")}'`
}

/** 谓词片段：col IS NULL / col = 'val' */
function sqlPredicate(col: string, v: string | null): string {
  return v === null ? `\`${col}\` IS NULL` : `\`${col}\` = ${sqlLiteral(v)}`
}

/** 解析选中区域的数据：返回行数据（含待提交修改叠加）与列名 */
function selectedData(fallbackRow?: number): { rows: (string | null)[][]; cols: string[] } | null {
  const r = store.lastResult
  if (!r) return null
  const region = selectedRegion(fallbackRow)
  if (!region) return null
  const rows = region.rows.map((ri) => region.cols.map((ci) => displayCell(ri, ci)))
  return { rows, cols: region.cols.map((ci) => r.columns[ci]) }
}

/** TSV 文本：列名 + 数据行（null 输出空串，便于粘贴到表格类工具） */
function tsvText(cols: string[], rows: (string | null)[][], withHeader: boolean): string {
  const lines: string[] = []
  if (withHeader) lines.push(cols.join('\t'))
  for (const row of rows) lines.push(row.map((v) => (v === null ? '' : String(v))).join('\t'))
  return lines.join('\n')
}

/** 复制文本到剪贴板 */
async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text)
    ui.toast(`已复制 ${text.split('\n').length} 行到剪贴板`, 'success')
  } catch {
    ui.toast('复制到剪贴板失败', 'error')
  }
}

/** 复制为变体 */
type CopyKind =
  | 'where'
  | 'insert'
  | 'insertBatch'
  | 'insertOrUpdate'
  | 'update'
  | 'delete'
  | 'textAll'
  | 'textData'
  | 'textHeader'

/** 按变体生成 SQL 文本并复制到剪贴板（fallbackRow：无选区时的回退行） */
async function copyAs(kind: CopyKind, fallbackRow?: number): Promise<void> {
  const r = store.lastResult
  if (!r) return
  const data = selectedData(fallbackRow)
  if (!data) return
  const { rows, cols } = data
  // 表格文本：TSV（字段和数据 / 仅数据 / 仅字段），无需表名
  if (kind === 'textHeader') {
    await copyText(cols.join('\t'))
    return
  }
  if (kind === 'textAll' || kind === 'textData') {
    await copyText(tsvText(cols, rows, kind === 'textAll'))
    return
  }
  const table = tableOfQuery(lastQuerySql.value)
  if (!table) {
    ui.toast('无法解析目标表名，无法生成 SQL', 'warning')
    return
  }
  const colList = cols.map((c) => `\`${c}\``).join(', ')
  if (kind === 'where') {
    // 单行：WHERE col = val AND ...；多行：各行条件括号后 OR 联结
    const clause = rows
      .map((row) => `(${cols.map((c, i) => sqlPredicate(c, row[i])).join(' AND ')})`)
      .join(' OR ')
    await copyText(`WHERE ${clause}`)
    return
  }
  if (kind === 'insert') {
    // 单条：每行一条 INSERT
    const stmts = rows.map(
      (row) =>
        `INSERT INTO \`${table}\` (${colList}) VALUES (${row.map((v) => sqlLiteral(v)).join(', ')});`,
    )
    await copyText(stmts.join('\n'))
    return
  }
  if (kind === 'insertBatch' || kind === 'insertOrUpdate') {
    // 批量：一条 INSERT 携带多行 VALUES；InsertOrUpdate 追加 ON DUPLICATE KEY UPDATE
    const tuples = rows.map((row) => `(${row.map((v) => sqlLiteral(v)).join(', ')})`).join(',\n  ')
    let stmt = `INSERT INTO \`${table}\` (${colList}) VALUES\n  ${tuples}`
    if (kind === 'insertOrUpdate') {
      const assigns = cols.map((c) => `\`${c}\` = VALUES(\`${c}\`)`).join(', ')
      stmt += `\nON DUPLICATE KEY UPDATE ${assigns}`
    }
    await copyText(`${stmt};`)
    return
  }
  // UpdateSQL / DeleteSQL：按主键定位（未解析到主键时回退全列谓词）
  const pkIdx = r.columns.indexOf(pkColumn.value)
  const whereOf = (row: (string | null)[]): string => {
    if (pkColumn.value && pkIdx >= 0) return sqlPredicate(pkColumn.value, row[pkIdx])
    return cols.map((c, i) => sqlPredicate(c, row[i])).join(' AND ')
  }
  if (kind === 'update') {
    const stmts = rows.map((row) => {
      // SET 子句剔除主键列（主键不参与更新）
      const sets = cols
        .map((c, i) => (c === pkColumn.value ? null : `\`${c}\` = ${sqlLiteral(row[i])}`))
        .filter((s): s is string => s !== null)
      return `UPDATE \`${table}\` SET ${sets.join(', ')} WHERE ${whereOf(row)};`
    })
    await copyText(stmts.join('\n'))
    return
  }
  if (kind === 'delete') {
    const stmts = rows.map((row) => `DELETE FROM \`${table}\` WHERE ${whereOf(row)};`)
    await copyText(stmts.join('\n'))
  }
}

// ---------- 填充：作用于选中区域（走编辑管道，预览 -> 确认 -> 执行） ----------

const showFillDialog = ref(false)
const fillValue = ref('')
/** 自定义填充的目标回退单元格（菜单关闭后确认填充时仍需定位） */
const fillFallback = ref<{ ri: number; ci: number } | undefined>(undefined)

/**
 * 对选区内每个单元格记录待提交修改（不直接写库）。
 * 填充作用区域：右键单元格在矩形选区内时作用于整个选区，否则仅作用于右键单元格
 */
function fillCells(value: string | null, isNull: boolean, fallbackRi?: number, fallbackCi?: number): void {
  if (!canEdit.value) return
  const map = new Map(pendingEdits.value)
  const apply = (ri: number, ci: number) => {
    const col = resultColumns.value[ci]
    if (!col) return
    map.set(`${ri}:${col}`, { rowIndex: ri, column: col, value, isNull })
  }
  const s = cellSelection.value
  const inSelection =
    fallbackRi !== undefined &&
    fallbackCi !== undefined &&
    s !== null &&
    isCellSelected(fallbackRi, fallbackCi)
  if (inSelection && s) {
    for (let i = Math.min(s.r1, s.r2); i <= Math.max(s.r1, s.r2); i++) {
      for (let j = Math.min(s.c1, s.c2); j <= Math.max(s.c1, s.c2); j++) apply(i, j)
    }
  } else if (fallbackRi !== undefined && fallbackCi !== undefined) {
    apply(fallbackRi, fallbackCi)
  }
  pendingEdits.value = map
}

/** 自定义填充确认 */
function confirmFill(): void {
  showFillDialog.value = false
  fillCells(fillValue.value, false, fillFallback.value?.ri, fillFallback.value?.ci)
}

/** 当前日期时间 YYYY-MM-DD HH:mm:ss */
function nowDateTime(): string {
  const d = new Date()
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
}

/** UUID v4（webview 不支持 crypto.randomUUID 时的回退实现） */
function genUuid(): string {
  if (typeof crypto.randomUUID === 'function') return crypto.randomUUID()
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0
    const v = c === 'x' ? r : (r & 0x3) | 0x8
    return v.toString(16)
  })
}

// ---------- 跳转行：服务端分页导航 ----------

const showGotoDialog = ref(false)
const gotoRowNo = ref('')

/** 滚动到页内指定行（越界时滚到最后一行） */
function scrollToRowInPage(rowIndex: number): void {
  const el = resultHost.value
  if (!el) return
  const rows = el.querySelectorAll<HTMLElement>('tbody tr[data-row-index]')
  if (!rows.length) return
  const target = rowIndex >= rows.length ? rows[rows.length - 1] : rows[rowIndex]
  target.scrollIntoView({ block: 'center' })
}

async function gotoTop(): Promise<void> {
  await runQuery(1, pageSize.value)
  scrollToRowInPage(0)
}

async function gotoBottom(): Promise<void> {
  await runQuery(pageCount.value, pageSize.value)
  scrollToRowInPage(Number.MAX_SAFE_INTEGER)
}

/** 自定义行号跳转：按每页行数换算页码并滚动到页内对应行 */
async function confirmGoto(): Promise<void> {
  showGotoDialog.value = false
  const n = Number(gotoRowNo.value)
  if (!Number.isFinite(n) || n < 1) return
  const targetPage = Math.max(1, Math.ceil(n / Math.max(1, pageSize.value)))
  await runQuery(targetPage, pageSize.value)
  scrollToRowInPage(n - (targetPage - 1) * pageSize.value - 1)
}

// ---------- 粘贴：从剪贴板 TSV 解析 ----------

/** 读取剪贴板文本（失败时 toast 提示，返回 null） */
async function readClipboard(): Promise<string | null> {
  try {
    return await navigator.clipboard.readText()
  } catch {
    ui.toast('读取剪贴板失败', 'error')
    return null
  }
}

/** 解析 TSV 文本：制表符分列、换行分行（忽略行尾空行） */
function parseTsv(text: string): string[][] {
  return text
    .replace(/\r\n/g, '\n')
    .replace(/\r/g, '\n')
    .split('\n')
    .filter((line, i, arr) => i < arr.length - 1 || line !== '')
    .map((line) => line.split('\t'))
}

/** 粘贴到单元格：以右键单元格为起点写入 TSV 值（走编辑管道，不直接写库） */
async function pasteToCell(startRi: number, startCi: number): Promise<void> {
  const text = await readClipboard()
  if (text === null || !canEdit.value) return
  const grid = parseTsv(text)
  if (!grid.length) return
  const map = new Map(pendingEdits.value)
  for (let i = 0; i < grid.length; i++) {
    for (let j = 0; j < grid[i].length; j++) {
      const ri = startRi + i
      const ci = startCi + j
      const col = resultColumns.value[ci]
      // 越界部分忽略（只粘贴进已加载页的既有行列）
      if (!col || !store.lastResult?.rows[ri]) continue
      map.set(`${ri}:${col}`, { rowIndex: ri, column: col, value: grid[i][j], isNull: false })
    }
  }
  pendingEdits.value = map
}

// ---------- 行操作：克隆行 / 插入 N 行 / 新建并粘贴（预览 -> 确认 -> 执行） ----------

/**
 * 插入行预览：INSERT 语句由前端构造（仅用于展示，单引号/反斜杠成对转义），
 * 主键列不参与插入（由数据库自增/默认值生成）；确认后走原生命令
 * mysql_insert_rows（事务包裹 + 逐行参数化，见 confirmPreview）。
 */
function insertPreviewRows(values: (string | null)[][]): void {
  const table = tableOfQuery(lastQuerySql.value)
  if (!table) {
    ui.toast('无法解析目标表名，无法插入', 'warning')
    return
  }
  // 主键列不参与插入
  const colIdx: number[] = []
  resultColumns.value.forEach((c, i) => {
    if (c !== pkColumn.value) colIdx.push(i)
  })
  const columns = colIdx.map((i) => resultColumns.value[i])
  const colList = columns.map((c) => `\`${c}\``).join(', ')
  previewItems.value = values.map((row) => ({
    sql: `INSERT INTO \`${table}\` (${colList}) VALUES (${colIdx.map((i) => sqlLiteral(row[i] ?? null)).join(', ')});`,
    estimate: 1,
    danger: false,
  }))
  // 原生负载：rows 与 columns 逐列对齐（null = NULL），确认后交 mysql_insert_rows
  pendingInsert.value = {
    table,
    columns,
    rows: values.map((row) => colIdx.map((i) => row[i] ?? null)),
  }
  previewMode.value = 'insert'
  showPreview.value = true
}

/** 插入 N 行空行：INSERT INTO `t` () VALUES ();（全默认值，由数据库生成） */
function insertDefaultRows(n: number): void {
  const table = tableOfQuery(lastQuerySql.value)
  if (!table) {
    ui.toast('无法解析目标表名，无法插入', 'warning')
    return
  }
  const statements: string[] = []
  for (let i = 0; i < n; i++) statements.push(`INSERT INTO \`${table}\` () VALUES ();`)
  previewItems.value = statements.map((s) => ({ sql: s, estimate: 1, danger: false }))
  // 原生命令以空 columns 表达全默认值行（后端逐行生成 INSERT INTO t () VALUES ()）
  pendingInsert.value = { table, columns: [], rows: Array.from({ length: n }, () => []) }
  previewMode.value = 'insert'
  showPreview.value = true
}

/** 克隆行：复制当前行数据为新行（主键列由数据库自增生成，不参与 INSERT） */
function cloneRow(ri?: number): void {
  if (ri === undefined || !store.lastResult) return
  const row = store.lastResult.rows[ri]
  if (!row) return
  insertPreviewRows([row.map((c) => (c === null ? null : String(c)))])
}

/** 新建并粘贴：剪贴板 TSV 每行作为新行（INSERT，预览 -> 确认 -> 执行） */
async function pasteNewRow(): Promise<void> {
  const text = await readClipboard()
  if (text === null) return
  const grid = parseTsv(text)
  if (!grid.length) return
  insertPreviewRows(grid.map((cells) => cells.map((c) => (c === null ? '' : String(c)))))
}

/** 仅运行选中的（SQL 编辑器右键菜单）：SELECT 走分页查询，其余走写操作 */
async function runSelection(text: string): Promise<void> {
  const stmts = splitSqlStatements(text)
  for (const stmt of stmts) {
    if (SELECT_RE.test(stmt)) {
      await runQuery(1, pageSize.value, stmt)
    } else {
      try {
        const outcome = await store.executeSql(stmt)
        if (outcome.needsConfirm) return // 确认框由 store.pendingConfirm 驱动弹出
      } catch {
        // 错误已写入 store，由 v-alert 展示
      }
    }
  }
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
const previewMode = ref<'edit' | 'delete' | 'insert'>('edit')
const previewItems = ref<PreviewItem[]>([])

/** 插入预览的原生负载（columns + rows 逐列对齐）：确认后交 mysql_insert_rows */
interface PendingInsert {
  table: string
  columns: string[]
  rows: (string | null)[][]
}

const pendingInsert = ref<PendingInsert | null>(null)

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
  pendingInsert.value = null // 非插入预览：清空插入负载
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
      pv === null
        ? 'IS NULL'
        : `= '${pv.replace(/\\/g, '\\\\').replace(/'/g, "''")}'`
    items.push({
      sql: `DELETE FROM \`${table}\` WHERE \`${pkColumn.value}\` ${predicate}`,
      estimate: 1,
      danger: false,
    })
  }
  previewItems.value = items
  previewMode.value = 'delete'
  pendingInsert.value = null // 非插入预览：清空插入负载
  showPreview.value = true
}

/**
 * 预览确认后的执行入口：
 * - danger=true 或估算影响行数 > 1 时先弹 uiStore.confirm 二次确认
 * - 编辑批量走 mysql_update_rows（隐式事务包裹，失败整体回滚）
 * - 删除逐行走 mysql_delete_row
 * - 插入走 mysql_insert_rows（事务包裹 + 逐行参数化）
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
    } else if (previewMode.value === 'delete') {
      const rows = [...selectedRows.value]
      let affected = 0
      for (const ri of rows) {
        affected += await mysqlDeleteRow(connId, table, pkColumn.value, pkValueOf(ri), true)
      }
      ui.toast(`已删除 ${rows.length} 行，受影响行数：${affected}`, 'success')
      selectedRows.value = new Set()
    } else {
      // 插入：走原生命令 mysql_insert_rows（事务包裹 + 逐行参数化，
      // 预览对话框已提供确认环节）
      const pending = pendingInsert.value
      if (!pending) return
      const affected = await mysqlInsertRows(connId, pending.table, pending.columns, pending.rows, true)
      ui.toast(`已插入 ${pending.rows.length} 行，受影响行数：${affected}`, 'success')
      pendingInsert.value = null
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

/**
 * 结果网格展示行：originalIndex 为原始行索引（与列头按索引对齐；排序仅改变展示顺序，
 * 待提交修改/选中行仍按原始行索引定位，避免错位写入），null 保持为 null。
 * 待提交修改叠加展示：命中处显示新值/NULL
 */
const displayRows = computed<{ originalIndex: number; cells: (string | null)[] }[]>(() => {
  const r = store.lastResult
  if (!r) return []
  const rows = r.rows.map((row, ri) => ({
    originalIndex: ri,
    cells: r.columns.map((_, ci) => {
      const col = r.columns[ci]
      const edit = pendingEdits.value.get(`${ri}:${col}`)
      if (edit) return edit.isNull ? null : (edit.value ?? '')
      return row[ci] ?? null
    }),
  }))
  const s = sortState.value
  if (!s) return rows
  // 前端排序：null 视为最小值；两侧均为数值时按数值比较，否则按字典序
  return [...rows].sort((a, b) => {
    const av = a.cells[s.ci]
    const bv = b.cells[s.ci]
    let cmp: number
    if (av === null && bv === null) cmp = 0
    else if (av === null) cmp = -1
    else if (bv === null) cmp = 1
    else {
      const an = Number(av)
      const bn = Number(bv)
      cmp =
        av !== '' && bv !== '' && !Number.isNaN(an) && !Number.isNaN(bn)
          ? an - bn
          : av < bv
            ? -1
            : av > bv
              ? 1
              : 0
    }
    return s.dir === 'asc' ? cmp : -cmp
  })
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

/** 查询序列号：会话级单例，快速换页/双击表并发时丢弃过期响应，结果与页码始终一致 */
let querySeq = 0
/** 分页查询：page 从 1 开始 */
async function runQuery(p: number, size: number, querySql?: string): Promise<void> {
  const text = querySql ?? lastQuerySql.value
  if (!text) return
  const seq = ++querySeq
  try {
    await store.query(text, p, size)
    if (seq !== querySeq) return // 已有更新的查询在途/完成，丢弃本此过期结果
    lastQuerySql.value = text
    page.value = p
    pageSize.value = size
    // 行索引随查询/翻页失效：清空待提交集、选中行、编辑态与选区，避免错位写入
    pendingEdits.value = new Map()
    selectedRows.value = new Set()
    editingCell.value = null
    ctxMenu.value = null
    clearCellSelection()
    sortState.value = null
    // 渲染后测量列宽（列锁定的 sticky 左偏移依赖像素值）
    void nextTick().then(measureColumns)
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
  clearCellSelection()
  sortState.value = null
  lockedCols.value = []
  pkColumn.value = ''
  void connLabel
}
</script>

<style scoped>
/* 顶部状态条：flex 布局让连接信息与按钮排一行，v-spacer 才能生效
   （缺 flex 时 spacer 失效，图标+连接信息与按钮换行堆叠、文字挤叠） */
.mysql-grid__toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  padding: 4px 8px;
  gap: 0.3em;
}

/* 功能按钮：字号 12px（全局按钮 14px 基准减两档）+ 文字到边框 0.2em + 按钮间距 0.3em */
.mysql-grid__toolbar .v-btn {
  --v-btn-size: 12px;
  padding-left: 0.2em;
  padding-right: 0.2em;
}

/* SQL 编辑区按钮行：与顶部工具条同规格（执行/执行计划/历史/保存查询/BEGIN/COMMIT/ROLLBACK） */
.mysql-grid__editor-bar {
  flex-wrap: wrap;
  gap: 0.3em;
}

.mysql-grid__editor-bar .v-btn {
  --v-btn-size: 12px;
  padding-left: 0.2em;
  padding-right: 0.2em;
}

/* 结果网格编辑管道工具条：与顶部工具条同规格（提交修改/放弃修改/删除选中行） */
.mysql-grid__result-bar {
  flex-wrap: wrap;
  gap: 0.3em;
}

.mysql-grid__result-bar .v-btn {
  --v-btn-size: 12px;
  padding-left: 0.2em;
  padding-right: 0.2em;
}

/* 侧栏刷新按钮（筛选表行）：同规格 12px */
.mysql-grid__sidebar .v-btn {
  --v-btn-size: 12px;
  padding-left: 0.2em;
  padding-right: 0.2em;
}

/* 结果区提示 alert 文字：12px（全局 14px 基准减两档；只缩文字不动图标；
   __content 是 VAlert 内部元素无 data-v 属性，须用 :deep 穿透） */
.mysql-grid__main :deep(.v-alert__content) {
  font-size: 12px;
}

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

/* 侧栏拖拽手柄：4px 竖条，悬停高亮，col-resize 光标 */
.mysql-grid__resizer {
  flex: 0 0 4px;
  cursor: col-resize;
  background: transparent;
}

.mysql-grid__resizer:hover {
  background: rgba(var(--v-theme-primary), 0.25);
}

.mysql-grid__table-list {
  flex: 1 1 auto;
  overflow-y: auto;
  min-height: 0;
}

/* 表列表左对齐：Vuetify .v-list-item 的 padding-inline 带 !important，
   需同级强度覆盖才生效；压缩 prepend 图标间距，表名贴近左侧 */
.mysql-grid__table-list :deep(.v-list) {
  padding: 4px 0;
}

.mysql-grid__table-list :deep(.v-list-item) {
  padding-inline-start: 8px !important;
  padding-inline-end: 8px !important;
}

.mysql-grid__table-list :deep(.v-list-item__prepend) {
  padding-inline-end: 6px;
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
  /* 数据网格表格区（数据库显示内容）：容器设 12px 基准（2026-09-21 用户指定，
     与表右键菜单同款模式），th/td/NULL/编辑输入框/图标 1em 跟随 */
  font-size: 12px;
}

/* 行距 0.5em（单元格上下 padding，文字与边框 0.5 个字符，2026-09-21 用户指定）；
   th/td 的 v-table 默认字号 0.875rem 需显式覆盖为 1em；
   padding-block !important 压过 Vuetify 密度样式（同 min-height 覆盖惯例） */
.mysql-grid__result-table :deep(th),
.mysql-grid__result-table :deep(td) {
  font-size: 1em;
  padding-block: 0.5em !important;
}

/* 表头主键/排序图标：1em 跟随 + 与文字间距 0.5em（mr-1 带 !important 须同级强度覆盖） */
.mysql-grid__result-table :deep(th .v-icon) {
  font-size: 1em;
  margin-inline-end: 0.5em !important;
}

/* 行高收紧（2026-09-21 用户指定"高度太大"）：checkbox 高度 28px 撑高行，
   强制跟随字号压缩 */
.mysql-grid__result-table :deep(.v-selection-control) {
  height: 1.75em !important;
  min-height: 0 !important;
}

/* SQL NULL：灰色斜体（深色主题下用主题 token 保证可读性）；1em 跟随容器 12px */
/* SQL NULL：普通文字（2026-09-21 用户指定"最简表格模式"，灰色斜体等待殊样式已去除） */

/* 可编辑单元格：光标提示可编辑；待提交修改：警告色左侧描边 */
.mysql-grid__cell--editable {
  cursor: pointer;
}

.mysql-grid__cell--edited {
  background: rgba(var(--v-theme-warning), 0.08);
}

/* 选中行高亮 */
.mysql-grid__row--selected {
  background: rgba(var(--v-theme-primary), 0.08);
}

/* 最简表格模式（2026-09-21 用户指定，参考 Excel 极简网格）：白底 + 横竖细网格线，
   无斑马纹/无行 hover 高亮/无 NULL 特殊样式；border-right+bottom 在 separate 模式
   下与相邻单元格不叠加（各一条线），形成完整网格 */
.mysql-grid__result-table :deep(th),
.mysql-grid__result-table :deep(td) {
  border-right: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 列锁定：sticky 冻结（不透明底色防止下方内容透出；表头层级高于单元格） */
.mysql-grid__cell--locked,
.mysql-grid__head-cell--locked {
  position: sticky;
  background: rgb(var(--v-theme-surface));
}

.mysql-grid__cell--locked {
  z-index: 1;
}

.mysql-grid__head-cell--locked {
  z-index: 2;
}

/* 行号 gutter：弱化小字 + 等宽右对齐（Navicat 式行号锚点，随 checkbox 列 sticky 冻结） */
.mysql-grid__row-num {
  color: rgb(var(--v-theme-on-surface) / 0.5);
  font-size: 12px;
  font-family: var(--fy-mono);
  text-align: right;
  padding: 2px 6px 2px 0;
}

/* 锁定列在选中/编辑状态下的底色与行高亮保持一致 */
.mysql-grid__row--selected .mysql-grid__cell--locked {
  background: rgba(var(--v-theme-primary), 0.08);
}

.mysql-grid__cell--locked.mysql-grid__cell--edited {
  background: rgba(var(--v-theme-warning), 0.08);
}

/* 单元格选中区域（矩形选区）：Excel 式单框——outline 单线；自己的网格线透明
   （outline 画在 box 外侧 1px，与 border-right/bottom 紧贴成灰+蓝双线，即用户指的双层）；
   左/上边的邻格线由 outline 覆盖 */
.mysql-grid__result-table :deep(td.mysql-grid__cell--selected) {
  outline: 1px solid rgba(var(--v-theme-primary), 0.6);
  background: transparent;
  border-right-color: transparent !important;
  border-bottom-color: transparent !important;
}

/* 列头可单击选中整列 */
th[title='单击选中整列'] {
  cursor: pointer;
}

/* 编辑态单元格：Excel 式——td 网格线消失，input 直角实线边框占满单元格
   （替代网格线位置，无间隙无圆角；td border 透明保留占位防止行高变化） */
.mysql-grid__result-table :deep(td:has(> input.mysql-grid__cell-input)) {
  padding: 0 !important;
  outline: none !important;
  background: none !important;
  border-right-color: transparent !important;
  border-bottom-color: transparent !important;
}

/* 单元格内联编辑输入框：不透明主色实线直角边框，占满 td（1em 跟随容器 12px） */
.mysql-grid__cell-input {
  width: 100%;
  height: 100%;
  box-sizing: border-box;
  display: block;
  border: 1.5px solid rgb(var(--v-theme-primary));
  border-radius: 0;
  padding: 2px 6px;
  font-size: 1em;
  font-family: inherit;
  background: rgb(var(--v-theme-surface));
  color: rgb(var(--v-theme-on-surface));
  outline: none;
  box-shadow: none;
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

/* 表右键菜单（Navicat 表对象菜单）紧凑化：字体 12px（2026-09-21 用户指定改回 12px），
   字体显式声明系统微软雅黑；行间 0.3em，图标与文字间距 0.5em（em 以菜单字号为基准）；
   仅作用于表菜单，单元格右键菜单不受影响。
   min-height !important 压过全局列表 28px !important，标题/图标用更高特异性压过全局 14px */
.mysql-grid__ctx-menu--table {
  font-size: 12px;
  font-family: var(--fy-font);
}

.mysql-grid__ctx-menu--table .v-list {
  row-gap: 0.3em;
}

/* 标题/图标/spacer 由 v-list-item 内部渲染（无 data-v 属性），必须 :deep 穿透，
   否则组激活项（复制表/转储 SQL 文件/维护）回退全局 14px */
.mysql-grid__ctx-menu--table .v-list :deep(.v-list-item) {
  min-height: 0 !important;
  padding-block: 0 !important;
}

.mysql-grid__ctx-menu--table .v-list :deep(.v-list-item-title) {
  font-size: 1em;
  line-height: 1.3;
}

.mysql-grid__ctx-menu--table .v-list :deep(.v-list-item .v-icon) {
  font-size: 1em;
}

.mysql-grid__ctx-menu--table .v-list :deep(.v-list-item__prepend > .v-icon ~ .v-list-item__spacer) {
  width: 0.5em;
}

/* 编辑预览对话框中的 SQL 片段（与危险确认框共用类名） */
.mysql-grid__sql-preview {
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
  padding: 6px 8px;
}

.mysql-grid__sql-preview-sql {
  font-family: var(--fy-font);
  font-size: 14px;
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
