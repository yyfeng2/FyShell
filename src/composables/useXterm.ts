/**
 * xterm.js 终端初始化组合式函数
 *
 * 关键设计：
 * - WebGL renderer + fit addon；监听 webglcontextlost 自动重建 WebGL renderer
 *   （xterm.js #4128 教训：不全局禁用 GPU，只在上下文丢失时重建）
 * - Rust 侧透传原始字节流，ANSI 转义序列解析交给 xterm.js
 * - 输出侧按会话编码用 TextDecoder 解码（支持 UTF-8/GBK/Big5 等），
 *   跨批量边界的多字节字符由 stream 模式正确衔接
 */
import { onScopeDispose, ref, type Ref } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebglAddon } from '@xterm/addon-webgl'
import '@xterm/xterm/css/xterm.css'

/** 深色主题配色（终端背景/前景/光标 + 完整 16 色 ANSI 调色板） */
export const TERMINAL_DARK_THEME = {
  background: '#1e1e1e',
  foreground: '#d4d4d4',
  cursor: '#528bff',
  cursorAccent: '#1e1e1e',
  selectionBackground: '#3a4b5c',
  black: '#000000',
  red: '#cd313c',
  green: '#0dbc79',
  yellow: '#e5e510',
  blue: '#2472c8',
  magenta: '#bc3fbc',
  cyan: '#11a8cd',
  white: '#d4d4d4',
  brightBlack: '#666666',
  brightRed: '#f14c4c',
  brightGreen: '#23d18b',
  brightYellow: '#f5f543',
  brightBlue: '#3b8eea',
  brightMagenta: '#d67df6',
  brightCyan: '#29d2cd',
  brightWhite: '#ffffff',
} as const

export interface TerminalDimensions {
  cols: number
  rows: number
}

export interface UseXtermOptions {
  /** 键盘输入回调（xterm 原始字符串，含编辑键序列） */
  onData?: (data: string) => void
  /** 容器尺寸变化并完成 fit 后触发（防抖），用于按会话 ID 路由 resize */
  onResize?: (dims: TerminalDimensions) => void
  /** 会话编码，默认 UTF-8（如 "GBK"、"Big5"） */
  encoding?: string
  /** 字号，默认 14 */
  fontSize?: number
  /**
   * 打开终端后是否立即 focus（默认 false）。
   * 多窗格/后台标签等场景会因盲抢焦点打断输入，交由上层自行决定。
   */
  autoFocus?: boolean
}

/** 规范化编码名：TextDecoder 只认标准标签，未知标签回退 UTF-8 */
function normalizeEncoding(encoding: string): string {
  const name = encoding.trim().toLowerCase()
  if (!name || name === 'utf8' || name === 'utf-8') return 'utf-8'
  return name
}

