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
    /** 会话节点 id（SSH 会话 UUID；会话级 SSH 选项按它覆盖全局值） */
    sessionNodeId?: string
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
 * - sessionNodeId：会话级 SSH 选项（响铃/高亮/登录提示符）按它覆盖全局值
 */
const xterm = useXterm({
  sessionNodeId: props.sessionNodeId,
  encoding: props.encoding,
  fontSize: props.fontSize,
  onData: (data) => {
    // 键盘输入写入（xterm 输出为 UTF-16 字符串，统一按 UTF-8 编码上送）
    terminalStore.writeTerminal(props.sessionId, new TextEncoder().encode(data))
  },
  onResize: (dims) => {
    terminalStore.resizeTerminal(props.sessionId, dims.cols, dims.rows)
  },
  onCwd: (path) => {
    // 终端 OSC 7 实时 cwd：记到 store，供「SFTP 快捷双栏」打开时定位远程栏
    terminalStore.setSessionCwd(props.sessionId, path)
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

// 连接完成补齐首次尺寸同步：onMounted 的首次 fit() 常发生在 connecting 阶段，
// 被 store 的 resizeTerminal 吞掉（未 connected 直接 return），PTY 会永久停在
// request_pty 默认 80x24 而 xterm 是实际宽度——列数错位正是长命令行覆盖/折行错乱的根因。
watch(
  () => terminalStore.sessionStatus[props.sessionId],
  (status) => {
    if (status === 'connected') xterm.fit()
  },
)
// 窗格复用到「已连接」会话时（status 值可能不变不触发上一 watch）也补一次尺寸
watch(
  () => props.sessionId,
  (id) => {
    if (terminalStore.isConnected(id)) xterm.fit()
  },
)

onBeforeUnmount(() => {
  // 组件卸载时清理所有监听与写入器（会话生命周期由 store 管理）
  unbindWriter?.()
  unbindWriter = null
  xterm.dispose()
})

// 菜单「编辑 → 复制/粘贴/全选」入口：WorkspaceView 经模板 ref 就地执行真实剪贴板操作
defineExpose({
  /** 选中内容写入剪贴板（含复制后处理；无选中返回 false） */
  copySelection: () => xterm.copySelection(),
  /** 从剪贴板粘贴到终端 */
  pasteFromClipboard: () => xterm.pasteFromClipboard(),
  /** 全选缓冲区文本 */
  selectAll: () => xterm.selectAllText(),
  /** 解析最后一条提示符中的绝对路径（SFTP 快捷双栏初始目录跟随；只读缓冲零注入） */
  readPromptCwd: () => xterm.readPromptCwd(),
  /** 终端获得焦点（Tab 激活/新开时由 WorkspaceView 调用，含复制会话后自动聚焦） */
  focus: () => xterm.focus(),
})
</script>

<style scoped>
.terminal-pane {
  /* 深色主题全覆盖：容器背景与终端主题一致，避免闪白 */
  width: 100%;
  /* flex 收缩：命令栏/撰写栏内嵌后终端占满剩余高度（无两栏时仍为 100%） */
  flex: 1 1 0;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
  /* 终端背景单一来源 = theme.css --fy-terminal-bg（与 useXterm TERMINAL_BG 同值），避免闪白 */
  background-color: var(--fy-terminal-bg);
}

/* xterm 填满窗格 */
.terminal-pane :deep(.xterm) {
  height: 100%;
}
</style>
