<template>
  <v-card class="tunnel-view" flat>
    <v-card-title class="tunnel-view__title">
      <span>SSH 隧道</span>
      <v-chip v-if="store.listeningCount > 0" size="small" color="success" class="ml-2">
        {{ store.listeningCount }} 个监听中
      </v-chip>
      <v-spacer />
      <!-- 按会话过滤（store 内 filteredRules 本地过滤） -->
      <v-select
        :model-value="store.filterSessionId"
        :items="sessionOptions"
        item-title="name"
        item-value="id"
        density="compact"
        variant="outlined"
        hide-details
        clearable
        placeholder="全部会话"
        min-width="120"
        max-width="220"
        class="tunnel-view__filter"
        @update:model-value="(v: string | null) => store.setFilter(v)"
      />
      <v-btn variant="text" size="small" color="primary" :loading="startingAll" @click="startAll">
        启动全部隧道
      </v-btn>
      <v-btn variant="text" size="small" @click="refresh">刷新</v-btn>
      <v-btn variant="text" size="small" color="primary" @click="openCreate">新建规则</v-btn>
    </v-card-title>
    <v-divider />

    <!-- 表头 -->
    <div v-if="store.filteredRules.length" class="tunnel-view__head">
      <span class="tunnel-view__cell tunnel-view__cell--type">类型</span>
      <span class="tunnel-view__cell">会话</span>
      <span class="tunnel-view__cell">监听地址:端口</span>
      <span class="tunnel-view__cell">目标地址:端口</span>
      <span class="tunnel-view__cell">状态</span>
      <span class="tunnel-view__cell">错误信息</span>
      <span class="tunnel-view__cell tunnel-view__cell--switch">启停</span>
      <span class="tunnel-view__cell tunnel-view__cell--action"></span>
    </div>

    <!-- 规则列表 -->
    <div v-if="!store.filteredRules.length" class="tunnel-view__empty">暂无隧道规则</div>
    <template v-else>
      <div
        v-for="rule in store.filteredRules"
        :key="rule.id"
        class="tunnel-view__row"
      >
        <span class="tunnel-view__cell tunnel-view__cell--type">
          <v-chip size="x-small" :color="kindColor(rule.kind)" label>{{ kindText(rule.kind) }}</v-chip>
        </span>
        <span class="tunnel-view__cell" :title="sessionName(rule.session_id)">
          {{ sessionName(rule.session_id) }}
        </span>
        <span class="tunnel-view__cell tunnel-view__cell--mono">
          {{ rule.listen_host }}:{{ rule.listen_port }}
        </span>
        <span class="tunnel-view__cell">
          <template v-if="rule.kind === 'Socks'">
            <span class="tunnel-view__muted">—</span>
          </template>
          <span v-else :title="`${rule.target_host}:${rule.target_port}`">
            {{ rule.target_host }}:{{ rule.target_port }}
          </span>
        </span>
        <span class="tunnel-view__cell">
          <v-chip size="x-small" :color="statusColor(rule.status)" :title="rule.error ?? ''">
            {{ statusText(rule.status) }}
          </v-chip>
        </span>
        <span class="tunnel-view__cell tunnel-view__cell--error" :title="rule.error ?? ''">
          {{ rule.error || '—' }}
        </span>
        <span class="tunnel-view__cell tunnel-view__cell--switch">
          <v-switch
            :model-value="rule.status === 'Listening'"
            color="primary"
            density="compact"
            hide-details
            :title="rule.status === 'Listening' ? '点击停止' : '点击启动'"
            @update:model-value="(on: boolean | null) => toggle(rule, on === true)"
          />
        </span>
        <span class="tunnel-view__cell tunnel-view__cell--action">
          <v-btn variant="text" size="x-small" @click="openEdit(rule)">编辑</v-btn>
          <v-btn variant="text" size="x-small" color="error" @click="remove(rule)">删除</v-btn>
        </span>
      </div>
    </template>
  </v-card>

  <!-- 新建 / 编辑规则对话框 -->
  <v-dialog v-model="showForm" width="520">
    <v-card class="tunnel-form">
      <v-card-title class="d-flex align-center">
        <v-icon size="small" class="mr-2">
          {{ editingId ? 'mdi-pencil-box-outline' : 'mdi-plus-box-outline' }}
        </v-icon>
        {{ editingId ? '编辑隧道规则' : '新建隧道规则' }}
      <v-spacer />
      <v-btn
        icon="mdi-close"
        size="x-small"
        variant="text"
        title="关闭"
        @click="showForm = false"
      />
      </v-card-title>
      <v-divider />
      <v-card-text>
        <v-form ref="formRef" @submit.prevent="submit">
          <v-row dense>
            <v-col cols="12">
              <div class="fy-field-row">
                <span class="fy-field-row__label">会话</span>
                <v-select
                  v-model="form.session_id"
                  density="compact"
                  :items="sessionOptions"
                  item-title="name"
                  item-value="id"
                  :rules="[rules.required]"
                  placeholder="选择隧道所属会话"
                />
              </div>
            </v-col>
            <v-col cols="12">
              <div class="tunnel-form__label">类型</div>
              <v-btn-toggle v-model="form.kind" mandatory density="comfortable" class="tunnel-form__toggle">
                <v-btn value="Local">本地</v-btn>
                <v-btn value="Remote">远程</v-btn>
                <v-btn value="Socks">SOCKS</v-btn>
              </v-btn-toggle>
            </v-col>
            <v-col cols="8">
              <div class="fy-field-row">
                <span class="fy-field-row__label">监听主机</span>
                <v-text-field
                  v-model="form.listen_host"
                  density="compact"
                  :rules="[rules.required]"
                />
              </div>
            </v-col>
            <v-col cols="4">
              <div class="fy-field-row">
                <span class="fy-field-row__label">监听端口</span>
                <v-text-field
                  v-model.number="form.listen_port"
                  type="number"
                  density="compact"
                  :rules="[rules.required, rules.port]"
                />
              </div>
            </v-col>
            <v-col v-if="form.kind !== 'Socks'" cols="8">
              <div class="fy-field-row">
                <span class="fy-field-row__label">目标主机</span>
                <v-text-field
                  v-model="form.target_host"
                  density="compact"
                  :rules="[rules.required]"
                />
              </div>
            </v-col>
            <v-col v-if="form.kind !== 'Socks'" cols="4">
              <div class="fy-field-row">
                <span class="fy-field-row__label">目标端口</span>
                <v-text-field
                  v-model.number="form.target_port"
                  type="number"
                  density="compact"
                  :rules="[rules.required, rules.port]"
                />
              </div>
            </v-col>
            <v-col v-if="form.kind === 'Socks'" cols="12">
              <v-alert type="info" variant="tonal" density="compact">
                SOCKS 代理无需目标地址：客户端通过本监听端口经会话动态转发。
              </v-alert>
            </v-col>
            <v-col cols="12">
              <v-switch
                v-model="form.enabled"
                label="会话连接时自动启动"
                color="primary"
                density="compact"
                hide-details
              />
            </v-col>
          </v-row>
        </v-form>
      </v-card-text>
      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" @click="showForm = false">取消</v-btn>
        <v-btn color="primary" @click="submit">保存</v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { sessionList } from '@/api/session'
