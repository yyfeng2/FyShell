<template>
  <v-dialog
    :model-value="modelValue"
    max-width="640"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <v-card>
      <v-card-title class="d-flex align-center">
        <v-icon icon="mdi-keyboard-outline" size="small" class="mr-2" />
        按键对应
      </v-card-title>
      <v-divider />
      <v-card-text class="key-mapping-dialog__body">
        <!-- 映射列表 -->
        <div v-if="store.mappings.length > 0" class="key-mapping-dialog__list">
          <div v-for="m in store.mappings" :key="m.id" class="key-mapping-dialog__row">
            <v-chip size="small" variant="tonal" color="primary" class="mr-2 flex-shrink-0">
              {{ formatCombo(m.key_combo) }}
            </v-chip>
            <span class="key-mapping-dialog__desc">
              {{ m.action_type === 'send_string' ? `发送：${m.payload}` : `菜单命令：${m.payload}` }}
            </span>
            <v-spacer />
            <v-btn size="x-small" variant="text" icon="mdi-pencil-outline" title="编辑" @click="startEdit(m)" />
            <v-btn
              size="x-small"
              variant="text"
              icon="mdi-delete-outline"
              color="error"
              title="删除"
              @click="remove(m)"
            />
          </div>
        </div>
        <div v-else class="key-mapping-dialog__empty">暂无键位映射，点击下方按钮添加。</div>

        <!-- 新增/编辑表单 -->
        <v-divider class="my-3" />
        <div class="settings-dialog__row-title">{{ editingId ? '编辑映射' : '添加映射' }}</div>
        <div class="key-mapping-dialog__form">
          <div class="key-mapping-dialog__field-row">
            <v-text-field
              v-model="keyCombo"
              label="键位（如 ctrl+shift+x）"
              density="compact"
              hide-details
              readonly
              placeholder="点击「捕获」后按下键位"
              class="flex-grow-1"
            />
            <v-btn
              size="small"
              variant="tonal"
              prepend-icon="mdi-record-circle-outline"
              @click="startCapture"
            >
              {{ capturing ? '捕获中…' : '捕获' }}
            </v-btn>
          </div>
          <v-select
            v-model="actionType"
            :items="ACTION_ITEMS"
            item-title="title"
            item-value="value"
            label="动作类型"
            density="compact"
            class="mt-3"
          />
          <v-text-field
            v-if="actionType === 'send_string'"
            v-model="payload"
            label="发送的字符串"
            density="compact"
            class="mt-3"
          />
          <v-select
            v-else
            v-model="payload"
            :items="MENU_COMMANDS"
            item-title="title"
            item-value="value"
            label="菜单命令"
            density="compact"
            class="mt-3"
          />
          <div class="key-mapping-dialog__actions">
            <v-btn size="small" variant="text" @click="cancelEdit">取消</v-btn>
            <v-btn size="small" color="primary" variant="tonal" :disabled="!canSave" @click="save">
              保存
            </v-btn>
          </div>
        </div>
      </v-card-text>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
/**
 * KeyMappingDialog —— 键位映射管理对话框
 *
 * 列表展示全部键位映射（键位 + 动作），支持添加/编辑/删除：
 * - 键位经「捕获」按钮捕获 keydown（复用 ui store 的 shortcutOf 归一化）
 * - 动作类型：发送字符串（payload 为任意字符串）/ 执行菜单命令（payload 为菜单 action 名）
 * - 变更即写回后端 SQLite（key_mappings 表），键位拦截器实时生效（useXterm 联动）
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import type { KeyMapping } from '@/api/keyMapping'
import { useKeyMappingStore } from '@/stores/keyMapping'
import { useUiStore } from '@/stores/ui'

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ (e: 'update:modelValue', value: boolean): void }>()

const store = useKeyMappingStore()
const ui = useUiStore()

/** 菜单命令下拉选项（对齐 MenuBar MENUS 的 action 集，全部在 WorkspaceView 接线） */
const MENU_COMMANDS: { title: string; value: string }[] = [
  { title: '新建会话', value: 'new-session' },
  { title: '复制', value: 'copy' },
  { title: '粘贴', value: 'paste' },
  { title: '全选', value: 'select-all' },
  { title: '切换左导航', value: 'toggle-nav' },
  { title: '快速命令栏', value: 'toggle-quickbar' },
  { title: '切换主题（深色/浅色）', value: 'toggle-theme' },
  { title: '传输队列', value: 'transfer' },
  { title: 'SFTP 文件传输', value: 'sftp' },
  { title: '快捷命令', value: 'quick-command' },
  { title: 'SSH 隧道', value: 'tunnel' },
  { title: 'MySQL', value: 'mysql' },
  { title: '会话日志', value: 'session-log' },
  { title: '主密码设置', value: 'master-password' },
  { title: '下一个标签', value: 'next-tab' },
  { title: '设置', value: 'settings' },
]

