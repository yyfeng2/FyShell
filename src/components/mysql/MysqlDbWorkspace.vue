<template>
  <v-card class="mysql-ws" flat>
    <!-- 顶部对象工具条（仿 Navicat）：图标+文字按钮组，当前选中项蓝色高亮 -->
    <div class="mysql-ws__toolbar">
      <v-btn
        size="small"
        variant="text"
        prepend-icon="mdi-console-line"
        :disabled="!store.isConnected"
        title="新建查询（由顶部工作台处理）"
        @click="onNewQuery"
      >
        新建查询
      </v-btn>
      <v-divider vertical inset class="mx-1" />
      <v-btn
        v-for="tab in TABS"
        :key="tab.key"
        size="small"
        variant="text"
        :class="['mysql-ws__tab', { 'mysql-ws__tab--active': activeTab === tab.key }]"
        :disabled="!store.isConnected"
        @click="activeTab = tab.key"
      >
        <v-icon size="small" class="mr-1">{{ tab.icon }}</v-icon>
        {{ tab.label }}
      </v-btn>
      <v-divider vertical inset class="mx-1" />
      <v-btn size="small" variant="text" prepend-icon="mdi-database-export" :disabled="!store.isConnected" @click="showBackup = true">
        备份
      </v-btn>
      <v-btn size="small" variant="text" prepend-icon="mdi-clock-outline" :disabled="!store.isConnected" @click="showAutoRun = true">
        自动运行
      </v-btn>
    </div>
    <v-divider />

    <!-- 未连接占位 -->
    <div v-if="!store.isConnected" class="mysql-ws__placeholder text-caption text-medium-emphasis">
      连接 MySQL 后显示对象工作台
    </div>

    <!-- 表：直接嵌入现有数据网格（不修改 MysqlDataGrid） -->
    <div v-else-if="activeTab === 'table'" class="mysql-ws__grid-wrap">
      <MysqlDataGrid />
    </div>

    <!-- 视图 / 函数 / 其它：通用对象列表面板（左侧列表 + 主区 DDL） -->
    <div v-else-if="currentKind" class="mysql-ws__body">
      <div class="mysql-ws__sidebar">
        <!-- 函数 tab：函数 / 存储过程分组切换 -->
        <div v-if="activeTab === 'function'" class="d-flex px-2 pt-1 pb-1">
          <v-btn-toggle v-model="functionKind" density="compact" mandatory class="mysql-ws__kind-toggle">
            <v-btn value="function" size="x-small">函数</v-btn>
            <v-btn value="procedure" size="x-small">存储过程</v-btn>
          </v-btn-toggle>
        </div>
        <!-- 其它 tab：触发器 / 事件分组切换 -->
        <div v-if="activeTab === 'other'" class="d-flex px-2 pt-1 pb-1">
          <v-btn-toggle v-model="otherKind" density="compact" mandatory class="mysql-ws__kind-toggle">
            <v-btn value="trigger" size="x-small">触发器</v-btn>
            <v-btn value="event" size="x-small">事件</v-btn>
          </v-btn-toggle>
        </div>
        <div class="d-flex align-center px-2 py-1">
          <v-text-field
            v-model="objectFilter"
            label="筛选对象"
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
            :loading="objectsLoading"
            title="刷新对象列表"
            @click="refreshObjects"
          />
        </div>
        <v-divider />
        <div class="mysql-ws__list">
          <v-list density="compact" nav>
            <v-list-item
              v-for="obj in filteredObjects"
              :key="obj.name"
              :active="obj.name === selectedObject"
              @click="selectObject(obj.name)"
            >
              <template #prepend>
                <v-icon size="small">{{ kindIcon }}</v-icon>
              </template>
              <v-list-item-title class="text-body-2">{{ obj.name }}</v-list-item-title>
              <v-list-item-subtitle v-if="obj.comment" class="text-caption">
                {{ obj.comment }}
              </v-list-item-subtitle>
            </v-list-item>
            <v-list-item v-if="!objectsLoading && filteredObjects.length === 0">
              <v-list-item-title class="text-caption text-medium-emphasis">没有匹配的对象</v-list-item-title>
            </v-list-item>
          </v-list>
        </div>
      </div>

      <!-- 主区：DDL 查看 + 新建/编辑/删除 -->
      <div class="mysql-ws__main">
        <div class="d-flex align-center mb-1">
          <v-icon size="small" class="mr-1">{{ kindIcon }}</v-icon>
          <span class="text-body-2 mr-2 text-medium-emphasis">
            {{ selectedObject || `选择${kindLabel}` }}
          </span>
          <v-spacer />
          <v-btn size="small" color="primary" variant="tonal" prepend-icon="mdi-plus" @click="openCreate">
            新建
          </v-btn>
          <v-btn
            size="small"
            variant="text"
            prepend-icon="mdi-pencil"
            :disabled="!selectedObject"
            class="ml-1"
            @click="openEdit"
          >
            编辑
          </v-btn>
          <v-btn
            size="small"
            color="error"
            variant="outlined"
            prepend-icon="mdi-delete-outline"
            :disabled="!selectedObject || ddlLoading"
            class="ml-1"
            @click="deleteObject"
          >
            删除
          </v-btn>
        </div>
        <v-alert v-if="objectError" type="error" variant="tonal" density="compact" closable class="mb-2">
          {{ objectError }}
        </v-alert>
        <v-alert v-else-if="ddlError" type="error" variant="tonal" density="compact" closable class="mb-2">
          {{ ddlError }}
        </v-alert>
        <div class="mysql-ws__ddl">
          <div v-if="ddlLoading" class="text-caption text-medium-emphasis pa-2">加载 DDL 中…</div>
          <div v-else-if="ddl" class="mysql-ws__ddl-text">{{ ddl }}</div>
          <div v-else class="mysql-ws__ddl-hint text-caption text-medium-emphasis">
            在左侧选择{{ kindLabel }}查看 DDL
          </div>
        </div>
      </div>
    </div>

    <!-- 用户管理：左侧用户列表 + 主区权限（SHOW GRANTS） -->
    <div v-else-if="activeTab === 'user'" class="mysql-ws__body">
      <div class="mysql-ws__sidebar">
        <div class="d-flex align-center px-2 py-1">
          <v-text-field
            v-model="userFilter"
            label="筛选用户"
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
            :loading="usersLoading"
            title="刷新用户列表"
            @click="refreshUsers"
          />
        </div>
        <v-divider />
        <div class="mysql-ws__list">
          <v-list density="compact" nav>
            <v-list-item
              v-for="u in filteredUsers"
              :key="`${u.user}@${u.host}`"
              :active="u.user === selectedUser?.user && u.host === selectedUser?.host"
              @click="selectUser(u)"
            >
              <template #prepend>
                <v-icon size="small">mdi-account-outline</v-icon>
              </template>
              <v-list-item-title class="text-body-2">{{ u.user }}@{{ u.host }}</v-list-item-title>
              <v-list-item-subtitle v-if="u.comment" class="text-caption">
                {{ u.comment }}
              </v-list-item-subtitle>
            </v-list-item>
            <v-list-item v-if="!usersLoading && filteredUsers.length === 0">
              <v-list-item-title class="text-caption text-medium-emphasis">没有匹配的用户</v-list-item-title>
            </v-list-item>
          </v-list>
        </div>
      </div>

      <div class="mysql-ws__main">
        <div class="d-flex align-center mb-1">
          <v-icon size="small" class="mr-1">mdi-account-outline</v-icon>
          <span class="text-body-2 mr-2 text-medium-emphasis">
            {{ selectedUser ? `${selectedUser.user}@${selectedUser.host}` : '选择用户' }}
          </span>
          <v-spacer />
          <v-btn size="small" color="primary" variant="tonal" prepend-icon="mdi-account-plus" @click="openUserCreate">
            新建用户
          </v-btn>
          <v-btn
            size="small"
            variant="text"
            prepend-icon="mdi-key"
            :disabled="!selectedUser"
            title="重新拉取 SHOW GRANTS"
            class="ml-1"
            @click="refreshGrants"
          >
            查看权限
          </v-btn>
          <v-btn
            size="small"
            color="error"
            variant="outlined"
            prepend-icon="mdi-account-remove"
            :disabled="!selectedUser"
            class="ml-1"
            @click="deleteUser"
          >
            删除
          </v-btn>
        </div>
        <v-alert v-if="userError" type="error" variant="tonal" density="compact" closable class="mb-2">
          {{ userError }}
        </v-alert>
        <div class="mysql-ws__ddl">
          <div v-if="grantsLoading" class="text-caption text-medium-emphasis pa-2">加载权限中…</div>
          <template v-else-if="selectedUser">
            <div v-if="grants.length" class="mysql-ws__ddl-text">
              <div v-for="(g, i) in grants" :key="i" class="mysql-ws__grant-line">{{ g.grant_sql }}</div>
            </div>
            <div v-else class="mysql-ws__ddl-hint text-caption text-medium-emphasis">
              暂无权限记录（或无权限查看）
            </div>
          </template>
          <div v-else class="mysql-ws__ddl-hint text-caption text-medium-emphasis">
            在左侧选择用户查看权限
          </div>
        </div>
      </div>
    </div>

    <!-- 模型：外键关系只读视图（表卡片 + SVG 连线） -->
    <div v-else class="mysql-ws__model">
      <div class="d-flex align-center mb-1">
        <span class="text-body-2 mr-2">外键关系视图（只读）</span>
        <v-chip v-if="modelCards.length" size="x-small" variant="tonal" class="mr-2">
          {{ modelCards.length }} 表 · {{ modelRels.length }} 条外键
        </v-chip>
        <v-spacer />
        <v-btn
          size="small"
          variant="text"
          prepend-icon="mdi-refresh"
          :loading="modelLoading"
          @click="refreshModel"
        >
          刷新
        </v-btn>
      </div>
      <v-alert v-if="modelError" type="error" variant="tonal" density="compact" closable>
        {{ modelError }}
      </v-alert>
      <div class="mysql-ws__model-canvas-wrap">
        <div
          v-if="modelCards.length"
          class="mysql-ws__model-canvas"
          :style="{ width: `${canvasWidth}px`, height: `${canvasHeight}px` }"
        >
          <svg class="mysql-ws__model-lines" :width="canvasWidth" :height="canvasHeight">
            <defs>
              <marker id="mysql-ws-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
                <path d="M0,0 L8,4 L0,8 Z" class="mysql-ws__model-arrow" />
              </marker>
            </defs>
            <path
              v-for="(rel, i) in modelRels"
              :key="i"
              :d="lineOf(rel)"
              class="mysql-ws__model-line"
              marker-end="url(#mysql-ws-arrow)"
            >
              <title>{{ rel.label }}</title>
            </path>
          </svg>
          <v-card
            v-for="t in modelCards"
            :key="t.table"
            class="mysql-ws__model-card"
            flat
            :style="{ left: `${t.x}px`, top: `${t.y}px`, width: `${MODEL_CARD_W}px`, height: `${MODEL_CARD_H}px` }"
          >
            <div class="mysql-ws__model-card-title">
              <v-icon size="x-small" class="mr-1">mdi-table-outline</v-icon>{{ t.table }}
            </div>
            <div class="mysql-ws__model-card-sub text-caption">
              {{ t.columns }} 字段 · {{ t.fkCount }} 外键
            </div>
          </v-card>
        </div>
        <div v-else class="mysql-ws__ddl-hint text-caption text-medium-emphasis">
          {{ modelLoading ? '解析外键关系中…' : '当前库没有表，或未解析到外键' }}
        </div>
      </div>
    </div>

    <!-- DDL 编辑对话框：新建/编辑对象（保存调 mysql_object_save） -->
    <v-dialog v-model="showDdlDialog" width="720" persistent>
      <v-card>
        <v-card-title class="d-flex align-center">
          <v-icon size="small" class="mr-2">{{ kindIcon }}</v-icon>
          {{ dialogMode === 'create' ? `新建${kindLabel}` : `编辑${kindLabel}` }}
        </v-card-title>
        <v-divider />
        <v-card-text>
          <v-text-field
            v-model="dialogName"
            :label="dialogMode === 'create' ? '对象名称' : '对象名称（编辑模式不可改名）'"
            density="compact"
            variant="outlined"
            :readonly="dialogMode === 'edit'"
            class="mb-2"
          />
          <v-textarea
            v-model="dialogSql"
            label="CREATE 语句"
            density="compact"
            variant="outlined"
            rows="10"
            auto-grow
            hide-details
            class="mysql-ws__ddl-input"
            @input="dialogSqlDirty = true"
          />
          <v-alert type="warning" variant="tonal" density="compact" class="mt-2">
            保存时后端会先 DROP 旧对象再执行新 CREATE，请确认语句无误。
          </v-alert>
        </v-card-text>
        <v-divider />
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="showDdlDialog = false">取消</v-btn>
          <v-btn color="primary" prepend-icon="mdi-check" :loading="dialogSaving" @click="saveObject">
            保存
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- 新建用户对话框：用户 / 主机 / 密码 -->
    <v-dialog v-model="showUserForm" width="480" persistent>
      <v-card>
        <v-card-title class="d-flex align-center">
          <v-icon size="small" class="mr-2">mdi-account-plus</v-icon>
          新建用户
        </v-card-title>
        <v-divider />
        <v-card-text>
          <v-text-field v-model="newUser.user" label="用户名" density="compact" variant="outlined" class="mb-2" />
          <v-text-field v-model="newUser.host" label="主机（host）" density="compact" variant="outlined" class="mb-2" />
          <v-text-field
            v-model="newUser.password"
            label="密码（留空 = 无密码）"
            density="compact"
            variant="outlined"
            type="password"
          />
        </v-card-text>
        <v-divider />
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="showUserForm = false">取消</v-btn>
          <v-btn color="primary" prepend-icon="mdi-check" :loading="creatingUser" @click="createUser">
            创建
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- 备份 / 自动运行（仿 Navicat） -->
    <BackupPanel v-model="showBackup" :conn-id="store.connId ?? ''" />
    <AutoRunPanel v-model="showAutoRun" :conn-id="store.connId ?? ''" />
  </v-card>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useMysqlStore } from '@/stores/mysql'
