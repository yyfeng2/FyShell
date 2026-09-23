<template>
  <v-dialog
    :model-value="show"
    persistent
    max-width="480"
    @update:model-value="onDialogChange"
  >
    <v-card>
      <v-card-title class="text-h6">ZMODEM 传输</v-card-title>
      <v-divider />
      <!-- 会话级错误（如选择器打开失败）：对话框内可见提示 -->
      <v-alert
        v-if="dialogError"
        type="error"
        variant="tonal"
        density="compact"
        class="mx-4 mt-3"
        >{{ dialogError }}</v-alert
      >

      <!-- 选择器打开中：系统选择器未出现/被错过时的可见反馈 -->
      <template v-if="mode === 'picking'">
        <v-card-text>
          <p class="text-body-2 mb-0">
            正在等待选择{{ current?.direction === 'send' ? '要上传的文件' : '保存目录' }}…
            若未弹出选择窗口，请直接取消后重试。
          </p>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn color="error" variant="text" @click="cancel">取消</v-btn>
        </v-card-actions>
      </template>

      <!-- 方向未知兜底：哨兵帧无法区分 rz/sz 时手选 -->
      <template v-else-if="mode === 'choose'">
        <v-card-text>
          <p class="text-body-2 mb-0">
            无法自动识别传输方向。请选择操作：
          </p>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn color="error" variant="text" @click="cancel">取消</v-btn>
          <v-btn color="primary" variant="text" @click="pickRecvFile">
            接收文件（对端 sz）…
          </v-btn>
          <v-btn color="primary" variant="flat" @click="pickSendFile">
            发送文件（对端 rz）…
          </v-btn>
        </v-card-actions>
      </template>

      <!-- 传输中：进度条 -->
      <template v-else-if="mode === 'progress'">
        <v-card-text>
          <div class="zmodem-file" :title="active?.file_name">
            {{ active?.file_name }}
          </div>
          <v-progress-linear
            :model-value="percent"
            :indeterminate="!(active && active.total > 0)"
            rounded
            height="6"
            color="primary"
          />
          <div class="zmodem-size text-body-2">
            {{ sizeLabel }}
          </div>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn color="error" variant="text" @click="cancel">取消传输</v-btn>
        </v-card-actions>
      </template>

      <!-- 失败：错误信息 + 手动关闭 -->
      <template v-else>
        <v-card-text>
          <p class="text-body-2 mb-0 zmodem-error">{{ ended?.message }}</p>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn color="primary" variant="flat" @click="dismiss">关闭</v-btn>
        </v-card-actions>
      </template>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import {
  listenZmodemEnd,
  listenZmodemProgress,
  listenZmodemStart,
  zmodemRespond,
} from '@/api/ssh'
import { friendlyError as errText } from '@/utils/errors'

/** zmodem-start 事件 payload（与后端契约同名同构） */
interface ZmodemStartPayload {
  key: string
  /** 识别的传输方向：recv=对端 sz（选保存目录）/ send=对端 rz（选上传文件）/ null=无法识别 */
  direction: 'recv' | 'send' | null
}

/** zmodem-progress 事件 payload */
interface ZmodemProgressPayload {
  key: string
  file_name: string
  transferred: number
  total: number
}

/** zmodem-end 事件 payload */
interface ZmodemEndPayload {
  key: string
  ok: boolean
  message: string
}

/** 对话框模式：idle 隐藏 / picking 系统选择器打开中（对话框显示等待提示）/ choose 方向手选 / progress 传输中 / failed 失败 */
type DialogMode = 'idle' | 'picking' | 'choose' | 'progress' | 'failed'

/** 待选择的请求队列（连续多次 rz/sz 时排队） */
const pending = ref<ZmodemStartPayload[]>([])
/** 当前进行中的传输（进度展示） */
const active = ref<ZmodemProgressPayload | null>(null)
/** 上一次失败的信息 */
const ended = ref<ZmodemEndPayload | null>(null)
const mode = ref<DialogMode>('idle')
/** 对话框内展示的会话级错误（如选择器打开失败） */
const dialogError = ref('')

/** picking 期间对话框保持可见：选择器被错过/未出现时终端无任何输出的 60s
 * 等待窗内，这是用户唯一能看到的反馈（并可立即取消） */
const show = computed(() => mode.value !== 'idle')
const current = computed(() => pending.value[0] ?? null)

const percent = computed(() => {
  const a = active.value
  if (!a || a.total <= 0) return 0
  return Math.min(100, Math.round((a.transferred / a.total) * 100))
})

