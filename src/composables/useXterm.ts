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
import { onScopeDispose, ref, watch, type Ref } from 'vue'
import { Terminal, type IDisposable, type IDecoration } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebglAddon } from '@xterm/addon-webgl'
import { openUrl } from '@tauri-apps/plugin-opener'
import '@xterm/xterm/css/xterm.css'
import { useSettingsStore } from '@/stores/settings'
import { useKeyMappingStore } from '@/stores/keyMapping'
import { useSshOptionsStore } from '@/stores/sshOptions'
import { useUiStore } from '@/stores/ui'

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
  /** 字号，默认取设置 store 的 terminal_font_size */
  fontSize?: number
  /** 字体家族，默认取设置 store 的 terminal_font_family */
  fontFamily?: string
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

  // 设置联动：读取设置 store 的终端项（新终端默认值 + 运行时实时生效）
  const settings = useSettingsStore()
  const keyMappingStore = useKeyMappingStore()
  const sshOpts = useSshOptionsStore()
  const ui = useUiStore()

  // 键位映射拦截器需要最新映射列表（启动即加载，幂等）
  void keyMappingStore.ensureLoaded()

  let term: Terminal | null = null
  let webglAddon: WebglAddon | null = null
  let fitAddon: FitAddon | null = null
  let resizeObserver: ResizeObserver | null = null
  let visibilityObserver: IntersectionObserver | null = null
  let resizeTimer: ReturnType<typeof setTimeout> | null = null
  let contextLostHandled = false

  let decoder: TextDecoder | null = null
  let decoderEncoding = ''

  /** 键位映射拦截器集合（单个 attachCustomKeyEventHandler 内依次执行） */
  let keyInterceptors: ((ev: KeyboardEvent) => boolean)[] = []

  /** 注册键位拦截器（keydown 命中返回 false 拦截 xterm 处理），返回注销函数 */
  function registerKeyInterceptor(fn: (ev: KeyboardEvent) => boolean): () => void {
    keyInterceptors.push(fn)
    return () => {
      keyInterceptors = keyInterceptors.filter((f) => f !== fn)
    }
  }

  /** 键位映射拦截：keydown 命中映射键位时执行动作并拦截 xterm 处理 */
  function keyMappingInterceptor(ev: KeyboardEvent): boolean {
    if (ev.type !== 'keydown') return true
    const combo = ui.shortcutOf(ev)
    const mapping = keyMappingStore.comboIndex.get(combo)
    if (!mapping) return true
    if (mapping.action_type === 'send_string') {
      // 发送字符串：走键盘输入同路（onData → sshWrite）
      options.onData?.(mapping.payload)
    } else if (mapping.action_type === 'menu_command') {
      // 菜单命令：经 ui store 分发到 WorkspaceView onMenuAction
      ui.requestMenuAction(mapping.payload)
    }
    return false
  }

  // ---------------- SSH 选项联动：响铃 / 关键词高亮 / 登录提示符自动响应 ----------------

  /** 关键词高亮 decoration 集合（重扫时统一释放） */
  let highlightDecorations: IDecoration[] = []
  /** 关键词重扫防抖定时器（输出停止后扫描一次） */
  let highlightTimer: ReturnType<typeof setTimeout> | null = null
  /** 单终端 decoration 总量上限（防御超大输出） */
  const HIGHLIGHT_DECORATION_CAP = 500

  /** 关键词重扫防抖（写入侧每次触发，静默 500ms 后扫描） */
  function scheduleKeywordScan(): void {
    if (highlightTimer !== null) clearTimeout(highlightTimer)
    highlightTimer = setTimeout(() => {
      highlightTimer = null
      scanKeywords()
    }, 500)
  }

  /** 扫描缓冲区中的高亮关键词并注册 decoration（已知限制：仅扫缓冲末尾 3000 行） */
  function scanKeywords(): void {
    if (!term) return
    for (const d of highlightDecorations) {
      try {
        d.dispose()
      } catch {
        // 已释放的 decoration 忽略
      }
    }
    highlightDecorations = []
    if (!sshOpts.highlightEnabled) return
    const rules = sshOpts.highlightRules.filter((r) => r.keyword)
    if (rules.length === 0) return
    const buffer = term.buffer.active
    const cursorAbs = buffer.cursorY + buffer.baseY
    const startLine = Math.max(0, buffer.length - 3000)
    for (let i = startLine; i < buffer.length && highlightDecorations.length < HIGHLIGHT_DECORATION_CAP; i++) {
      const line = buffer.getLine(i)
      if (!line) continue
      const text = line.translateToString(false)
      for (const rule of rules) {
        let idx = text.indexOf(rule.keyword)
        while (idx !== -1 && highlightDecorations.length < HIGHLIGHT_DECORATION_CAP) {
          const marker = term.registerMarker(i - cursorAbs)
          if (marker) {
            const d = term.registerDecoration({
              marker,
              x: idx,
              width: rule.keyword.length,
              height: 1,
              backgroundColor: rule.color,
              layer: 'bottom',
            })
            if (d) {
              highlightDecorations.push(d)
            }
          }
          idx = text.indexOf(rule.keyword, idx + rule.keyword.length)
        }
      }
    }
  }

  /** Web Audio 短促 beep（OscillatorNode，880Hz / 120ms） */
  function playBellSound(): void {
    try {
      const ctx = new AudioContext()
      const osc = ctx.createOscillator()
      const gain = ctx.createGain()
      osc.frequency.value = 880
      gain.gain.value = 0.08
      osc.connect(gain)
      gain.connect(ctx.destination)
      osc.start()
      osc.stop(ctx.currentTime + 0.12)
      osc.onended = () => void ctx.close()
    } catch {
      // Web Audio 不可用（无用户手势等）时静默
    }
  }

  /** 容器背景闪烁（视觉响铃，150ms 后恢复） */
  function flashScreen(container: HTMLElement): void {
    const prev = container.style.backgroundColor
    container.style.backgroundColor = 'rgba(255, 255, 255, 0.25)'
    setTimeout(() => {
      container.style.backgroundColor = prev
    }, 150)
  }

  /** 终端 BEL 响铃（按 sshopt_bell_style：sound → 提示音，visual → 背景闪烁） */
  function handleBell(container: HTMLElement): void {
    const style = sshOpts.bellStyle
    if (style === 'sound' || style === 'both') playBellSound()
    if (style === 'visual' || style === 'both') flashScreen(container)
  }

  /** 登录提示符滚动缓冲（保留最近 64 字符，跨批量匹配行尾提示） */
  let promptTailBuffer = ''
  /** 已自动响应次数（防死循环：达到上限后不再响应） */
  let promptResponses = 0
  /** 上次响应时间戳（2s 冷却，避免连续响应） */
  let lastPromptResponse = 0

  /** 登录提示符自动响应：行尾匹配 login:/username:/password: 时自动发送配置内容 */
  function checkLoginPrompt(decoded: string): void {
    if (!sshOpts.promptAutoRespond || !options.onData) return
    const now = Date.now()
    if (now - lastPromptResponse < 2000) return
    if (promptResponses >= sshOpts.promptMaxAttempts) return
    promptTailBuffer = (promptTailBuffer + decoded).slice(-64)
    if (/(?:login|username)\s*:\s*$/i.test(promptTailBuffer)) {
      lastPromptResponse = now
      promptResponses += 1
      options.onData(sshOpts.promptUsername + '\r')
    } else if (/password\s*:\s*$/i.test(promptTailBuffer)) {
      lastPromptResponse = now
      promptResponses += 1
      options.onData(sshOpts.promptPassword + '\r')
    }
  }

  /** 关键词高亮规则/开关变更时重扫（无终端时仅记录，新终端创建后生效） */
  watch(
    () => [sshOpts.highlightEnabled, sshOpts.highlightRules] as const,
    () => {
      if (!term) return
      scanKeywords()
    },
    { deep: true },
  )

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

  // 设置联动：设置项变更时实时应用到已打开的终端（xterm options 支持运行时修改），
  // 未调用（尚无终端）时仅记录，新终端创建时按当前设置生效
  watch(
    () =>
      [
        settings.terminalFontSize,
        settings.terminalFontFamily,
        settings.terminalScrollback,
        settings.terminalCursorBlink,
        settings.selectionWordSeparators,
      ] as const,
    ([fontSize, fontFamily, scrollback, cursorBlink, wordSeparator]) => {
      if (!term) return
      if (Number.isFinite(fontSize) && fontSize > 0) term.options.fontSize = fontSize
      if (fontFamily) term.options.fontFamily = fontFamily
      if (Number.isFinite(scrollback) && scrollback >= 0) term.options.scrollback = scrollback
      term.options.cursorBlink = !!cursorBlink
      if (wordSeparator) term.options.wordSeparator = wordSeparator
      // 字号/字体变化会改变行列数，需重新 fit（含按会话 ID 路由的 onResize）
      scheduleFit()
    },
  )

  // URL 超链接开关/前缀实时生效：重建 provider（无终端时仅记录，新终端创建时生效）
  watch(
    () => [settings.mouseUrlHyperlink, settings.mouseUrlPrefixes] as const,
    () => {
      if (!term) return
      disposeLinkProvider()
      setupLinkProvider()
    },
  )

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
    const decoded = typeof data === 'string' ? data : decodeBytes(data)
    term.write(decoded)
    // SSH 选项联动：关键词高亮重扫（防抖）+ 登录提示符自动响应
    scheduleKeywordScan()
    checkLoginPrompt(decoded)
  }

  /** 初始化终端（容器必须已挂载） */
  function open(): Terminal {
    if (term) return term
    const container = containerRef.value
    if (!container) throw new Error('终端容器未挂载，无法初始化 xterm')

    term = new Terminal({
      theme: { ...TERMINAL_DARK_THEME },
      fontFamily: options.fontFamily ?? settings.terminalFontFamily,
      fontSize: options.fontSize ?? settings.terminalFontSize,
      cursorBlink: settings.terminalCursorBlink,
      scrollback: settings.terminalScrollback,
      wordSeparator: settings.selectionWordSeparators,
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

    // BEL 响铃（SSH 选项「高级 → 响铃」：sound → 提示音，visual → 背景闪烁）
    term.onBell(() => handleBell(container))
    // 登录提示符自动响应计数重置（每次新建终端视为一次新会话）
    promptResponses = 0
    promptTailBuffer = ''

    // 选中文本自动复制（键盘和鼠标设置联动）
    term.onSelectionChange(() => handleSelectionChange())

    // 鼠标行为：中键粘贴 / Ctrl+左单击移动光标 / 右键粘贴 / Shift+双击选择
    container.addEventListener('mousedown', handleMouseDown)
    container.addEventListener('contextmenu', handleContextMenu)
    container.addEventListener('dblclick', handleDblClick)

    // URL 超链接（mouse_url_hyperlink 开启时创建 provider）+ 键位映射拦截器
    setupLinkProvider()
    keyInterceptors.push(keyMappingInterceptor)

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

  // ---------------- 鼠标 / 选择 / URL 链接（键盘和鼠标设置联动） ----------------

  /** URL 链接 provider 句柄（mouse_url_hyperlink 开启时创建） */
  let linkDisposable: IDisposable | null = null
  /** 最近一次 provideLinks 提供的链接范围（按绝对缓冲行号索引，x 为 1-based 列） */
  let linkRangesByLine = new Map<number, { start: number; end: number }[]>()

  /** 按设置的前缀列表构造 URL 匹配正则（前缀以 | 分隔） */
  function buildUrlPattern(prefixes: string): RegExp | null {
    const parts = prefixes
      .split('|')
      .map((p) => p.trim())
      .filter(Boolean)
    if (parts.length === 0) return null
    const escaped = parts.map((p) => p.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
    return new RegExp(`(?:${escaped.join('|')})[^\\s]+`, 'g')
  }

  /** 注册 URL 链接 provider：按前缀匹配行内 URL，Ctrl+单击打开（可配置） */
  function setupLinkProvider(): void {
    if (!term || linkDisposable || !settings.mouseUrlHyperlink) return
    const pattern = buildUrlPattern(settings.mouseUrlPrefixes)
    if (!pattern) return
    linkRangesByLine = new Map()
    linkDisposable = term.registerLinkProvider({
      provideLinks: (lineNumber, callback) => {
        if (!term) {
          callback(undefined)
          return
        }
        const line = term.buffer.active.getLine(lineNumber)
        if (!line) {
          callback(undefined)
          return
        }
        // translateToString(false)：索引与缓冲列对齐（不裁剪尾部空白）
        const text = line.translateToString(false)
        const ranges: { start: number; end: number }[] = []
        let m: RegExpExecArray | null
        const re = new RegExp(pattern.source, 'g')
        while ((m = re.exec(text)) !== null) {
          ranges.push({ start: m.index + 1, end: m.index + m[0].length })
          if (m.index === re.lastIndex) re.lastIndex += 1
        }
        if (ranges.length === 0) {
          callback(undefined)
          return
        }
        linkRangesByLine.set(lineNumber, ranges)
        callback(
          ranges.map((r) => ({
            range: {
              start: { x: r.start, y: lineNumber },
              end: { x: r.end, y: lineNumber },
            },
            text: text.slice(r.start - 1, r.end - 1),
            activate: (event: MouseEvent, linkText: string) => {
              // [Ctrl+单击]以打开超链接：勾选时要求按住 Ctrl
              if (settings.mouseCtrlClickOpenHyperlink && !event.ctrlKey) return
              void openUrl(linkText).catch(() => {
                // 打开失败静默（无效 URL 等）
              })
            },
          })),
        )
      },
    })
  }

  /** 注销 URL 链接 provider（设置关闭或 dispose 时调用） */
  function disposeLinkProvider(): void {
    try {
      linkDisposable?.dispose()
    } catch {
      // 忽略
    }
    linkDisposable = null
    linkRangesByLine = new Map()
  }

  /** 点击位置是否命中已提供的 URL 链接（命中时链接激活优先于移动光标） */
  function isLinkAt(bufferLine: number, col0: number): boolean {
    const ranges = linkRangesByLine.get(bufferLine)
    if (!ranges) return false
    const col1 = col0 + 1
    return ranges.some((r) => col1 >= r.start && col1 <= r.end)
  }

  /** 复制文本后处理：软标签 / 删尾部空白 / 仅非空白行 / 包含末尾换行 */
  function postProcessCopiedText(raw: string): string {
    let result = raw
    if (settings.selectionCopyNonblankOnly) {
      result = result
        .split('\n')
        .filter((line) => line.trim() !== '')
        .join('\n')
    }
    if (settings.selectionCopyTrimWhitespace) {
      result = result
        .split('\n')
        .map((line) => line.replace(/[ \t]+$/, ''))
        .join('\n')
    }
    if (settings.selectionSoftTabs) {
      const width = term?.options.tabStopWidth ?? 8
      result = result.replace(/\t/g, ' '.repeat(Math.max(1, width)))
    }
    if (settings.selectionCopyIncludeNewline && !result.endsWith('\n')) {
      result += '\n'
    }
    return result
  }

  /** 自动复制防抖定时器（拖拽选中文本期间连续触发，静默后复制最终选区） */
  let autoCopyTimer: ReturnType<typeof setTimeout> | null = null

  /** 选中文本自动复制到剪贴板（onSelectionChange 防抖） */
  function handleSelectionChange(): void {
    if (!settings.selectionAutoCopy || !term) return
    const selection = term.getSelection()
    if (!selection) return
    if (autoCopyTimer !== null) clearTimeout(autoCopyTimer)
    autoCopyTimer = setTimeout(() => {
      autoCopyTimer = null
      if (!term) return
      const processed = postProcessCopiedText(term.getSelection())
      if (processed) {
        void navigator.clipboard.writeText(processed).catch(() => {
          // 剪贴板写入失败静默
        })
      }
    }, 150)
  }

  /** 从剪贴板粘贴（term.paste 自动处理 bracketed paste 模式） */
  async function pasteFromClipboard(): Promise<void> {
    try {
      const text = await navigator.clipboard.readText()
      if (text) term?.paste(text)
    } catch {
      // 剪贴板读取失败静默
    }
  }

  /** 计算鼠标事件对应的缓冲 cell 位置（0-based col/row，视口相对） */
  function cellPositionOf(e: MouseEvent): { col: number; row: number } | null {
    if (!term || !term.element) return null
    const screen = term.element.querySelector('.xterm-screen') as HTMLElement | null
    if (!screen) return null
    const rect = screen.getBoundingClientRect()
    if (rect.width <= 0 || rect.height <= 0 || term.cols <= 0 || term.rows <= 0) return null
    const col = Math.floor(((e.clientX - rect.left) / rect.width) * term.cols)
    const row = Math.floor(((e.clientY - rect.top) / rect.height) * term.rows)
    if (col < 0 || col >= term.cols || row < 0 || row >= term.rows) return null
    return { col, row }
  }

  /** Ctrl+左单击移动终端光标：按点击位置与光标位置的行列差发送方向键序列 */
  function moveCursorTo(e: MouseEvent): void {
    if (!term) return
    const pos = cellPositionOf(e)
    if (!pos) return
    const buffer = term.buffer.active
    // 光标在屏幕上的行（baseY 与 viewportY 的差 = 向上滚动行数）
    const cursorRow = buffer.cursorY + (buffer.baseY - buffer.viewportY)
    if (cursorRow < 0 || cursorRow >= term.rows) return
    let seq = ''
    const dRow = pos.row - cursorRow
    if (dRow < 0) seq += '\x1b[A'.repeat(-dRow)
    else if (dRow > 0) seq += '\x1b[B'.repeat(dRow)
    const dCol = pos.col - buffer.cursorX
    if (dCol < 0) seq += '\x1b[D'.repeat(-dCol)
    else if (dCol > 0) seq += '\x1b[C'.repeat(dCol)
    if (seq) options.onData?.(seq)
  }

  /** 容器 mousedown：中键粘贴（按设置）+ Ctrl+左单击移动光标（按设置） */
  function handleMouseDown(e: MouseEvent): void {
    if (!term) return
    if (e.button === 1) {
      if (settings.mouseMiddleButton === 'paste') {
        e.preventDefault()
        void pasteFromClipboard()
      }
      return
    }
    if (e.button === 0 && e.ctrlKey && settings.mouseCtrlClickMoveCursor) {
      // 点击位置命中 URL 链接时跳过（链接激活优先，避免打开链接同时移动光标）
      const pos = cellPositionOf(e)
      if (pos && isLinkAt(term.buffer.active.viewportY + pos.row, pos.col)) return
      moveCursorTo(e)
    }
  }

  /** 容器右键：按设置为粘贴（无右键菜单实现，默认拦截默认行为） */
  function handleContextMenu(e: MouseEvent): void {
    if (settings.mouseRightButton === 'paste') {
      e.preventDefault()
      void pasteFromClipboard()
    }
  }

  /** Shift+双击按空格（而非分隔符）选择：覆盖 xterm 内部按分隔符的选词 */
  function handleDblClick(e: MouseEvent): void {
    if (!term || !settings.selectionShiftDoubleClick || !e.shiftKey) return
    const pos = cellPositionOf(e)
    if (!pos) return
    const bufferLine = term.buffer.active.viewportY + pos.row
    const line = term.buffer.active.getLine(bufferLine)
    if (!line) return
    const text = line.translateToString(false)
    // 以空格分隔 token：向两侧扩展到空格边界
    let start = pos.col
    let end = pos.col
    while (start > 0 && text[start - 1] && text[start - 1] !== ' ' && text[start - 1] !== '\t') {
      start -= 1
    }
    while (end < text.length && text[end] !== ' ' && text[end] !== '\t') {
      end += 1
    }
    if (end > start) term.select(start, bufferLine, end - start)
  }

  /** 全键盘可达：不拦截 Ctrl+T / Ctrl+Tab / Alt+数字 等全局快捷键，让其冒泡到应用层 */
  function attachGlobalKeyPassthrough(): void {
    term?.attachCustomKeyEventHandler((ev) => {
      // 键位映射拦截器优先（命中映射键位时拦截 xterm 处理）
      for (const interceptor of keyInterceptors) {
        if (!interceptor(ev)) return false
      }
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
    if (autoCopyTimer !== null) {
      clearTimeout(autoCopyTimer)
      autoCopyTimer = null
    }
    if (highlightTimer !== null) {
      clearTimeout(highlightTimer)
      highlightTimer = null
    }
    for (const d of highlightDecorations) {
      try {
        d.dispose()
      } catch {
        // dispose 过程中可能已释放，忽略
      }
    }
    highlightDecorations = []
    disposeLinkProvider()
    keyInterceptors = []
    containerRef.value?.removeEventListener('mousedown', handleMouseDown)
    containerRef.value?.removeEventListener('contextmenu', handleContextMenu)
    containerRef.value?.removeEventListener('dblclick', handleDblClick)
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
    /** 注册键位拦截器（keydown 命中返回 false 拦截 xterm 处理） */
    registerKeyInterceptor,
    /** 释放终端与全部监听 */
    dispose,
  }
}