import { useUiStore } from '@/stores/ui'
import {
  mysqlObjectList,
  mysqlObjectDdl,
  mysqlObjectSave,
  mysqlObjectDrop,
} from '@/api/mysqlObjects'
import type { MySqlObjectInfo, MySqlObjectKind } from '@/api/mysqlObjects'
import {
  mysqlUserList,
  mysqlUserCreate,
  mysqlUserDrop,
  mysqlUserGrants,
} from '@/api/mysqlUsers'
import type { MySqlUserInfo, MySqlUserGrant } from '@/api/mysqlUsers'
import { mysqlTableDesignGet } from '@/api/mysqlDesign'
import MysqlDataGrid from './MysqlDataGrid.vue'
import BackupPanel from './BackupPanel.vue'
import AutoRunPanel from './AutoRunPanel.vue'

const store = useMysqlStore()
const ui = useUiStore()

/** 错误归一化：Rust 侧 AppError 以字符串形式 reject */
function errText(err: unknown): string {
  return typeof err === 'string' ? err : String(err)
}

const emit = defineEmits<{ (e: 'new-query'): void }>()

// ---------- 顶部对象工具条（仿 Navicat） ----------
type WorkspaceTab = 'table' | 'view' | 'function' | 'user' | 'other' | 'model'

interface WorkspaceTabItem {
  key: WorkspaceTab
  label: string
  icon: string
}