const sizeLabel = computed(() => {
  const a = active.value
  if (!a) return ''
  const fmt = (n: number): string => {
    if (n >= 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`
    if (n >= 1024) return `${(n / 1024).toFixed(1)} KB`
    return `${n} B`
  }
  return a.total > 0 ? `${fmt(a.transferred)} / ${fmt(a.total)}` : fmt(a.transferred)
})

/** 呈现队首请求：方向已知时直接弹对应系统选择器，未知时弹三选手选 */
function presentNext(): void {
  const next = pending.value[0]
  if (!next) {
    mode.value = 'idle'
    return
  }
  if (next.direction === 'recv') {
    void pickRecvFile()
  } else if (next.direction === 'send') {
    void pickSendFile()
  } else {
    mode.value = 'choose'
  }
}

/** 接收文件：目录选择器，选中后回传 recv；取消选择视作放弃传输 */
async function pickRecvFile(): Promise<void> {
  const key = current.value?.key
  if (!key) return
  mode.value = 'picking'
  dialogError.value = ''
  let dir: string | string[] | null
  try {
    dir = await open({ directory: true, multiple: false, title: '选择保存目录' })
  } catch (e) {
    // 选择器异常（会话级错误）：对话框内提示并回退三选手选
    dialogError.value = errText(e)
    mode.value = 'choose'
    return
  }
  if (typeof dir === 'string') {
    await respond(key, 'recv', dir)
  } else {
    await respond(key, 'cancel', '')
  }
}

/** 发送文件：文件选择器，选中后回传 send；取消选择视作放弃传输 */
async function pickSendFile(): Promise<void> {
  const key = current.value?.key
  if (!key) return
  mode.value = 'picking'
  dialogError.value = ''
  let file: string | string[] | null
  try {
    file = await open({ multiple: false, title: '选择要上传的文件' })
  } catch (e) {
    // 选择器异常（会话级错误）：对话框内提示并回退三选手选
    dialogError.value = errText(e)
    mode.value = 'choose'
    return
  }
  if (typeof file === 'string') {
    await respond(key, 'send', file)
  } else {
    await respond(key, 'cancel', '')
  }
}

/** 取消传输 */
async function cancel(): Promise<void> {
  const key = current.value?.key
  if (!key) return
  await respond(key, 'cancel', '')
}

/** 回传选择：清空待选队列并进入进度态（进度/结束由事件驱动） */
async function respond(
  key: string,
  action: 'recv' | 'send' | 'cancel',
  localPath: string,
): Promise<void> {
  pending.value = pending.value.filter((p) => p.key !== key)
  try {
    await zmodemRespond(key, action, localPath)
  } catch (e) {
    // 任务已结束/清理（key 不在会话表，如超时自动取消）：不进入进度态
    console.warn('[zmodem] zmodem_respond 失败:', e)
    presentNext()
    return
  }
  if (action === 'cancel') {
    presentNext()
  } else {
    // 先占位（后端 FileStarted/开始发送后立即推首条进度）
    active.value = { key, file_name: '', transferred: 0, total: 0 }
    mode.value = 'progress'
  }
}

/** persistent 对话框不允许点击外部关闭；强制关闭视作取消 */
function onDialogChange(value: boolean): void {
  if (!value) {
    if (mode.value === 'choose' && current.value) {
      const key = current.value.key
      void zmodemRespond(key, 'cancel', '')
      pending.value = pending.value.filter((p) => p.key !== key)
      presentNext()
    } else if (mode.value === 'failed') {
      dismiss()
    }
  }
}

/** 关闭失败提示 */
function dismiss(): void {
  ended.value = null
  presentNext()
}

onMounted(() => {
  void listenZmodemStart((payload) => {
    // 去重：同一连接的重复提示只保留一条
    if (!pending.value.some((p) => p.key === payload.key)) {
      pending.value.push(payload)
    }
    // 空闲时呈现队首（选择器打开中/传输中/已弹三选一时排队，稍后呈现）
    if (mode.value === 'idle') {
      presentNext()
    }
  }).then((unlistenStart) => {
    unlistenFns.push(unlistenStart)
  })
  void listenZmodemProgress((payload) => {
    if (active.value && active.value.key === payload.key) {
      active.value = payload
    }
  }).then((unlistenProgress) => {
    unlistenFns.push(unlistenProgress)
  })
  void listenZmodemEnd((payload) => {
    // 同 key 的待选请求一并清理（如对话框开着时会话断开自动取消）
    pending.value = pending.value.filter((p) => p.key !== payload.key)
    if (active.value?.key === payload.key) {
      active.value = null
      if (payload.ok) {
        // 成功自动关闭；有待选请求时呈现下一条
        presentNext()
      } else {
        ended.value = payload
        mode.value = 'failed'
      }
    } else if (
      (mode.value === 'choose' || mode.value === 'picking') &&
      pending.value.length === 0
    ) {
      // 待选请求被清空（如选择等待中超时自动取消）：回到隐藏态
      mode.value = 'idle'
    }
  }).then((unlistenEnd) => {
    unlistenFns.push(unlistenEnd)
  })
})

const unlistenFns: Array<() => void> = []

onBeforeUnmount(() => {
  // 组件卸载时清理所有 listen（架构红线）
  unlistenFns.forEach((fn) => fn())
  unlistenFns.length = 0
})
</script>

<style scoped>
.zmodem-file {
  font-size: 13px;
  font-weight: bold;
  margin-bottom: 8px;
  word-break: break-all;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.zmodem-size {
  margin-top: 8px;
  color: rgba(var(--v-theme-on-surface), 0.55);
}

.zmodem-error {
  word-break: break-all;
}

</style>
