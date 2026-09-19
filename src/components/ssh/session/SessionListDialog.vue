<script setup lang="ts">
/**
 * SessionListDialog —— "打开会话"对话框（参考 Xshell 会话管理器）
 *
 * 结构：标题 + 工具栏（新建下拉/删除/属性/连接 + 右端搜索框）
 *       + 类型过滤栏（"所有会话"下拉 + 刷新）+ 7 列虚拟滚动表格
 *       （名称/主机/端口/协议/用户名/说明/修改时间，复用 AdvancedTable）。
 * 交互：行点击选中、双击打开连接、右键菜单（打开/删除）、搜索框回车打开首个匹配。
 * 动作：打开/删除/属性/新建/刷新均 emit 回父级，复用 WorkspaceView 现有函数；
 *       搜索/过滤/选中/右键状态内聚在本组件内。
 */
import { computed, reactive, ref, watch } from 'vue'
import AdvancedTable, { type AdvancedTableColumn } from '@/components/common/AdvancedTable.vue'

/** 会话列表项（父级 SessionNode 的结构子集，config 按字段容错读取） */
export interface SessionListItem {
  id: string
  name: string
  config?: {
    color?: string | null
    host?: string
    port?: number
    username?: string
    session_type?: string | null
    description?: string | null
    updated_at?: number | null
  } & Record<string, unknown>
}

const props = defineProps<{
  modelValue: boolean
  /** 扁平化的已添加会话列表（父级 sessionListFlat） */
  sessions: SessionListItem[]
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  (e: 'open-session', node: SessionListItem): void
  (e: 'properties', node: SessionListItem): void
  (e: 'delete', node: SessionListItem): void
  (e: 'new-session'): void
  (e: 'new-folder'): void
  (e: 'refresh'): void
}>()

// ---------------- 工具栏 ----------------

const search = ref('')
/** 当前选中的会话（行点击；工具栏删除/属性/连接的作用目标） */
const selectedId = ref<string | null>(null)
const selectedNode = computed(
  () => props.sessions.find((s) => s.id === selectedId.value) ?? null,
)

// ---------------- 类型过滤栏 ----------------

const typeFilter = ref('all')
const typeItems = [
  { title: '所有会话', value: 'all' },
  { title: 'SSH', value: 'ssh' },
  { title: 'Telnet', value: 'telnet' },
  { title: 'RLOGIN', value: 'rlogin' },
  { title: 'MySQL', value: 'mysql' },
  { title: 'Redis', value: 'redis' },
  { title: '串口', value: 'serial' },
]

/** session_type -> 协议显示名（"ssh" 缺省） */
function protocolLabel(sessionType: string): string {
  if (sessionType === 'mysql') return 'MySQL'
  if (sessionType === 'redis') return 'Redis'
  if (sessionType === 'telnet') return 'Telnet'
  if (sessionType === 'rlogin') return 'RLOGIN'
  if (sessionType === 'serial') return '串口'
  return 'SSH'
}