/** 对象类型切换项（新建查询为动作按钮，单独渲染） */
const TABS: WorkspaceTabItem[] = [
  { key: 'table', label: '表', icon: 'mdi-table-outline' },
  { key: 'view', label: '视图', icon: 'mdi-eye-outline' },
  { key: 'function', label: '函数', icon: 'mdi-function-variant' },
  { key: 'user', label: '用户', icon: 'mdi-account-outline' },
  { key: 'other', label: '其它', icon: 'mdi-dots-horizontal' },
  { key: 'model', label: '模型', icon: 'mdi-sitemap' },
]

const activeTab = ref<WorkspaceTab>('table')
const showBackup = ref(false)
const showAutoRun = ref(false)

/** 函数 tab 内部分组：函数 / 存储过程；其它 tab 内部分组：触发器 / 事件 */
const functionKind = ref<'function' | 'procedure'>('function')
const otherKind = ref<'trigger' | 'event'>('trigger')

/** 对象类型 -> 中文标签 / 图标（列表面板与 DDL 对话框共用） */
const KIND_LABELS: Record<MySqlObjectKind, string> = {
  view: '视图',
  function: '函数',
  procedure: '存储过程',
  trigger: '触发器',
  event: '事件',
}
const KIND_ICONS: Record<MySqlObjectKind, string> = {
  view: 'mdi-eye-outline',
  function: 'mdi-function-variant',
  procedure: 'mdi-function',
  trigger: 'mdi-flash-outline',
  event: 'mdi-clock-outline',
}

