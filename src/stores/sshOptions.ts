/**
 * SSH 专属选项 Pinia store（SSH 选项对话框）
 *
 * 职责：
 * - 持有全部 SSH 选项（身份验证/登录脚本/安全性/隧道/代理/保持活动/VT 模式/
 *   外观/跟踪/响铃/日志记录），启动时从后端 SQLite settings 表加载（settings_get_all）、
 *   变更时写回（settings_set，key-value 文本，重启后仍保留）
 * - 全局生效：对所有 SSH 连接生效，不做按会话覆盖（SecureCRT「会话选项」的全局版）
 * - 键名统一 sshopt_ 前缀，避免与全局设置（theme_mode、terminal_* 与 mouse_* 等）冲突
 */
import { defineStore } from 'pinia'
import { ref } from 'vue'
import { settingsGetAll, settingsSet } from '@/api/settings'

/** 突出显示规则（关键词 + 颜色） */
export interface HighlightRule {
  keyword: string
  color: string
}

/** SSH 选项 key（SQLite settings 表，sshopt_ 前缀 snake_case） */
export const SSH_OPT_KEYS = {
  defaultAuthType: 'sshopt_default_auth_type',
  defaultProfileId: 'sshopt_default_profile_id',
  promptAutoRespond: 'sshopt_prompt_auto_respond',
  promptUsername: 'sshopt_prompt_username',
  promptPassword: 'sshopt_prompt_password',
  promptMaxAttempts: 'sshopt_prompt_max_attempts',
  scriptEnabled: 'sshopt_script_enabled',
  scriptContent: 'sshopt_script_content',
  scriptDelay: 'sshopt_script_delay',
  hostkeyStrict: 'sshopt_hostkey_strict',
  compression: 'sshopt_compression',
  tunnelAutoStart: 'sshopt_tunnel_auto_start',
  tunnelListenHost: 'sshopt_tunnel_listen_host',
  proxyEnabled: 'sshopt_proxy_enabled',
  proxyType: 'sshopt_proxy_type',
  proxyHost: 'sshopt_proxy_host',
  proxyPort: 'sshopt_proxy_port',
  proxyUsername: 'sshopt_proxy_username',
  proxyPassword: 'sshopt_proxy_password',
  keepaliveEnabled: 'sshopt_keepalive_enabled',
  keepaliveInterval: 'sshopt_keepalive_interval',
  vtTermType: 'sshopt_vt_term_type',
  termWebgl: 'sshopt_term_webgl',
  defaultTabColor: 'sshopt_default_tab_color',
  highlightEnabled: 'sshopt_highlight_enabled',
  highlightRules: 'sshopt_highlight_rules',
  traceEnabled: 'sshopt_trace_enabled',
  bellStyle: 'sshopt_bell_style',
  logAutoStart: 'sshopt_log_auto_start',
} as const

/** 选项默认值（加载失败时同样生效） */
const DEFAULTS = {
  default_auth_type: 'password',
  default_profile_id: '',
  prompt_auto_respond: false,
  prompt_username: '',
  prompt_password: '',
  prompt_max_attempts: 3,
  script_enabled: false,
  script_content: '',
  script_delay: 200,
  hostkey_strict: false,
  compression: false,
  tunnel_auto_start: true,
  tunnel_listen_host: '127.0.0.1',
  proxy_enabled: false,
  proxy_type: 'socks5' as 'socks5' | 'http',
  proxy_host: '',
  proxy_port: 1080,
  proxy_username: '',
  proxy_password: '',
  keepalive_enabled: true,
  keepalive_interval: 30,
  vt_term_type: 'xterm-256color',
  term_webgl: true,
  default_tab_color: '',
  highlight_enabled: false,
  highlight_rules: [] as HighlightRule[],
  trace_enabled: false,
  bell_style: 'off' as 'off' | 'sound' | 'visual' | 'both',
  log_auto_start: false,
}