import { tunnelStartAll } from '@/api/tunnel'
import { useTunnelStore, type TunnelKind, type TunnelRule, type TunnelStatus } from '@/stores/tunnel'
import { useUiStore } from '@/stores/ui'
import { friendlyError } from '@/utils/errors'

const store = useTunnelStore()
const ui = useUiStore()

/** 一键全启进行中标志（按钮 loading，避免重复点击） */
const startingAll = ref(false)

/**
 * 一键全启：后端遍历全部规则逐个启动（已在运行中的跳过），
 * 汇总成功/失败计数；结束后拉快照同步状态。
 */
async function startAll(): Promise<void> {
  if (startingAll.value) return
  startingAll.value = true
  try {
    const report = await tunnelStartAll()
    if (report.failed === 0) {
      ui.toast(
        report.started > 0 ? `已启动 ${report.started} 条隧道` : '全部隧道均已在运行中',
        'success',
      )
    } else {
      ui.toast(`启动完成：成功 ${report.started} 个，失败 ${report.failed} 个`, 'warning')
    }
  } catch (e) {
    ui.toast(`一键启动失败：${friendlyError(e)}`, 'error')
  } finally {
    // 快照兜底：无论成败都拉取最新状态
    try {
      await store.refresh()
    } catch {
      /* 忽略 */
    }
    startingAll.value = false
  }
}