/** 新建查询：交给父级处理（如切换 Tab / 聚焦 SQL 编辑器），并 toast 提示 */
function onNewQuery(): void {
  emit('new-query')
  ui.toast('已发出新建查询请求，可在 SQL 编辑区输入并执行', 'info')
}

// ---------- 通用对象列表（视图 / 函数 / 其它三个 tab 共用） ----------

/** 当前对象面板对应的对象类型；user/table/model tab 下为 null */
const currentKind = computed<MySqlObjectKind | null>(() => {
  if (activeTab.value === 'view') return 'view'
  if (activeTab.value === 'function') return functionKind.value
  if (activeTab.value === 'other') return otherKind.value
  return null
})

const kindLabel = computed(() => (currentKind.value ? KIND_LABELS[currentKind.value] : ''))
const kindIcon = computed(() => (currentKind.value ? KIND_ICONS[currentKind.value] : ''))

const objects = ref<MySqlObjectInfo[]>([])
const objectsLoading = ref(false)
const objectError = ref('')
const objectFilter = ref<string | null>(null)
const selectedObject = ref('')
const ddl = ref('')
const ddlLoading = ref(false)
const ddlError = ref('')

/** 已加载过的对象类型（避免重复拉取；保存/删除后失效） */
const loadedKinds = ref(new Set<MySqlObjectKind>())

