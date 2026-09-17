<template>
  <div ref="containerRef" class="terminal-pane" aria-label="SSH 终端" />
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, watch } from 'vue'
import { useXterm } from '@/composables/useXterm'
import { useTerminalStore } from '@/stores/terminal'
import { debugLog } from '@/api/channels'

const props = withDefaults(
  defineProps<{
    /** 绑定的会话 ID（resize/write 均按会话 ID 路由） */
    sessionId: string
    /** 会话编码，默认 UTF-8（如 "GBK"、"Big5"） */
    encoding?: string
    /** 字号 */
    fontSize?: number
  }>(),
  { encoding: 'UTF-8' },
)

const terminalStore = useTerminalStore()

/**
 * useXterm 组合式函数：
 * - onData/onResize：按会话传输类型路由到各自命令
 *   （SSH → ssh_write/ssh_resize，本地/Telnet/串口 → 各自 byte-stream 命令）
 * - write：注册到 store，Rust 侧 Channel 输出按会话 ID 分发到这里
 */
const xterm = useXterm({
  encoding: props.encoding,
  fontSize: props.fontSize,
  onData: (data) => {
    // 键盘输入写入（xterm 输出为 UTF-16 字符串，统一按 UTF-8 编码上送）
    terminalStore.writeTerminal(props.sessionId, new TextEncoder().encode(data))
  },
  onResize: (dims) => {
    terminalStore.resizeTerminal(props.sessionId, dims.cols, dims.rows)
  },
})

// 模板 ref 绑定：ref="containerRef" 需要 setup 作用域存在同名 ref，
// 缺失会导致模板 ref 静默失败、xterm 容器无法初始化（终端黑屏）
const { containerRef } = xterm

/** 输出写入器注销函数 */
let unbindWriter: (() => void) | null = null

onMounted(() => {
  debugLog(`TerminalPane onMounted: ${props.sessionId}`)
  try {
    xterm.open()
  } catch (e) {
    debugLog(`TerminalPane xterm.open failed: ${e instanceof Error ? e.message : String(e)}`)
    throw e
  }
  xterm.attachGlobalKeyPassthrough()
  // 把本窗格写入器绑到会话输出流（按会话 ID 路由）
  unbindWriter = terminalStore.bindPaneWriter(props.sessionId, (data) => {
    xterm.write(data)
  })
  // 窗格重挂载（v-if 场景）时立即校准一次尺寸
  xterm.fit()
})

// 编码设置生效：会话配置变更时切换解码器
watch(
  () => props.encoding,
  (encoding) => {
    xterm.setEncoding(encoding)
  },
)

// 字号设置生效：运行中改字号需更新终端选项并重新 fit 布局（字号变化会改变行列数）
watch(
  () => props.fontSize,
  (size) => {
    if (typeof size !== 'number' || size <= 0) return
    xterm.setFontSize(size)
    xterm.fit()
  },
)

// 会话 ID 变化（窗格复用场景）：重新绑定写入器
watch(
  () => props.sessionId,
  () => {
    unbindWriter?.()
    unbindWriter = terminalStore.bindPaneWriter(props.sessionId, (data) => {
      xterm.write(data)
    })
  },
)

onBeforeUnmount(() => {
  // 组件卸载时清理所有监听与写入器（会话生命周期由 store 管理）
  unbindWriter?.()
  unbindWriter = null
  xterm.dispose()
})
</script>

<style scoped>
.terminal-pane {
  /* 深色主题全覆盖：容器背景与终端主题一致，避免闪白 */
  width: 100%;
  height: 100%;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
  background-color: #1e1e1e;
}

/* xterm 填满窗格 */
.terminal-pane :deep(.xterm) {
  height: 100%;
}
</style>
