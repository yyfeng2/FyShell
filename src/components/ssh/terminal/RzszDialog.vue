<template>
  <v-dialog
    :model-value="show"
    persistent
    max-width="480"
    @update:model-value="onDialogChange"
  >
    <v-card>
      <v-card-title class="text-h6">rz/sz 文件传输</v-card-title>
      <v-divider />

      <!-- 原生对话框打开中：自研 IFileDialog 已弹出/被错过时的可见反馈与取消 -->
      <template v-if="mode === 'picking'">
        <v-card-text>
          <v-alert
            v-if="dialogError"
            type="error"
            variant="tonal"
            density="compact"
            class="mb-2"
            >{{ dialogError }}</v-alert
          >
          <p class="text-body-2 mb-0">
            正在等待选择{{ current?.direction === 'send' ? '要上传的文件' : '保存目录' }}…
            若未弹出选择窗口，请取消后重试。
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
          <v-btn color="primary" variant="text" @click="openNative('recv')">
            接收文件（对端 sz）…
          </v-btn>
          <v-btn color="primary" variant="flat" @click="openNative('send')">
            发送文件（对端 rz）…
          </v-btn>
        </v-card-actions>
      </template>

      <!-- 传输中：进度条 -->
      <template v-else-if="mode === 'progress'">
        <v-card-text>
          <div class="rzsz-file" :title="active?.file_name">
            {{ active?.file_name }}
          </div>
          <v-progress-linear
            :model-value="percent"
            :indeterminate="!(active && active.total > 0)"
            rounded
            height="6"
            color="primary"
          />
          <div class="rzsz-size text-body-2">
            {{ sizeLabel }}
          </div>
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn color="error" variant="text" @click="cancel">取消传输</v-btn>
        </v-card-actions>
      </template>

      <!-- 结束：完成/失败信息 + 手动关闭（提示框显示任务完成，不自动关闭） -->
      <template v-else>
        <v-card-text>
          <v-alert
            :type="ended?.ok ? 'success' : 'error'"
            variant="tonal"
            density="compact"
            class="mb-0 rzsz-error"
            >{{ ended?.message }}</v-alert
          >
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
import { localPickDialog } from '@/api/sftp'
import {
  listenRzszEnd,
  listenRzszProgress,
  listenRzszStart,
  rzszRespond,
} from '@/api/ssh'
import { friendlyError as errText } from '@/utils/errors'

/** rzsz-start 事件 payload（与后端契约同名同构） */
interface RzszStartPayload {
  key: string
  /** 识别的传输方向：recv=对端 sz（选保存目录）/ send=对端 rz（选上传文件）/ null=无法识别 */
  direction: 'recv' | 'send' | null
}

/** rzsz-progress 事件 payload */
interface RzszProgressPayload {
  key: string
  file_name: string
  transferred: number
  total: number
}

/** rzsz-end 事件 payload */
interface RzszEndPayload {
  key: string
  ok: boolean
  message: string
}

/** 对话框模式：idle 隐藏 / picking 原生对话框打开中 / choose 方向手选 / progress 传输中 / ended 结束（完成或失败） */
type DialogMode = 'idle' | 'picking' | 'choose' | 'progress' | 'ended'

/** 待选择的请求队列（连续多次 rz/sz 时排队） */
const pending = ref<RzszStartPayload[]>([])
/** 当前进行中的传输（进度展示） */
const active = ref<RzszProgressPayload | null>(null)
/** 上一次失败的信息 */
const ended = ref<RzszEndPayload | null>(null)
const mode = ref<DialogMode>('idle')
/** 原生对话框等待期间的会话级错误（如调用失败，回退方向手选） */
const dialogError = ref('')

/** picking/传输期间对话框保持可见：防止误触终端；原生选择器由 Rust 自研模态弹出 */
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

/** 呈现队首请求：按方向弹自研原生对话框（recv=选保存目录 / send=选上传文件），
 * 未知方向弹三选手选 */
function presentNext(): void {
  const next = pending.value[0]
  if (!next) {
    mode.value = 'idle'
    return
  }
  if (next.direction === 'send') {
    openNative('send')
  } else if (next.direction === 'recv') {
    openNative('recv')
  } else {
    mode.value = 'choose'
  }
}

/**
 * 调 Rust 自研 Windows 原生文件对话框（IFileDialog 系，资源管理器选择器）：
 * send=选文件 / recv=选目录。取消返回 null 视作放弃；调用失败回退方向手选。
 */