export const useSshOptionsStore = defineStore('sshOptions', () => {
  // ---------------- 选项状态 ----------------
  const defaultAuthType = ref(DEFAULTS.default_auth_type)
  const defaultProfileId = ref(DEFAULTS.default_profile_id)
  const promptAutoRespond = ref(DEFAULTS.prompt_auto_respond)
  const promptUsername = ref(DEFAULTS.prompt_username)
  const promptPassword = ref(DEFAULTS.prompt_password)
  const promptMaxAttempts = ref(DEFAULTS.prompt_max_attempts)
  const scriptEnabled = ref(DEFAULTS.script_enabled)
  const scriptContent = ref(DEFAULTS.script_content)
  const scriptDelay = ref(DEFAULTS.script_delay)
  const hostkeyStrict = ref(DEFAULTS.hostkey_strict)
  const compression = ref(DEFAULTS.compression)
  const tunnelAutoStart = ref(DEFAULTS.tunnel_auto_start)
  const tunnelListenHost = ref(DEFAULTS.tunnel_listen_host)
  const proxyEnabled = ref(DEFAULTS.proxy_enabled)
  const proxyType = ref<'socks5' | 'http'>(DEFAULTS.proxy_type)
  const proxyHost = ref(DEFAULTS.proxy_host)
  const proxyPort = ref(DEFAULTS.proxy_port)
  const proxyUsername = ref(DEFAULTS.proxy_username)
  const proxyPassword = ref(DEFAULTS.proxy_password)
  const keepaliveEnabled = ref(DEFAULTS.keepalive_enabled)
  const keepaliveInterval = ref(DEFAULTS.keepalive_interval)
  const vtTermType = ref(DEFAULTS.vt_term_type)
  const termWebgl = ref(DEFAULTS.term_webgl)
  const defaultTabColor = ref(DEFAULTS.default_tab_color)
  const highlightEnabled = ref(DEFAULTS.highlight_enabled)
  const highlightRules = ref<HighlightRule[]>(DEFAULTS.highlight_rules)
  const traceEnabled = ref(DEFAULTS.trace_enabled)
  const bellStyle = ref<'off' | 'sound' | 'visual' | 'both'>(DEFAULTS.bell_style)
  const logAutoStart = ref(DEFAULTS.log_auto_start)

  /** 是否已完成首次后端加载 */
  const loaded = ref(false)

  let loadPromise: Promise<void> | null = null

  /** 启动时从后端加载选项（幂等：仅在首次调用时发起请求） */
  function ensureLoaded(): Promise<void> {
    if (loadPromise) return loadPromise
    loadPromise = (async () => {
      try {
        const map = await settingsGetAll()
        const flag = (key: string): boolean => map[key] === 'true'
        const num = (key: string, fallback: number): number => {
          const v = Number(map[key])
          return Number.isFinite(v) && v > 0 ? v : fallback
        }
        if (map[SSH_OPT_KEYS.defaultAuthType]) defaultAuthType.value = map[SSH_OPT_KEYS.defaultAuthType]
        if (map[SSH_OPT_KEYS.defaultProfileId] !== undefined) {
          defaultProfileId.value = map[SSH_OPT_KEYS.defaultProfileId]
        }
        promptAutoRespond.value = flag(SSH_OPT_KEYS.promptAutoRespond)
        if (map[SSH_OPT_KEYS.promptUsername] !== undefined) promptUsername.value = map[SSH_OPT_KEYS.promptUsername]
        if (map[SSH_OPT_KEYS.promptPassword] !== undefined) promptPassword.value = map[SSH_OPT_KEYS.promptPassword]
        promptMaxAttempts.value = num(SSH_OPT_KEYS.promptMaxAttempts, DEFAULTS.prompt_max_attempts)
        scriptEnabled.value = flag(SSH_OPT_KEYS.scriptEnabled)
        if (map[SSH_OPT_KEYS.scriptContent] !== undefined) scriptContent.value = map[SSH_OPT_KEYS.scriptContent]
        scriptDelay.value = num(SSH_OPT_KEYS.scriptDelay, DEFAULTS.script_delay)
        hostkeyStrict.value = flag(SSH_OPT_KEYS.hostkeyStrict)
        compression.value = flag(SSH_OPT_KEYS.compression)
        tunnelAutoStart.value = flag(SSH_OPT_KEYS.tunnelAutoStart)
        if (map[SSH_OPT_KEYS.tunnelListenHost] !== undefined) tunnelListenHost.value = map[SSH_OPT_KEYS.tunnelListenHost]
        proxyEnabled.value = flag(SSH_OPT_KEYS.proxyEnabled)
        const pt = map[SSH_OPT_KEYS.proxyType]
        if (pt === 'socks5' || pt === 'http') {
          proxyType.value = pt
        }
        if (map[SSH_OPT_KEYS.proxyHost] !== undefined) proxyHost.value = map[SSH_OPT_KEYS.proxyHost]
        proxyPort.value = num(SSH_OPT_KEYS.proxyPort, DEFAULTS.proxy_port)
        if (map[SSH_OPT_KEYS.proxyUsername] !== undefined) proxyUsername.value = map[SSH_OPT_KEYS.proxyUsername]
        if (map[SSH_OPT_KEYS.proxyPassword] !== undefined) proxyPassword.value = map[SSH_OPT_KEYS.proxyPassword]
        keepaliveEnabled.value = flag(SSH_OPT_KEYS.keepaliveEnabled)
        keepaliveInterval.value = num(SSH_OPT_KEYS.keepaliveInterval, DEFAULTS.keepalive_interval)
        if (map[SSH_OPT_KEYS.vtTermType]) vtTermType.value = map[SSH_OPT_KEYS.vtTermType]
        if (map[SSH_OPT_KEYS.termWebgl] !== undefined) termWebgl.value = flag(SSH_OPT_KEYS.termWebgl)
        if (map[SSH_OPT_KEYS.defaultTabColor] !== undefined) defaultTabColor.value = map[SSH_OPT_KEYS.defaultTabColor]
        highlightEnabled.value = flag(SSH_OPT_KEYS.highlightEnabled)
        try {
          const rules = JSON.parse(map[SSH_OPT_KEYS.highlightRules] ?? '[]')
          if (Array.isArray(rules)) {
            highlightRules.value = rules.filter(
              (r) => r && typeof r.keyword === 'string' && typeof r.color === 'string',
            )
          }
        } catch {
          // 解析失败保持默认
        }
        traceEnabled.value = flag(SSH_OPT_KEYS.traceEnabled)
        const bell = map[SSH_OPT_KEYS.bellStyle]
        if (bell === 'off' || bell === 'sound' || bell === 'visual' || bell === 'both') {
          bellStyle.value = bell
        }
        logAutoStart.value = flag(SSH_OPT_KEYS.logAutoStart)
        loaded.value = true
      } catch {
        // 加载失败不阻塞 UI：保持默认值，后续变更时按 key 逐条落库
      }
    })()
    return loadPromise
  }

  // ---------------- 通用写回 ----------------

  /** 按 key 持久化到后端（失败静默：不阻塞 UI，下次修改会重试覆盖） */
  function persist(key: string, value: string): void {
    void settingsSet(key, value).catch(() => {
      /* 持久化失败不影响 UI */
    })
  }

  function setDefaultAuthType(v: string): void {
    defaultAuthType.value = v
    persist(SSH_OPT_KEYS.defaultAuthType, v)
  }
  function setDefaultProfileId(v: string | null): void {
    defaultProfileId.value = v ?? ''
    persist(SSH_OPT_KEYS.defaultProfileId, v ?? '')
  }
  function setPromptAutoRespond(v: boolean): void {
    promptAutoRespond.value = v
    persist(SSH_OPT_KEYS.promptAutoRespond, v ? 'true' : 'false')
  }
  function setPromptUsername(v: string): void {
    promptUsername.value = v
    persist(SSH_OPT_KEYS.promptUsername, v)
  }
  function setPromptPassword(v: string): void {
    promptPassword.value = v
    persist(SSH_OPT_KEYS.promptPassword, v)
  }
  function setPromptMaxAttempts(v: number): void {
    if (!Number.isFinite(v) || v <= 0) return
    const n = Math.round(v)
    promptMaxAttempts.value = n
    persist(SSH_OPT_KEYS.promptMaxAttempts, String(n))
  }
  function setScriptEnabled(v: boolean): void {
    scriptEnabled.value = v
    persist(SSH_OPT_KEYS.scriptEnabled, v ? 'true' : 'false')
  }
  function setScriptContent(v: string): void {
    scriptContent.value = v
    persist(SSH_OPT_KEYS.scriptContent, v)
  }
  function setScriptDelay(v: number): void {
    if (!Number.isFinite(v) || v <= 0) return
    const n = Math.round(v)
    scriptDelay.value = n
    persist(SSH_OPT_KEYS.scriptDelay, String(n))
  }
  function setHostkeyStrict(v: boolean): void {
    hostkeyStrict.value = v
    persist(SSH_OPT_KEYS.hostkeyStrict, v ? 'true' : 'false')
  }
  function setCompression(v: boolean): void {
    compression.value = v
    persist(SSH_OPT_KEYS.compression, v ? 'true' : 'false')
  }
  function setTunnelAutoStart(v: boolean): void {
    tunnelAutoStart.value = v
    persist(SSH_OPT_KEYS.tunnelAutoStart, v ? 'true' : 'false')
  }
  function setTunnelListenHost(v: string): void {
    tunnelListenHost.value = v
    persist(SSH_OPT_KEYS.tunnelListenHost, v)
  }
  function setProxyEnabled(v: boolean): void {
    proxyEnabled.value = v
    persist(SSH_OPT_KEYS.proxyEnabled, v ? 'true' : 'false')
  }
  function setProxyType(v: 'socks5' | 'http'): void {
    proxyType.value = v
    persist(SSH_OPT_KEYS.proxyType, v)
  }
  function setProxyHost(v: string): void {
    proxyHost.value = v
    persist(SSH_OPT_KEYS.proxyHost, v)
  }
  function setProxyPort(v: number): void {
    if (!Number.isFinite(v) || v <= 0) return
    const n = Math.round(v)
    proxyPort.value = n
    persist(SSH_OPT_KEYS.proxyPort, String(n))
  }
  function setProxyUsername(v: string): void {
    proxyUsername.value = v
    persist(SSH_OPT_KEYS.proxyUsername, v)
  }
  function setProxyPassword(v: string): void {
    proxyPassword.value = v
    persist(SSH_OPT_KEYS.proxyPassword, v)
  }
  function setKeepaliveEnabled(v: boolean): void {
    keepaliveEnabled.value = v
    persist(SSH_OPT_KEYS.keepaliveEnabled, v ? 'true' : 'false')
  }
  function setKeepaliveInterval(v: number): void {
    if (!Number.isFinite(v) || v <= 0) return
    const n = Math.round(v)
    keepaliveInterval.value = n
    persist(SSH_OPT_KEYS.keepaliveInterval, String(n))
  }
  function setVtTermType(v: string): void {
    vtTermType.value = v
    persist(SSH_OPT_KEYS.vtTermType, v)
  }
  function setTermWebgl(v: boolean): void {
    termWebgl.value = v
    persist(SSH_OPT_KEYS.termWebgl, v ? 'true' : 'false')
  }
  function setDefaultTabColor(v: string | null): void {
    defaultTabColor.value = v ?? ''
    persist(SSH_OPT_KEYS.defaultTabColor, v ?? '')
  }
  function setHighlightEnabled(v: boolean): void {
    highlightEnabled.value = v
    persist(SSH_OPT_KEYS.highlightEnabled, v ? 'true' : 'false')
  }
  function setHighlightRules(v: HighlightRule[]): void {
    highlightRules.value = v
    persist(SSH_OPT_KEYS.highlightRules, JSON.stringify(v))
  }
  function setTraceEnabled(v: boolean): void {
    traceEnabled.value = v
    persist(SSH_OPT_KEYS.traceEnabled, v ? 'true' : 'false')
  }
  function setBellStyle(v: 'off' | 'sound' | 'visual' | 'both'): void {
    bellStyle.value = v
    persist(SSH_OPT_KEYS.bellStyle, v)
  }
  function setLogAutoStart(v: boolean): void {
    logAutoStart.value = v
    persist(SSH_OPT_KEYS.logAutoStart, v ? 'true' : 'false')
  }

  return {
    // 状态
    defaultAuthType,
    defaultProfileId,
    promptAutoRespond,
    promptUsername,
    promptPassword,
    promptMaxAttempts,
    scriptEnabled,
    scriptContent,
    scriptDelay,
    hostkeyStrict,
    compression,
    tunnelAutoStart,
    tunnelListenHost,
    proxyEnabled,
    proxyType,
    proxyHost,
    proxyPort,
    proxyUsername,
    proxyPassword,
    keepaliveEnabled,
    keepaliveInterval,
    vtTermType,
    termWebgl,
    defaultTabColor,
    highlightEnabled,
    highlightRules,
    traceEnabled,
    bellStyle,
    logAutoStart,
    loaded,
    // 动作
    ensureLoaded,
    setDefaultAuthType,
    setDefaultProfileId,
    setPromptAutoRespond,
    setPromptUsername,
    setPromptPassword,
    setPromptMaxAttempts,
    setScriptEnabled,
    setScriptContent,
    setScriptDelay,
    setHostkeyStrict,
    setCompression,
    setTunnelAutoStart,
    setTunnelListenHost,
    setProxyEnabled,
    setProxyType,
    setProxyHost,
    setProxyPort,
    setProxyUsername,
    setProxyPassword,
    setKeepaliveEnabled,
    setKeepaliveInterval,
    setVtTermType,
    setTermWebgl,
    setDefaultTabColor,
    setHighlightEnabled,
    setHighlightRules,
    setTraceEnabled,
    setBellStyle,
    setLogAutoStart,
  }
})