/** 对象类型对应的 CREATE 语句模板（新建对话框预填用） */
function createTemplate(kind: MySqlObjectKind, name: string): string {
  switch (kind) {
    case 'view':
      return `CREATE VIEW \`${name}\` AS\nSELECT 1;`
    case 'function':
      return `CREATE FUNCTION \`${name}\`()\nRETURNS INT\nDETERMINISTIC\nBEGIN\n  RETURN 1;\nEND`
    case 'procedure':
      return `CREATE PROCEDURE \`${name}\`()\nBEGIN\n  SELECT 1;\nEND`
    case 'trigger':
      return `CREATE TRIGGER \`${name}\` AFTER INSERT ON \`表名\`\nFOR EACH ROW\nBEGIN\n  -- 触发逻辑\nEND`
    case 'event':
      return `CREATE EVENT \`${name}\`\nON SCHEDULE EVERY 1 DAY\nDO\n  -- 事件逻辑;\n  SELECT 1;`
  }
}

const filteredObjects = computed(() => {
  const kw = (objectFilter.value ?? '').trim().toLowerCase()
  if (!kw) return objects.value
  return objects.value.filter((o) => o.name.toLowerCase().includes(kw))
})

async function refreshObjects(): Promise<void> {
  const kind = currentKind.value
  const connId = store.connId
  if (!kind || !connId) return
  objectsLoading.value = true
  objectError.value = ''
  try {
    objects.value = await mysqlObjectList(connId, kind)
    loadedKinds.value.add(kind)
  } catch (err) {
    objectError.value = errText(err)
  } finally {
    objectsLoading.value = false
  }
}

// 切换对象类型时按需加载（已缓存的不重复拉取）
watch(currentKind, (kind) => {
  objects.value = []
  selectedObject.value = ''
  ddl.value = ''
  ddlError.value = ''
  if (kind && store.connId && !loadedKinds.value.has(kind)) {
    void refreshObjects()
  }
})

/** 点击对象：拉取 DDL 展示到主区 */
async function selectObject(name: string): Promise<void> {
  selectedObject.value = name
  const connId = store.connId
  const kind = currentKind.value
  if (!connId || !kind) return
  ddlLoading.value = true
  ddlError.value = ''
  try {
    const result = await mysqlObjectDdl(connId, kind, name)
    ddl.value = result.sql
  } catch (err) {
    ddlError.value = errText(err)
    ddl.value = ''
  } finally {
    ddlLoading.value = false
  }
}

// ---------- DDL 编辑对话框（新建/编辑共用） ----------
const showDdlDialog = ref(false)
const dialogMode = ref<'create' | 'edit'>('create')
const dialogName = ref('')
const dialogSql = ref('')
const dialogSaving = ref(false)
/** 新建模式下 SQL 是否被手动改过（未改时随对象名称重新生成模板） */
const dialogSqlDirty = ref(false)

function openCreate(): void {
  if (!currentKind.value) return
  dialogMode.value = 'create'
  dialogName.value = ''
  dialogSql.value = createTemplate(currentKind.value, '名称')
  dialogSqlDirty.value = false
  showDdlDialog.value = true
}