/** Unix 秒 -> "YYYY-MM-DD HH:mm"（无值显示 "-"） */
function formatUpdated(unixSec: number | null | undefined): string {
  if (!unixSec) return '-'
  const d = new Date(unixSec * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

// ---------------- 表格 ----------------

const columns: AdvancedTableColumn[] = [
  { key: 'name', title: '名称', width: 150, sortable: true },
  { key: 'host', title: '主机', width: 140, sortable: true },
  { key: 'port', title: '端口', width: 60, align: 'end', sortable: true },
  { key: 'protocol', title: '协议', width: 70 },
  { key: 'username', title: '用户名', width: 100 },
  { key: 'description', title: '说明', width: 110 },
  { key: 'updatedText', title: '修改时间', width: 130, sortable: true },
]

/** 搜索 + 类型过滤后的表格行（_node 挂原会话节点供动作回传） */
const filteredRows = computed<Record<string, unknown>[]>(() => {
  const kw = search.value.trim().toLowerCase()
  return props.sessions
    .filter((s) => {
      if (typeFilter.value !== 'all') {
        const st = (s.config?.session_type as string) ?? 'ssh'
        if (st !== typeFilter.value) return false
      }
      if (!kw) return true
      const host = (s.config?.host as string) ?? ''
      const username = (s.config?.username as string) ?? ''
      return (
        s.name.toLowerCase().includes(kw) ||
        host.toLowerCase().includes(kw) ||
        username.toLowerCase().includes(kw)
      )
    })
    .map((s) => ({
      id: s.id,
      name: s.name,
      host: (s.config?.host as string) || '-',
      port: s.config?.port ?? '',
      protocol: protocolLabel((s.config?.session_type as string) ?? 'ssh'),
      username: (s.config?.username as string) || '-',
      description: (s.config?.description as string) || '-',
      updatedText: formatUpdated(s.config?.updated_at as number | null | undefined),
      _node: s,
    }))
})

/** 过滤/搜索变化后同步清空选中（AdvancedTable 内部选中索引同样按 rows 变化重置） */
watch(
  () => [search.value, typeFilter.value, props.sessions],
  () => {
    selectedId.value = null
  },
)

function onRowSelect(row: Record<string, unknown>): void {
  selectedId.value = String(row.id)
}

function onRowOpen(row: Record<string, unknown>): void {
  emit('open-session', row._node as SessionListItem)
}

// ---------------- 行右键菜单（打开/删除） ----------------

const ctx = reactive({ visible: false, x: 0, y: 0, node: null as SessionListItem | null })

function onRowContextmenu(payload: { row: Record<string, unknown>; event: MouseEvent }): void {
  ctx.x = payload.event.clientX
  ctx.y = payload.event.clientY
  ctx.node = payload.row._node as SessionListItem
  ctx.visible = true
}

/** 搜索框回车：打开首个匹配会话 */
function openFirstMatch(): void {
  const first = filteredRows.value[0]
  if (first) emit('open-session', first._node as SessionListItem)
}
</script>

<template>
  <v-dialog
    :model-value="modelValue"
    width="840"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <v-card>
      <v-card-title class="d-flex align-center text-subtitle-1">会话
        <v-spacer />
        <v-btn
        icon="mdi-close"
        size="x-small"
        variant="text"
        title="关闭"
        @click="emit('update:modelValue', false)"
        />
      </v-card-title>
      <v-divider />
      <!-- 工具栏：新建下拉 | 删除 属性 连接 + 右端搜索框 -->
      <div class="session-dlg__toolbar">
        <v-menu location="bottom start">
          <template #activator="{ props: activatorProps }">
            <v-btn
              v-bind="activatorProps"
              icon="mdi-plus"
              size="22"
              variant="text"
              color="primary"
              title="新建"
            />
          </template>
          <v-list density="compact">
            <v-list-item @click="emit('new-session')">
              <v-list-item-title>新建会话</v-list-item-title>
            </v-list-item>
            <v-list-item @click="emit('new-folder')">
              <v-list-item-title>新建文件夹</v-list-item-title>
            </v-list-item>
          </v-list>
        </v-menu>
        <v-divider vertical inset class="mx-1" />
        <v-btn
          icon="mdi-delete"
          size="22"
          variant="text"
          color="error"
          :disabled="!selectedNode"
          title="删除会话"
          @click="emit('delete', selectedNode!)"
        />
        <v-btn
          icon="mdi-cog-outline"
          size="22"
          variant="text"
          :disabled="!selectedNode"
          title="属性"
          @click="emit('properties', selectedNode!)"
        />
        <v-btn
          icon="mdi-lan-connect"
          size="22"
          variant="text"
          color="success"
          :disabled="!selectedNode"
          title="连接选中的会话"
          @click="emit('open-session', selectedNode!)"
        />
        <v-spacer />
        <v-text-field
          v-model="search"
          density="compact"
          variant="outlined"
          hide-details
          placeholder="搜索名称/主机/用户名"
          append-inner-icon="mdi-magnify"
          class="session-dlg__search"
          @keydown.enter="openFirstMatch"
        />
      </div>
      <!-- 类型过滤栏："所有会话"下拉 + 刷新 -->
      <div class="session-dlg__filter">
        <v-select
          v-model="typeFilter"
          density="compact"
          variant="outlined"
          hide-details
          :items="typeItems"
          class="session-dlg__type"
        />
        <v-btn icon="mdi-refresh" size="22" variant="text" title="刷新列表" @click="emit('refresh')" />
      </div>
      <!-- 7 列虚拟滚动表格 -->
      <div class="session-dlg__table">
        <AdvancedTable
          :columns="columns"
          :rows="filteredRows"
          row-key="id"
          :selectable="true"
          @row-click="onRowSelect"
          @row-dblclick="onRowOpen"
          @row-contextmenu="onRowContextmenu"
        />
      </div>
      <!-- 行右键菜单：打开/删除会话 -->
      <v-menu
        v-model="ctx.visible"
        :target="[ctx.x, ctx.y]"
        location="bottom start"
        :close-on-content-click="true"
      >
        <v-list density="compact">
          <v-list-item @click="emit('open-session', ctx.node!)">
            <v-list-item-title>打开连接</v-list-item-title>
          </v-list-item>
          <v-list-item @click="emit('delete', ctx.node!)">
            <v-list-item-title class="text-error">删除会话</v-list-item-title>
          </v-list-item>
        </v-list>
      </v-menu>
    </v-card>
  </v-dialog>
</template>

<style scoped>
/* 工具栏：图标按钮单行分组 + 右端搜索框（ToolBar 同款 22px 紧凑风格） */
.session-dlg__toolbar {
  display: flex;
  align-items: center;
  padding: 6px 0.2em 2px;
}

.session-dlg__search {
  width: 200px;
  flex: 0 0 auto;
}

/* 类型过滤栏 */
.session-dlg__filter {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 2px 0.2em 6px;
}

.session-dlg__type {
  width: 160px;
  flex: 0 0 auto;
}

/* 表格容器：AdvancedTable height:100% 需要定高 */
.session-dlg__table {
  height: 380px;
  padding: 0 0.2em 0.2em;
}
</style>