export function useXterm(options: UseXtermOptions = {}) {
  const containerRef: Ref<HTMLElement | null> = ref(null)

  let term: Terminal | null = null
  let webglAddon: WebglAddon | null = null
  let fitAddon: FitAddon | null = null
  let resizeObserver: ResizeObserver | null = null
  let visibilityObserver: IntersectionObserver | null = null
  let resizeTimer: ReturnType<typeof setTimeout> | null = null
  let contextLostHandled = false

  let decoder: TextDecoder | null = null
  let decoderEncoding = ''

  /** 当前编码（可经 setEncoding 切换） */
  let currentEncoding = normalizeEncoding(options.encoding ?? 'UTF-8')

  /** 切换会话编码：重置解码器，后续输出按新编码衔接 */
  function setEncoding(encoding: string): void {
    const next = normalizeEncoding(encoding)
    if (next === decoderEncoding) return
    currentEncoding = next
    decoder = null
    decoderEncoding = ''
  }

  /** 按当前编码把字节流解码为字符串（stream 模式处理跨批次多字节字符） */
  function decodeBytes(data: Uint8Array): string {
    if (!decoder || decoderEncoding !== currentEncoding) {
      decoderEncoding = currentEncoding
      try {
        decoder = new TextDecoder(currentEncoding)
      } catch {
        // 编码标签不受支持时回退 UTF-8
        decoder = new TextDecoder('utf-8')
        decoderEncoding = 'utf-8'
      }
    }
    return decoder.decode(data, { stream: true })
  }

  /** 运行时切换字号（上层改 fontSize prop 或用户设置时调用） */
  function setFontSize(size: number): void {
    if (!term || !Number.isFinite(size) || size <= 0) return
    term.options.fontSize = size
  }

  /**
   * 加载 WebGL renderer；失败（驱动不支持等）时静默回退 DOM renderer，
   * 不影响终端可用性，也不全局禁用 GPU
   */
  function loadWebgl(): void {
    if (!term || webglAddon) return
    try {
      const addon = new WebglAddon()
      addon.onContextLoss(() => {
        // xterm.js #4128：上下文丢失后销毁并重建 renderer
        try {
          addon.dispose()
        } catch {
          // 上下文已丢失时 dispose 可能抛错，忽略
        }
        webglAddon = null
        requestAnimationFrame(() => loadWebgl())
      })
      term.loadAddon(addon)
      webglAddon = addon
    } catch {
      webglAddon = null
    }
  }

  /** webglcontextlost 兜底监听：阻止默认行为并安排一次重建 */
  function handleContextLost(e: Event): void {
    e.preventDefault()
    if (contextLostHandled) return
    contextLostHandled = true
    if (webglAddon) {
      try {
        webglAddon.dispose()
      } catch {
        // 忽略
      }
      webglAddon = null
    }
    requestAnimationFrame(() => {
      contextLostHandled = false
      loadWebgl()
    })
  }

  /** fit 到容器尺寸并触发 onResize 回调（供上层调 sshResize） */
  function fit(): TerminalDimensions | null {
    if (!term || !fitAddon) return null
    try {
      fitAddon.fit()
      const dims = { cols: term.cols, rows: term.rows }
      options.onResize?.(dims)
      return dims
    } catch {
      // 容器尺寸为 0（如窗格尚未布局）时 fit 会抛错，忽略本次
      return null
    }
  }

  function scheduleFit(): void {
    if (resizeTimer !== null) clearTimeout(resizeTimer)
    resizeTimer = setTimeout(() => {
      resizeTimer = null
      fit()
    }, 50)
  }

  /** 写入输出：Rust 透传字节流（或已解码字符串），ANSI 由 xterm 解析 */
  function write(data: Uint8Array | string): void {
    if (!term) return
    if (typeof data === 'string') {
      term.write(data)
    } else {
      term.write(decodeBytes(data))
    }
  }

  /** 初始化终端（容器必须已挂载） */
  function open(): Terminal {
    if (term) return term
    const container = containerRef.value
    if (!container) throw new Error('终端容器未挂载，无法初始化 xterm')

    term = new Terminal({
      theme: { ...TERMINAL_DARK_THEME },
      fontFamily: '"Cascadia Mono", Consolas, "Microsoft YaHei", monospace',
      fontSize: options.fontSize ?? 14,
      cursorBlink: true,
      scrollback: 10000,
      allowProposedApi: true,
    })
    fitAddon = new FitAddon()
    term.loadAddon(fitAddon)
    term.open(container)

    // 启用 Unicode 11 提升中文等宽字符宽度判定
    try {
      term.unicode.activeVersion = '11'
    } catch {
      // 当前版本不支持时保持默认
    }

    // 键盘输入
    term.onData((data) => options.onData?.(data))

    // WebGL context lost：容器级兜底监听 + addon 级回调双保险
    container.addEventListener('webglcontextlost', handleContextLost)
    loadWebgl()

    // 容器尺寸变化 → 防抖 fit → onResize（按会话 ID 路由 resize 由上层处理）
    resizeObserver = new ResizeObserver(() => scheduleFit())
    resizeObserver.observe(container)

    // 窗格在隐藏（display:none）时挂载会让 fit() 抛错被吞、onResize 以 80×24 上报；
    // 用 IntersectionObserver 在可见性恢复时补一次 fit，保证首显后 PTY 尺寸正确。
    if (typeof IntersectionObserver !== 'undefined') {
      visibilityObserver = new IntersectionObserver((entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) scheduleFit()
        }
      })
      visibilityObserver.observe(container)
    }

    fit()
    // 默认不盲抢焦点（多窗格/后台标签会打断输入），由上层经 autoFocus 显式开启
    if (options.autoFocus) term.focus()
    return term
  }

  /** 全键盘可达：不拦截 Ctrl+T / Ctrl+Tab / Alt+数字 等全局快捷键，让其冒泡到应用层 */
  function attachGlobalKeyPassthrough(): void {
    term?.attachCustomKeyEventHandler((ev) => {
      if (ev.type !== 'keydown') return true
      if (ev.ctrlKey && ev.key === 't') return false
      if (ev.ctrlKey && ev.key === 'T') return false
      if (ev.ctrlKey && ev.key === 'Tab') return false
      if (ev.ctrlKey && ev.shiftKey && ev.key === 'Tab') return false
      if (ev.altKey && /^[1-9]$/.test(ev.key)) return false
      return true
    })
  }

  /** 释放终端与全部监听 */
  function dispose(): void {
    if (resizeTimer !== null) {
      clearTimeout(resizeTimer)
      resizeTimer = null
    }
    resizeObserver?.disconnect()
    resizeObserver = null
    visibilityObserver?.disconnect()
    visibilityObserver = null
    containerRef.value?.removeEventListener('webglcontextlost', handleContextLost)
    try {
      term?.dispose()
    } catch {
      // dispose 过程中 WebGL 上下文可能已丢失，忽略
    }
    term = null
    webglAddon = null
    fitAddon = null
    decoder = null
    decoderEncoding = ''
  }

  // 在组合式函数作用域结束时自动清理（兜底，组件内仍建议显式调用）
  onScopeDispose(dispose)

  return {
    /** 终端容器 ref，模板上绑定到根元素 */
    containerRef,
    /** 初始化终端；容器未挂载时抛错 */
    open,
    /** fit 到容器尺寸并触发 onResize */
    fit,
    /** 写入输出（字节流或字符串） */
    write,
    /** 切换会话编码 */
    setEncoding,
    /** 运行时切换字号（需外部随后调用 fit 重新布局） */
    setFontSize,
    /** 放行全局快捷键（Ctrl+T / Ctrl+Tab / Alt+1~9） */
    attachGlobalKeyPassthrough,
    /** 释放终端与全部监听 */
    dispose,
  }
}