function openEdit(): void {
  if (!selectedObject.value || !currentKind.value) return
  dialogMode.value = 'edit'
  dialogName.value = selectedObject.value
  dialogSql.value = ddl.value
  dialogSqlDirty.value = true
  showDdlDialog.value = true
}

// 新建模式下对象名称变化且 SQL 未手动改过时，重新生成模板
watch(dialogName, (name) => {
  if (dialogMode.value === 'create' && !dialogSqlDirty.value && currentKind.value) {
    dialogSql.value = createTemplate(currentKind.value, name.trim() || '名称')
  }
})

async function saveObject(): Promise<void> {
  const kind = currentKind.value
  const connId = store.connId
  const name = dialogName.value.trim()
  if (!connId || !kind || !name) {
    ui.toast('请填写对象名称', 'warning')
    return
  }
  dialogSaving.value = true
  try {
    await mysqlObjectSave(connId, kind, name, dialogSql.value)
    ui.toast(`${KIND_LABELS[kind]}「${name}」已保存`, 'success')
    showDdlDialog.value = false
    // 保存后强制刷新对象列表并重新选中
    loadedKinds.value.delete(kind)
    await refreshObjects()
    selectedObject.value = name
    ddl.value = dialogSql.value
  } catch (err) {
    ui.toast(errText(err), 'error')
  } finally {
    dialogSaving.value = false
  }
}

/** 删除对象：uiStore.confirm 二次确认后调 mysql_object_drop */
async function deleteObject(): Promise<void> {
  const kind = currentKind.value
  const connId = store.connId
  const name = selectedObject.value
  if (!connId || !kind || !name) return
  const ok = await ui.confirm({
    title: '删除确认',
    message: `确定删除${KIND_LABELS[kind]}「${name}」吗？该操作会执行 DROP，不可恢复。`,
    confirmText: '删除',
    danger: true,
  })
  if (!ok) return
  try {
    await mysqlObjectDrop(connId, kind, name)
    ui.toast(`${KIND_LABELS[kind]}「${name}」已删除`, 'success')
    loadedKinds.value.delete(kind)
    await refreshObjects()
    selectedObject.value = ''
    ddl.value = ''
  } catch (err) {
    ui.toast(errText(err), 'error')
  }
}

// ---------- 用户管理 ----------
const users = ref<MySqlUserInfo[]>([])
const usersLoading = ref(false)
const userError = ref('')
const userFilter = ref<string | null>(null)
const selectedUser = ref<{ user: string; host: string } | null>(null)
const usersLoaded = ref(false)

const grants = ref<MySqlUserGrant[]>([])
const grantsLoading = ref(false)

const filteredUsers = computed(() => {
  const kw = (userFilter.value ?? '').trim().toLowerCase()
  if (!kw) return users.value
  return users.value.filter((u) => `${u.user}@${u.host}`.toLowerCase().includes(kw))
})

async function refreshUsers(): Promise<void> {
  const connId = store.connId
  if (!connId) return
  usersLoading.value = true
  userError.value = ''
  try {
    users.value = await mysqlUserList(connId)
    usersLoaded.value = true
  } catch (err) {
    // 权限不足等后端错误直接展示
    userError.value = errText(err)
  } finally {
    usersLoading.value = false
  }
}

async function refreshGrants(): Promise<void> {
  const connId = store.connId
  const target = selectedUser.value
  if (!connId || !target) return
  grantsLoading.value = true
  try {
    grants.value = await mysqlUserGrants(connId, target.user, target.host)
  } catch (err) {
    ui.toast(errText(err), 'error')
    grants.value = []
  } finally {
    grantsLoading.value = false
  }
}

/** 点击用户：选中并自动拉取 SHOW GRANTS */
function selectUser(u: MySqlUserInfo): void {
  selectedUser.value = { user: u.user, host: u.host }
  void refreshGrants()
}

const showUserForm = ref(false)
const newUser = ref({ user: '', host: '%', password: '' })
const creatingUser = ref(false)

function openUserCreate(): void {
  newUser.value = { user: '', host: '%', password: '' }
  showUserForm.value = true
}