function openNative(m: 'recv' | 'send'): void {
  const key = current.value?.key
  if (!key) return
  mode.value = 'picking'
  dialogError.value = ''
  releasePointerResidue()
  void localPickDialog(m)
    .then((path) => {
      // 等待期内可能已超时/断开（key 被清）：放弃本次
      if (!pending.value.some((p) => p.key === key)) return
      if (path) {
        void respond(key, m, path)
      } else {
        void respond(key, 'cancel', '')
      }
    })
    .catch((e) => {
      dialogError.value = errText(e)
      mode.value = 'choose'
    })
}

/**
 * 弹系统原生对话框前释放指针残留：会话树/标签拖拽使用 setPointerCapture，
 * 若某元素在弹框瞬间仍持有捕获/锁定（pointerup 未达），原生框会继承
 * 「指针被捕获」状态致鼠标不可见。此处幂等释放后即可正常显示。
 */
function releasePointerResidue(): void {
  try {
    document.exitPointerLock?.()
  } catch {
    /* noop */
  }
  const root = document.body ?? document.documentElement
  const all = root.querySelectorAll('*')
  for (let i = 0; i < all.length; i++) {
    const el = all[i] as HTMLElement
    if (!el.hasPointerCapture) continue
    // 规范要求 pointerId；遍历常见 id（0/1）幂等释放
    for (const id of [0, 1]) {
      try {
        if (el.hasPointerCapture(id)) el.releasePointerCapture(id)
      } catch {
        /* noop */
      }
    }
  }
}

/** 取消传输（picking/choose/progress 分支的取消按钮）。
 * 传输进行中取 active.key（rzsz_respond 结束键在会话表即中止循环），
 * 本地先清掉进度占位——rzsz-end 随后到达时不再回弹失败框；
 * 否则（选择阶段）走 respond(cancel) 放弃待选队首。 */
async function cancel(): Promise<void> {
  const activeKey = active.value?.key
  const key = activeKey ?? current.value?.key
  if (!key) return
  if (activeKey) {
    active.value = null
    pending.value = pending.value.filter((p) => p.key !== key)
    dialogError.value = ''
    try {
      await rzszRespond(key, 'cancel', '')
    } catch (e) {
      console.warn('[rzsz] rzsz_respond 失败:', e)
    }
    presentNext()
    return
  }
  await respond(key, 'cancel', '')
}

/** 回传选择：清空待选队列并进入进度态（进度/结束由事件驱动） */
async function respond(
  key: string,
  action: 'recv' | 'send' | 'cancel',
  localPath: string,
): Promise<void> {
  pending.value = pending.value.filter((p) => p.key !== key)
  dialogError.value = ''
  try {
    await rzszRespond(key, action, localPath)
  } catch (e) {
    // 任务已结束/清理（key 不在会话表，如超时自动取消）：不进入进度态
    console.warn('[rzsz] rzsz_respond 失败:', e)
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
      void rzszRespond(key, 'cancel', '')
      pending.value = pending.value.filter((p) => p.key !== key)
      presentNext()
    } else if (mode.value === 'ended') {
      dismiss()
    }
  }
}

/** 关闭结束提示（完成/失败），回到隐藏态或呈现下一条 */
function dismiss(): void {
  ended.value = null
  presentNext()
}

onMounted(() => {
  void listenRzszStart((payload) => {
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
  void listenRzszProgress((payload) => {
    if (active.value && active.value.key === payload.key) {
      active.value = payload
    }
  }).then((unlistenProgress) => {
    unlistenFns.push(unlistenProgress)
  })
  void listenRzszEnd((payload) => {
    // 同 key 的待选请求一并清理（如对话框开着时会话断开自动取消）
    pending.value = pending.value.filter((p) => p.key !== payload.key)
    if (active.value?.key === payload.key) {
      active.value = null
      // 完成与失败都显示在对话框（用户指令：提示框显示进度及任务完成）
      ended.value = payload.ok && !payload.message
        ? { ...payload, message: '传输完成' }
        : payload
      mode.value = 'ended'
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
.rzsz-file {
  font-size: 12px;
  font-weight: bold;
  margin-bottom: 8px;
  word-break: break-all;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.rzsz-size {
  margin-top: 8px;
  color: rgba(var(--v-theme-on-surface), 0.55);
}

.rzsz-error {
  word-break: break-all;
}
</style>
