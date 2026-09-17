<template>
  <v-dialog
    :model-value="show"
    persistent
    max-width="480"
    @update:model-value="onDialogChange"
  >
    <v-card>
      <v-card-title class="text-h6">主机密钥确认</v-card-title>
      <v-card-text>
        <div class="hostkey-row">
          <span class="hostkey-label">主机：</span>
          <code class="hostkey-value">{{ current?.host }}</code>
        </div>
        <div class="hostkey-row">
          <span class="hostkey-label">指纹：</span>
          <code class="hostkey-value hostkey-fingerprint">
            {{ current?.fingerprint }}
          </code>
        </div>
        <p class="text-body-2 mt-4 mb-0">
          该主机的密钥尚未被信任。请核对指纹是否与服务器一致，确认后将信任并继续连接；拒绝将中断本次连接。
        </p>
      </v-card-text>
      <v-card-actions>
        <v-spacer />
        <v-btn color="error" variant="text" @click="answer(false)">
          拒绝
        </v-btn>
        <v-btn color="primary" variant="flat" @click="answer(true)">
          信任并连接
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { sshHostkeyAccept, listenHostkeyPrompt } from '@/api/ssh'

/** hostkey-prompt 事件 payload（与后端契约同名同构） */
interface HostkeyPromptPayload {
  id: string
  host: string
  fingerprint: string
}

/** 待确认队列：连接期间可能连续出现多条提示 */
const prompts = ref<HostkeyPromptPayload[]>([])
/** 是否正在处理（防止重复提交） */
const submitting = ref(false)

const show = computed(() => prompts.value.length > 0)
const current = computed(() => prompts.value[0] ?? null)

/** 用户确认/拒绝：调 ssh_hostkey_accept，并弹出下一条待确认项 */
async function answer(accept: boolean): Promise<void> {
  const prompt = current.value
  if (!prompt || submitting.value) return
  submitting.value = true
  try {
    await sshHostkeyAccept(prompt.id, accept)
  } catch (e) {
    console.warn('[hostkey] ssh_hostkey_accept 失败:', e)
  } finally {
    submitting.value = false
    prompts.value = prompts.value.filter((p) => p.id !== prompt.id)
  }
}

/** persistent 对话框不允许点击外部关闭 */
function onDialogChange(value: boolean): void {
  if (!value) {
    // 强制关闭（如 Esc）视作拒绝当前条目，避免连接悬空
    void answer(false)
  }
}

onMounted(() => {
  void listenHostkeyPrompt((payload) => {
    // 去重：同一会话的重复提示只保留一条
    if (!prompts.value.some((p) => p.id === payload.id)) {
      prompts.value.push(payload)
    }
  }).then((unlisten) => {
    unlistenFn = unlisten
  })
})

let unlistenFn: (() => void) | null = null

onBeforeUnmount(() => {
  // 组件卸载时清理 listen（架构红线）
  unlistenFn?.()
  unlistenFn = null
})
</script>

<style scoped>
.hostkey-row {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin-bottom: 8px;
}

.hostkey-label {
  flex-shrink: 0;
}

.hostkey-value {
  word-break: break-all;
}

.hostkey-fingerprint {
  user-select: all;
}
</style>