async function createUser(): Promise<void> {
  const connId = store.connId
  const name = newUser.value.user.trim()
  const host = newUser.value.host.trim()
  if (!connId || !name || !host) {
    ui.toast('请填写用户名与主机', 'warning')
    return
  }
  creatingUser.value = true
  try {
    await mysqlUserCreate(connId, name, host, newUser.value.password)
    ui.toast(`用户「${name}@${host}」已创建`, 'success')
    showUserForm.value = false
    usersLoaded.value = false
    await refreshUsers()
  } catch (err) {
    ui.toast(errText(err), 'error')
  } finally {
    creatingUser.value = false
  }
}

/** 删除用户：二次确认后调 mysql_user_drop */
async function deleteUser(): Promise<void> {
  const connId = store.connId
  const target = selectedUser.value
  if (!connId || !target) return
  const ok = await ui.confirm({
    title: '删除确认',
    message: `确定删除用户「${target.user}@${target.host}」吗？该操作不可恢复。`,
    confirmText: '删除',
    danger: true,
  })
  if (!ok) return
  try {
    await mysqlUserDrop(connId, target.user, target.host)
    ui.toast(`用户「${target.user}@${target.host}」已删除`, 'success')
    selectedUser.value = null
    grants.value = []
    usersLoaded.value = false
    await refreshUsers()
  } catch (err) {
    ui.toast(errText(err), 'error')
  }
}

// 进入用户 tab 时按需加载用户列表
watch(activeTab, (tab) => {
  if (tab === 'user' && store.connId && !usersLoaded.value) {
    void refreshUsers()
  }
})

// ---------- 模型：外键关系只读视图 ----------

interface ModelCard {
  table: string
  columns: number
  fkCount: number
  x: number
  y: number
}

interface FkRelation {
  from: string
  to: string
  label: string
}

/** 画布布局常量：固定卡片尺寸 + 网格排布（简化版，不做拖拽/自动布局） */
const MODEL_CARD_W = 180
const MODEL_CARD_H = 84
const MODEL_GAP_X = 64
const MODEL_GAP_Y = 48
const MODEL_PAD = 16
const MODEL_COLS = 4

const modelCards = ref<ModelCard[]>([])
const modelRels = ref<FkRelation[]>([])
const modelLoading = ref(false)
const modelError = ref('')

const canvasWidth = computed(
  () => MODEL_PAD * 2 + MODEL_COLS * MODEL_CARD_W + (MODEL_COLS - 1) * MODEL_GAP_X,
)

const canvasHeight = computed(() => {
  const rows = Math.max(1, Math.ceil(modelCards.value.length / MODEL_COLS))
  return MODEL_PAD * 2 + rows * MODEL_CARD_H + (rows - 1) * MODEL_GAP_Y
})

/**
 * 刷新模型视图：对当前库每张表调 mysql_table_design_get 拉外键
 * （单表失败按无外键处理，不阻塞整体渲染），按网格排布表卡片。
 */
async function refreshModel(): Promise<void> {
  const connId = store.connId
  if (!connId) return
  modelLoading.value = true
  modelError.value = ''
  try {
    const tables = store.tables
    const designs = await Promise.all(
      tables.map(async (t) => {
        try {
          return await mysqlTableDesignGet(connId, t.name)
        } catch {
          // 单表拉取失败（无权限/表不存在等）按无外键处理
          return null
        }
      }),
    )
    const cards: ModelCard[] = []
    const rels: FkRelation[] = []
    designs.forEach((design, i) => {
      const col = i % MODEL_COLS
      const row = Math.floor(i / MODEL_COLS)
      cards.push({
        table: tables[i].name,
        columns: design?.columns.length ?? 0,
        fkCount: design?.foreign_keys.length ?? 0,
        x: MODEL_PAD + col * (MODEL_CARD_W + MODEL_GAP_X),
        y: MODEL_PAD + row * (MODEL_CARD_H + MODEL_GAP_Y),
      })
      if (design) {
        for (const fk of design.foreign_keys) {
          rels.push({ from: tables[i].name, to: fk.ref_table, label: fk.name })
        }
      }
    })
    // 只画两端表都存在的关系（引用画布外表的连线省略）
    const names = new Set(cards.map((c) => c.table))
    modelRels.value = rels.filter((r) => names.has(r.to))
    modelCards.value = cards
  } catch (err) {
    modelError.value = errText(err)
  } finally {
    modelLoading.value = false
  }
}