// ---------------- 会话数据（下拉选项 + id->名称映射） ----------------

interface SessionOption {
  id: string
  name: string
}

interface SessionNodeRaw {
  id?: unknown
  name?: unknown
  kind?: unknown
  is_folder?: unknown
  children?: unknown
}

/** 递归收集会话叶子节点（兼容 kind tag / is_folder 两种形态） */
function collectSessions(list: unknown, out: SessionOption[]): void {
  const arr = Array.isArray(list) ? list : []
  for (const item of arr) {
    const n = (item ?? {}) as SessionNodeRaw
    const id = n.id == null ? '' : String(n.id)
    const name = String(n.name ?? '')
    if (!id) continue
    const kind = String(n.kind ?? '')
    if (kind === 'session' || n.is_folder === false) {
      if (!out.some((s) => s.id === id)) out.push({ id, name })
    } else {
      collectSessions(n.children, out)
    }
  }
}

const sessionOptions = ref<SessionOption[]>([])

async function loadSessions(): Promise<void> {
  try {
    const list = await sessionList()
    const out: SessionOption[] = []
    collectSessions(Array.isArray(list) ? list : [], out)
    sessionOptions.value = out
  } catch (e) {
    ui.toast(`加载会话列表失败：${friendlyError(e)}`, 'error')
  }
}

function sessionName(sessionId: string): string {
  return sessionOptions.value.find((s) => s.id === sessionId)?.name ?? sessionId
}

// ---------------- 规则列表 ----------------

function kindText(kind: TunnelKind): string {
  switch (kind) {
    case 'Local':
      return '本地'
    case 'Remote':
      return '远程'
    case 'Socks':
      return 'SOCKS'
    default:
      return kind
  }
}

function kindColor(kind: TunnelKind): string {
  switch (kind) {
    case 'Local':
      return 'primary'
    case 'Remote':
      return 'secondary'
    default:
      return 'warning'
  }
}

function statusText(status: string): string {
  switch (status) {
    case 'Listening':
      return '监听中'
    case 'Error':
      return '错误'
    default:
      return '已停止'
  }
}

function statusColor(status: TunnelStatus): string {
  switch (status) {
    case 'Listening':
      return 'success'
    case 'Error':
      return 'error'
    default:
      return 'grey'
  }
}

/** 启停开关：开 = 启动监听（Error 状态重新启动），关 = 停止监听 */
async function toggle(rule: TunnelRule, on: boolean): Promise<void> {
  try {
    if (on) {
      await store.start(rule.id)
    } else {
      await store.stop(rule.id)
    }
  } catch (e) {
    ui.toast(`隧道操作失败：${friendlyError(e)}`, 'error')
    // 操作失败后拉快照，让后端记录的 error 状态反映到列表
    try {
      await store.refresh()
    } catch {
      /* 忽略 */
    }
  }
}

/** 删除前二次确认（危险操作统一走全局确认弹层） */
async function remove(rule: TunnelRule): Promise<void> {
  const ok = await ui.confirm({
    title: '删除隧道规则',
    message: `确定删除该隧道规则吗？（${kindText(rule.kind)} ${rule.listen_host}:${rule.listen_port}）删除后将停止对应监听。`,
    confirmText: '删除',
    danger: true,
  })
  if (!ok) return
  try {
    await store.remove(rule.id)
    ui.toast('隧道规则已删除', 'success')
  } catch (e) {
    ui.toast(`删除失败：${friendlyError(e)}`, 'error')
  }
}

function refresh(): void {
  void loadSessions()
  void store
    .refresh()
    .catch((e) => ui.toast(`加载隧道规则失败：${friendlyError(e)}`, 'error'))
}

// ---------------- 新建 / 编辑表单 ----------------

