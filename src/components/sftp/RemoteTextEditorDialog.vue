<template>
  <v-dialog :model-value="modelValue" width="680" persistent>
    <v-card>
      <v-card-title class="d-flex align-center text-subtitle-1">
        编辑远程文件
        <v-spacer />
        <v-btn
          icon="mdi-close"
          size="x-small"
          variant="text"
          title="关闭"
          :disabled="executing"
          @click="close"
        />
      </v-card-title>
      <!-- 文件路径副行（等宽，方便核对目标） -->
      <div class="editor-dialog__path text-medium-emphasis">{{ props.path }}</div>
      <v-divider />
      <v-card-text>
        <!-- 加载/错误反馈：二进制/超大文件在后端拒绝，此处如实展示 -->
        <v-alert
          v-if="errorMsg"
          type="error"
          density="compact"
          variant="tonal"
          class="mb-2"
          :text="errorMsg"
        />
        <div v-if="loading" class="editor-dialog__hint">
          <v-progress-linear indeterminate height="2" class="mb-1" />
          正在加载文件内容…
        </div>
        <v-textarea
          v-else
          v-model="content"
          class="editor-dialog__area"
          density="compact"
          variant="outlined"
          hide-details
          auto-grow
          :rows="18"
          spellcheck="false"
          :disabled="!loaded || !!errorMsg"
          @keydown.ctrl.s.prevent="save"
        />
      </v-card-text>
      <v-card-actions>
        <v-spacer />
        <span class="text-caption text-medium-emphasis">{{ loaded ? `${content.length} 字符` : '' }}</span>
        <v-btn variant="text" :disabled="executing" @click="close">取消</v-btn>
        <v-btn
          color="primary"
          :loading="executing"
          :disabled="!loaded"
          title="Ctrl+S"
          @click="save"
        >
          保存
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { sftpReadText, sftpWriteText } from '@/api/sftp'
import { useUiStore } from '@/stores/ui'
import { friendlyError } from '@/utils/errors'

const props = withDefaults(
  defineProps<{
    /** v-model：对话框显隐 */
    modelValue: boolean
    /** 活动会话 ID（远程侧） */
    sessionId?: string
    /** 远程文件绝对路径 */
    path: string
  }>(),
  {
    sessionId: '',
    path: '',
  },
)

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  /** 保存成功后通知父级刷新当前目录（TRUNCATE 覆盖后大小/时间已变） */
  (e: 'saved'): void
}>()

const ui = useUiStore()

const content = ref('')
const loading = ref(false)
const loaded = ref(false)
const executing = ref(false)
const errorMsg = ref('')

/** 对话框打开：加载远程文件内容（sessionId/path 就绪后触发） */
watch(
  () => props.modelValue,
  async (open) => {
    if (!open) return
    content.value = ''
    loaded.value = false
    errorMsg.value = ''
    if (!props.sessionId) {
      errorMsg.value = '请先选择活动会话'
      return
    }
    loading.value = true
    try {
      content.value = await sftpReadText(props.sessionId, props.path)
      loaded.value = true
    } catch (err) {
      errorMsg.value = friendlyError(err)
      ui.toast(errorMsg.value, 'error')
    } finally {
      loading.value = false
    }
  },
)

/** 保存：全量写回远端，成功后通知父级刷新并关闭 */
async function save(): Promise<void> {
  if (!props.sessionId || !loaded.value || executing.value) return
  executing.value = true
  errorMsg.value = ''
  try {
    await sftpWriteText(props.sessionId, props.path, content.value)
    ui.toast(`已保存 ${props.path}`, 'success')
    emit('saved')
    emit('update:modelValue', false)
  } catch (err) {
    errorMsg.value = friendlyError(err)
    ui.toast(errorMsg.value, 'error')
  } finally {
    executing.value = false
  }
}

function close(): void {
  if (executing.value) return
  emit('update:modelValue', false)
}
</script>

<style scoped>
/* 编辑区：等宽字体（--fy-mono 与终端/数据区一致），不随全局默认字体 */
.editor-dialog__area :deep(textarea) {
  font-family: var(--fy-mono);
  font-size: 13px;
  line-height: 1.5;
}

.editor-dialog__path {
  padding: 0 24px 8px;
  font-family: var(--fy-mono);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.editor-dialog__hint {
  font-size: 12px;
  color: rgba(var(--v-theme-on-surface), 0.6);
}
</style>
