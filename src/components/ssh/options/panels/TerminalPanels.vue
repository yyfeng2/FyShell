<template>
  <!-- 键盘：键位映射管理（复用 keyMappingStore CRUD） -->
  <template v-if="page === 'keyboard'">
    <div class="settings-dialog__section-title">键盘</div>
    <div class="settings-dialog__row">
      <div>
        <div class="settings-dialog__row-title">键位映射（{{ mappings.length }} 条）</div>
        <div class="settings-dialog__row-desc">按键组合映射为发送字符串或执行菜单命令，对终端全局生效。</div>
      </div>
      <v-btn size="small" color="primary" variant="tonal" prepend-icon="mdi-plus" @click="openAdd">
        新增映射
      </v-btn>
    </div>
    <v-list density="compact" class="keyboard-mapping-list">
      <v-list-item v-for="m in mappings" :key="m.id" class="keyboard-mapping-item">
        <template #prepend>
          <kbd class="mapping-combo">{{ m.key_combo }}</kbd>
        </template>
        <span class="mapping-action">
          {{ m.action_type === 'send_string' ? '发送' : '执行命令' }}：
          <code>{{ m.payload }}</code>
        </span>
        <template #append>
          <v-btn
            icon="mdi-pencil-outline"
            size="x-small"
            variant="text"
            title="编辑"
            @click="openEdit(m)"
          />
          <v-btn
            icon="mdi-delete-outline"
            size="x-small"
            variant="text"
            title="删除"
            @click="removeMapping(m.id)"
          />
        </template>
      </v-list-item>
      <v-list-item v-if="mappings.length === 0">
        <span class="text-caption text-medium-emphasis">暂无键位映射</span>
      </v-list-item>
    </v-list>

    <!-- 新增/编辑对话框 -->
    <v-dialog :model-value="formVisible" width="420">
      <v-card>
        <v-card-title class="text-subtitle-1">{{ editingId ? '编辑键位映射' : '新增键位映射' }}</v-card-title>
        <v-card-text>
          <v-text-field
            v-model="formCombo"
            label="键位组合"
            density="compact"
            placeholder="例如 ctrl+shift+x"
            hint="格式：修饰键用 + 连接（ctrl/alt/shift/meta）"
            persistent-hint
            class="mb-3"
          />
          <v-select
            v-model="formActionType"
            :items="ACTION_TYPES"
            item-title="title"
            item-value="value"
            label="动作类型"
            density="compact"
            class="mb-3"
          />
          <v-text-field
            v-model="formPayload"
            :label="formActionType === 'send_string' ? '发送的字符串' : '菜单命令 action 名'"
            density="compact"
          />
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="formVisible = false">取消</v-btn>
          <v-btn color="primary" @click="saveMapping">保存</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </template>

  <!-- VT 模式：终端类型 -->
  <template v-else-if="page === 'vt-mode'">
    <div class="settings-dialog__section-title">VT 模式</div>
    <v-select
      :model-value="opts.vtTermType"
      :items="TERM_TYPES"
      label="终端类型（TERM）"
      class="settings-dialog__field"
      @update:model-value="(v: unknown) => opts.setVtTermType(String(v))"
    />
    <div class="settings-dialog__hint">
      连接时请求 PTY 的终端类型（服务器据此调整转义序列行为）；对新连接生效。
    </div>
  </template>

  <!-- 终端高级：WebGL 渲染开关 + 滚动缓冲说明 -->
  <template v-else-if="page === 'term-advanced'">
    <div class="settings-dialog__section-title">高级</div>
    <v-switch
      :model-value="opts.termWebgl"
      label="WebGL 渲染"
      color="primary"
      density="compact"
      hide-details
      class="mb-2"
      @update:model-value="(v: unknown) => opts.setTermWebgl(!!v)"
    />
    <div class="settings-dialog__hint">
      关闭后回退 DOM 渲染（排查显卡驱动问题时使用）；滚动缓冲行数等其余终端项在全局设置的「终端」分区调整。
    </div>
  </template>
</template>

<script setup lang="ts">
/**
 * TerminalPanels —— 键盘 / VT 模式 / 高级（SSH 选项）
 */
import { computed, ref } from 'vue'
import type { KeyMapping, KeyMappingActionType } from '@/api/keyMapping'
import { useKeyMappingStore } from '@/stores/keyMapping'
import { useSshOptionsStore } from '@/stores/sshOptions'
import { useUiStore } from '@/stores/ui'

/** 展示页（keyboard | vt-mode | term-advanced） */
defineProps<{ page: string }>()

const keyMappingStore = useKeyMappingStore()
const opts = useSshOptionsStore()
const ui = useUiStore()

/** 全量映射（computed 直读 store，save/remove 后自动刷新） */
const mappings = computed(() => keyMappingStore.mappings)
const formVisible = ref(false)
const editingId = ref<string | null>(null)
const formCombo = ref('')
const formActionType = ref<KeyMappingActionType>('send_string')
const formPayload = ref('')

const ACTION_TYPES: { value: KeyMappingActionType; title: string }[] = [
  { value: 'send_string', title: '发送字符串' },
  { value: 'menu_command', title: '执行菜单命令' },
]

const TERM_TYPES = ['xterm-256color', 'xterm', 'vt100', 'vt102', 'vt220', 'ansi', 'linux']

// 键位映射列表（ensureLoaded 幂等，computed 直读保证编辑后实时刷新）
void keyMappingStore.ensureLoaded()

function openAdd(): void {
  editingId.value = null
  formCombo.value = ''
  formActionType.value = 'send_string'
  formPayload.value = ''
  formVisible.value = true
}

function openEdit(m: KeyMapping): void {
  editingId.value = m.id
  formCombo.value = m.key_combo
  formActionType.value = m.action_type
  formPayload.value = m.payload
  formVisible.value = true
}

async function saveMapping(): Promise<void> {
  const combo = formCombo.value.trim()
  const payload = formPayload.value
  if (!combo || !payload) {
    ui.toast('键位组合与动作载荷不能为空', 'error')
    return
  }
  try {
    await keyMappingStore.save({
      id: editingId.value ?? '',
      key_combo: combo,
      action_type: formActionType.value,
      payload,
    })
    formVisible.value = false
  } catch (e) {
    ui.toast(`保存失败：${String(e)}`, 'error')
  }
}

async function removeMapping(id: string): Promise<void> {
  try {
    await keyMappingStore.remove(id)
  } catch (e) {
    ui.toast(`删除失败：${String(e)}`, 'error')
  }
}
</script>

<style scoped>
.keyboard-mapping-list {
  max-height: 240px;
  overflow-y: auto;
}

.mapping-combo {
  display: inline-block;
  padding: 1px 8px;
  border: 1px solid rgb(var(--v-theme-on-surface) / 0.2);
  border-radius: 4px;
  font-size: 12px;
  margin-right: 8px;
  background: rgb(var(--v-theme-on-surface) / 0.04);
}

.mapping-action {
  font-size: 12px;
}
</style>