interface TunnelForm {
  session_id: string
  kind: TunnelKind
  listen_host: string
  listen_port: number | null
  target_host: string
  target_port: number | null
  enabled: boolean
}

function emptyForm(): TunnelForm {
  return {
    session_id: '',
    kind: 'Local',
    listen_host: '127.0.0.1',
    listen_port: null,
    target_host: '',
    target_port: null,
    enabled: true,
  }
}

const showForm = ref(false)
/** 正在编辑的规则 id（null = 新建） */
const editingId = ref<string | null>(null)
const form = ref<TunnelForm>(emptyForm())

const rules = {
  required: (v: unknown): boolean | string => {
    const s = String(v ?? '').trim()
    return s.length > 0 ? true : '此项为必填'
  },
  port: (v: unknown): boolean | string => {
    const n = Number(v)
    if (!Number.isInteger(n) || n < 1 || n > 65535) return '端口需为 1-65535 的整数'
    return true
  },
}

const formRef = ref<{ validate: () => Promise<{ valid: boolean }> } | null>(null)

function openCreate(): void {
  editingId.value = null
  form.value = emptyForm()
  showForm.value = true
}

function openEdit(rule: TunnelRule): void {
  editingId.value = rule.id
  form.value = {
    session_id: rule.session_id,
    kind: rule.kind,
    listen_host: rule.listen_host,
    listen_port: rule.listen_port,
    target_host: rule.target_host,
    target_port: rule.target_port,
    enabled: rule.enabled,
  }
  showForm.value = true
}

async function submit(): Promise<void> {
  if (formRef.value) {
    const { valid } = await formRef.value.validate()
    if (!valid) return
  }
  const f = form.value
  const isSocks = f.kind === 'Socks'
  const rule: TunnelRule = {
    id: editingId.value ?? '',
    session_id: f.session_id,
    kind: f.kind,
    listen_host: f.listen_host.trim(),
    listen_port: Number(f.listen_port ?? 0),
    // 契约 §5.1：Socks 时 target_host 为空串（target_port 置 0）
    target_host: isSocks ? '' : f.target_host.trim(),
    target_port: isSocks ? 0 : Number(f.target_port ?? 0),
    enabled: f.enabled,
    status: 'Stopped',
    error: null,
  }
  try {
    await store.save(rule)
    showForm.value = false
    ui.toast(editingId.value ? '隧道规则已更新' : '隧道规则已创建', 'success')
  } catch (e) {
    ui.toast(`保存隧道规则失败：${friendlyError(e)}`, 'error')
  }
}

onMounted(() => {
  refresh()
})
</script>

<style scoped>
.tunnel-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: rgb(var(--v-theme-surface));
}

.tunnel-view__title {
  display: flex;
  align-items: center;
  gap: 8px;
}

.tunnel-view__filter {
  flex: none;
}

.tunnel-view__head,
.tunnel-view__row {
  display: grid;
  grid-template-columns: 64px minmax(0, 1fr) minmax(140px, 1.1fr) minmax(140px, 1.1fr) 84px minmax(0, 1fr) 56px 104px;
  gap: 8px;
  align-items: center;
  padding: 0 8px;
}

.tunnel-view__head {
  height: 32px;
  font-size: 13px;
  font-weight: 400;
  color: rgba(var(--v-theme-on-surface), 0.7);
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  user-select: none;
}

.tunnel-view__row {
  min-height: 52px;
  font-size: 13px;
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.12);
}

.tunnel-view__cell {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tunnel-view__cell--switch {
  text-align: center;
}

.tunnel-view__cell--action {
  text-align: right;
}

.tunnel-view__cell--mono {
  font-family: var(--fy-font);
  font-size: 13px;
}

.tunnel-view__muted {
  color: rgba(var(--v-theme-on-surface), 0.4);
}

.tunnel-view__empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 200px;
  font-size: 13px;
  color: rgba(var(--v-theme-on-surface), 0.5);
}

/* 表单对话框：深色主题下与 surface 背景一致 */
.tunnel-form__label {
  font-size: 13px;
  color: rgba(var(--v-theme-on-surface), 0.8);
}

.tunnel-form__toggle {
  align-self: flex-start;
}
</style>