const ACTION_ITEMS: { title: string; value: string }[] = [
  { title: '发送字符串', value: 'send_string' },
  { title: '执行菜单命令', value: 'menu_command' },
]

/** 编辑中的映射 id（空 = 新增） */
const editingId = ref('')
const keyCombo = ref('')
const actionType = ref<'send_string' | 'menu_command'>('send_string')
const payload = ref('')
/** 键位捕获中（捕获按钮点击后到按下键位/Esc 前） */
const capturing = ref(false)

/** 对话框打开时刷新列表；关闭时取消未完成的键位捕获 */
function onModelValueChange(visible: boolean): void {
  if (visible) {
    void store.reload()
  } else {
    cancelCapture()
  }
}

const canSave = computed(() => keyCombo.value.trim() !== '' && payload.value.trim() !== '')

/** 键位显示格式：分词首字母大写（如 ctrl+shift+x → Ctrl+Shift+X） */
function formatCombo(combo: string): string {
  return combo
    .split('+')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join('+')
}

function startEdit(m: KeyMapping): void {
  editingId.value = m.id
  keyCombo.value = m.key_combo
  actionType.value = m.action_type
  payload.value = m.payload
}

function resetForm(): void {
  editingId.value = ''
  keyCombo.value = ''
  actionType.value = 'send_string'
  payload.value = ''
}

function cancelEdit(): void {
  resetForm()
}

/** 开始键位捕获：捕获期间 window keydown 优先拦截（capture 阶段） */
function startCapture(): void {
  if (capturing.value) return
  capturing.value = true
  window.addEventListener('keydown', captureKeydown, true)
}

/** 捕获 keydown：Esc 取消，其余键归一化为键位组合 */
function captureKeydown(e: KeyboardEvent): void {
  e.preventDefault()
  e.stopPropagation()
  cancelCapture()
  if (e.key === 'Escape') return
  keyCombo.value = ui.shortcutOf(e)
}

/** 结束捕获并移除一次性监听 */
function cancelCapture(): void {
  if (!capturing.value) return
  capturing.value = false
  window.removeEventListener('keydown', captureKeydown, true)
}

async function save(): Promise<void> {
  try {
    await store.save({
      id: editingId.value,
      key_combo: keyCombo.value.trim(),
      action_type: actionType.value,
      payload: payload.value,
    })
    resetForm()
  } catch (e) {
    ui.toast(`保存键位映射失败：${String(e)}`, 'error')
  }
}

async function remove(m: KeyMapping): Promise<void> {
  const ok = await ui.confirm({
    title: '删除键位映射',
    message: `确定删除键位「${formatCombo(m.key_combo)}」的映射吗？`,
    danger: true,
  })
  if (!ok) return
  try {
    await store.remove(m.id)
  } catch (e) {
    ui.toast(`删除键位映射失败：${String(e)}`, 'error')
  }
}

// 对话框打开/关闭时刷新列表、取消捕获
watch(
  () => props.modelValue,
  (visible) => onModelValueChange(visible),
)

onBeforeUnmount(() => {
  // 捕获监听兜底清理
  cancelCapture()
})
</script>

<style scoped>
.key-mapping-dialog__body {
  max-height: 480px;
  overflow-y: auto;
}

.key-mapping-dialog__list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.key-mapping-dialog__row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 8px;
  border-radius: 4px;
  background: rgb(var(--v-theme-on-surface) / 0.04);
}

.key-mapping-dialog__empty {
  font-size: 14px;
  color: rgb(var(--v-theme-on-surface) / 0.5);
  padding: 8px 0;
}

.key-mapping-dialog__field-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
</style>