/** 连线路径：两端卡片中心点、以垂直中点为控制点的贝塞尔曲线 */
function lineOf(rel: FkRelation): string {
  const from = modelCards.value.find((c) => c.table === rel.from)
  const to = modelCards.value.find((c) => c.table === rel.to)
  if (!from || !to) return ''
  const x1 = from.x + MODEL_CARD_W / 2
  const y1 = from.y + MODEL_CARD_H / 2
  const x2 = to.x + MODEL_CARD_W / 2
  const y2 = to.y + MODEL_CARD_H / 2
  const my = (y1 + y2) / 2
  return `M ${x1} ${y1} C ${x1} ${my}, ${x2} ${my}, ${x2} ${y2}`
}

// 进入模型 tab 时按需加载外键关系
watch(activeTab, (tab) => {
  if (tab === 'model') void refreshModel()
})

// ---------- 连接切换：清空会话级状态 ----------
watch(
  () => store.connId,
  () => {
    loadedKinds.value = new Set()
    objects.value = []
    objectError.value = ''
    selectedObject.value = ''
    ddl.value = ''
    users.value = []
    userError.value = ''
    selectedUser.value = null
    grants.value = []
    usersLoaded.value = false
    modelCards.value = []
    modelRels.value = []
  },
)
</script>

<style scoped>
.mysql-ws {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

/* 顶部对象工具条（仿 Navicat）：按钮组 + 蓝色高亮 tab */
.mysql-ws__toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  padding: 4px 8px;
  gap: 2px;
}

.mysql-ws__tab {
  border-radius: 6px;
}

/* 当前选中项高亮：蓝色 tonal 背景（仿 Navicat 选中 tab） */
.mysql-ws__tab--active {
  background: rgba(var(--v-theme-primary), 0.16);
  color: rgb(var(--v-theme-primary));
}

.mysql-ws__grid-wrap {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

/* 对象/用户列表面板：左侧列表固定宽 + 主区滚动 */
.mysql-ws__body {
  display: flex;
  flex: 1 1 auto;
  min-height: 0;
}

.mysql-ws__sidebar {
  width: 240px;
  flex: 0 0 240px;
  border-right: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.mysql-ws__list {
  flex: 1 1 auto;
  overflow-y: auto;
  min-height: 0;
}

.mysql-ws__main {
  flex: 1 1 auto;
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  padding: 8px;
}

/* 对象分组切换按钮组 */
.mysql-ws__kind-toggle {
  align-self: flex-start;
}

/* DDL / 权限展示区：等宽字体，可滚动 */
.mysql-ws__ddl {
  flex: 1 1 auto;
  overflow: auto;
  min-height: 120px;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
}

.mysql-ws__ddl-text {
  font-family: 'Cascadia Mono', Consolas, monospace;
  font-size: 12px;
  white-space: pre-wrap;
  word-break: break-all;
  padding: 6px 8px;
}

.mysql-ws__ddl-input {
  font-family: 'Cascadia Mono', Consolas, monospace;
  font-size: 13px;
}

.mysql-ws__grant-line {
  font-family: 'Cascadia Mono', Consolas, monospace;
  font-size: 12px;
  word-break: break-all;
  white-space: pre-wrap;
  padding: 2px 0;
  border-bottom: 1px dashed rgba(var(--v-theme-on-surface), 0.08);
}

.mysql-ws__ddl-hint {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  min-height: 120px;
}

/* 模型画布：绝对定位卡片 + SVG 连线层 */
.mysql-ws__model {
  flex: 1 1 auto;
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 8px;
}

.mysql-ws__model-canvas-wrap {
  flex: 1 1 auto;
  overflow: auto;
  min-height: 0;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
  position: relative;
}

.mysql-ws__model-canvas {
  position: relative;
  min-width: 100%;
}

.mysql-ws__model-lines {
  position: absolute;
  inset: 0;
  pointer-events: none;
}

.mysql-ws__model-line {
  fill: none;
  stroke: rgb(var(--v-theme-primary));
  stroke-width: 1.5;
  opacity: 0.7;
}

.mysql-ws__model-arrow {
  fill: rgb(var(--v-theme-primary));
}

/* 表卡片：绝对定位在网格坐标上 */
.mysql-ws__model-card {
  position: absolute;
  padding: 8px;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.mysql-ws__model-card-title {
  font-size: 13px;
  font-weight: 500;
  display: flex;
  align-items: center;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mysql-ws__model-card-sub {
  opacity: 0.7;
  margin-top: 4px;
}

/* 未连接占位 */
.mysql-ws__placeholder {
  flex: 1 1 auto;
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 120px;
}
</style>
