/**
 * 配色方案数据（Xshell 风格颜色方案）
 *
 * 内置方案：FyShell 默认（现终端暗色主题）+ Xshell 常见 10 方案
 * （Espresso/Obsidian 取 iTerm2-Color-Schemes 精确值，其余按方案特征定义，
 * 可在对话框内编辑）。schemeToTheme 汇总为 xterm theme 全字段。
 */

/** 配色方案数据模型（xterm theme 全字段） */
export interface ColorScheme {
  name: string
  background: string
  foreground: string
  cursor: string
  cursorAccent: string
  selectionBackground: string
  black: string
  red: string
  green: string
  yellow: string
  blue: string
  magenta: string
  cyan: string
  white: string
  brightBlack: string
  brightRed: string
  brightGreen: string
  brightYellow: string
  brightBlue: string
  brightMagenta: string
  brightCyan: string
  brightWhite: string
}

/** 预览行文字项（Normal/Bold/Underline/Reversed + Cursor 光标块） */
export const PREVIEW_STYLES: { label: string; kind: 'normal' | 'bold' | 'underline' | 'reversed' }[] = [
  { label: 'Normal', kind: 'normal' },
  { label: 'Bold', kind: 'bold' },
  { label: 'Underline', kind: 'underline' },
  { label: 'Reversed', kind: 'reversed' },
]

/** 内置配色方案（FyShell 默认 = 现终端暗色主题，保持升级前外观不变） */
export const BUILTIN_SCHEMES: ColorScheme[] = [
  {
    name: 'FyShell 默认',
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
  },
  {
    name: 'Espresso',
    background: '#323232',
    foreground: '#ffffff',
    cursor: '#d6d6d6',
    cursorAccent: '#323232',
    selectionBackground: '#5b5b5b',
    black: '#353535',
    red: '#d25252',
    green: '#a5c261',
    yellow: '#ffc66d',
    blue: '#6c99bb',
    magenta: '#d197d9',
    cyan: '#bed6ff',
    white: '#eeeeec',
    brightBlack: '#606060',
    brightRed: '#f00c0c',
    brightGreen: '#c2e075',
    brightYellow: '#e1e48b',
    brightBlue: '#8ab7d9',
    brightMagenta: '#efb5f7',
    brightCyan: '#dcf4ff',
    brightWhite: '#ffffff',
  },
  {
    name: 'idleToes',
    background: '#323232',
    foreground: '#ffffff',
    cursor: '#d6d6d6',
    cursorAccent: '#323232',
    selectionBackground: '#5b5b5b',
    black: '#000000',
    red: '#f35b56',
    green: '#7fb238',
    yellow: '#ffc66d',
    blue: '#3b8eea',
    magenta: '#d197d9',
    cyan: '#74d2cd',
    white: '#eeeeec',
    brightBlack: '#666666',
    brightRed: '#ff7b76',
    brightGreen: '#c2e075',
    brightYellow: '#ffe08a',
    brightBlue: '#6cabea',
    brightMagenta: '#efb5f7',
    brightCyan: '#9adfd4',
    brightWhite: '#ffffff',
  },
  {
    name: 'IR_Black',
    background: '#000000',
    foreground: '#f1f1f1',
    cursor: '#d6d6d6',
    cursorAccent: '#000000',
    selectionBackground: '#4d4d4d',
    black: '#4c4c4c',
    red: '#cf6a4c',
    green: '#7fb238',
    yellow: '#c8a048',
    blue: '#6c99bb',
    magenta: '#d077ab',
    cyan: '#5d9c9b',
    white: '#bfbfb9',
    brightBlack: '#787878',
    brightRed: '#ff8177',
    brightGreen: '#a5d86f',
    brightYellow: '#ffe08a',
    brightBlue: '#8ab7d9',
    brightMagenta: '#ff9bc1',
    brightCyan: '#7fc4c2',
    brightWhite: '#ffffff',
  },
  {
    name: 'New Black',
    background: '#000000',
    foreground: '#c0c0c0',
    cursor: '#c0c0c0',
    cursorAccent: '#000000',
    selectionBackground: '#4d4d4d',
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
  },
  {
    name: 'New White',
    background: '#ffffff',
    foreground: '#1e1e1e',
    cursor: '#1e1e1e',
    cursorAccent: '#ffffff',
    selectionBackground: '#add6ff',
    black: '#333333',
    red: '#c80e0e',
    green: '#0aa370',
    yellow: '#c7a008',
    blue: '#1f6fc8',
    magenta: '#bb3fbc',
    cyan: '#0f9cc0',
    white: '#4d4d4d',
    brightBlack: '#828282',
    brightRed: '#ff3c3c',
    brightGreen: '#2ce0a0',
    brightYellow: '#e8c53c',
    brightBlue: '#3b8eea',
    brightMagenta: '#ff55ff',
    brightCyan: '#2cd9ff',
    brightWhite: '#595959',
  },
  {
    name: 'Obsidian',
    background: '#283033',
    foreground: '#cdcdcd',
    cursor: '#c0cad0',
    cursorAccent: '#283033',
    selectionBackground: '#3e4c4f',
    black: '#000000',
    red: '#b30d0e',
    green: '#00bb00',
    yellow: '#fecd22',
    blue: '#3a9bdb',
    magenta: '#bb00bb',
    cyan: '#00bbbb',
    white: '#bbbbbb',
    brightBlack: '#555555',
    brightRed: '#ff0003',
    brightGreen: '#93c863',
    brightYellow: '#fef874',
    brightBlue: '#a1d7ff',
    brightMagenta: '#ff55ff',
    brightCyan: '#55ffff',
    brightWhite: '#ffffff',
  },
  {
    name: 'Pastel on Black',
    background: '#000000',
    foreground: '#d9d9d9',
    cursor: '#ffffff',
    cursorAccent: '#000000',
    selectionBackground: '#4d4d4d',
    black: '#000000',
    red: '#ff6c60',
    green: '#a8ff60',
    yellow: '#ffffb6',
    blue: '#96cbfe',
    magenta: '#ff73fd',
    cyan: '#c6c5fe',
    white: '#eeeeec',
    brightBlack: '#7c7c7c',
    brightRed: '#ffb6b0',
    brightGreen: '#ceffac',
    brightYellow: '#ffffcc',
    brightBlue: '#b5dcff',
    brightMagenta: '#ff9cfe',
    brightCyan: '#dfdfd0',
    brightWhite: '#ffffff',
  },
  {
    name: 'Pastel on White',
    background: '#ffffff',
    foreground: '#4d4d4d',
    cursor: '#4d4d4d',
    cursorAccent: '#ffffff',
    selectionBackground: '#add6ff',
    black: '#4d4d4d',
    red: '#d16a5a',
    green: '#7fa85f',
    yellow: '#c7b347',
    blue: '#6c9bd9',
    magenta: '#d16ac0',
    cyan: '#6ab0b6',
    white: '#828282',
    brightBlack: '#828282',
    brightRed: '#ff7b76',
    brightGreen: '#8fd475',
    brightYellow: '#e0d48b',
    brightBlue: '#8ab7d9',
    brightMagenta: '#efb5f7',
    brightCyan: '#7fc4c2',
    brightWhite: '#333333',
  },
  {
    name: 'White on Black',
    background: '#000000',
    foreground: '#ffffff',
    cursor: '#ffffff',
    cursorAccent: '#000000',
    selectionBackground: '#4d4d4d',
    black: '#000000',
    red: '#cd0000',
    green: '#00cd00',
    yellow: '#cdcd00',
    blue: '#0000ee',
    magenta: '#cd00cd',
    cyan: '#00cdcd',
    white: '#e5e5e5',
    brightBlack: '#7f7f7f',
    brightRed: '#ff0000',
    brightGreen: '#00ff00',
    brightYellow: '#ffff00',
    brightBlue: '#5c5cff',
    brightMagenta: '#ff00ff',
    brightCyan: '#00ffff',
    brightWhite: '#ffffff',
  },
  {
    name: 'XTerm',
    background: '#000000',
    foreground: '#ffffff',
    cursor: '#ffffff',
    cursorAccent: '#000000',
    selectionBackground: '#4d4d4d',
    black: '#000000',
    red: '#cd0000',
    green: '#00cd00',
    yellow: '#cdcd00',
    blue: '#0000ee',
    magenta: '#cd00cd',
    cyan: '#00cdcd',
    white: '#e5e5e5',
    brightBlack: '#7f7f7f',
    brightRed: '#ff0000',
    brightGreen: '#00ff00',
    brightYellow: '#ffff00',
    brightBlue: '#5c5cff',
    brightMagenta: '#ff00ff',
    brightCyan: '#00ffff',
    brightWhite: '#ffffff',
  },
]

/** 16 色 ANSI 键（普通 + 亮色，预览/编辑器共用） */
export const ANSI_KEYS = [
  'black',
  'red',
  'green',
  'yellow',
  'blue',
  'magenta',
  'cyan',
  'white',
] as const

/** 汇总为 xterm theme 全字段（scheme 缺省时回退 FyShell 默认暗色主题；name 字段剥离不传入 xterm） */
export function schemeToTheme(scheme?: ColorScheme | null): Record<string, string> {
  const [def] = BUILTIN_SCHEMES
  const src: ColorScheme = scheme ?? def
  const { name, ...theme } = src
  return theme
}

/** 校验导入的方案对象：名称非空且 20 个色值字段均为 #hex 格式 */
export function parseScheme(raw: unknown): ColorScheme | null {
  if (!raw || typeof raw !== 'object') return null
  const obj = raw as Record<string, unknown>
  const name = typeof obj.name === 'string' ? obj.name.trim() : ''
  if (!name) return null
  const hex = (v: unknown): v is string => typeof v === 'string' && /^#[0-9a-fA-F]{6}$/.test(v)
  const keys = [
    'background',
    'foreground',
    'cursor',
    'cursorAccent',
    'selectionBackground',
    ...ANSI_KEYS.map((k) => `bright${k[0].toUpperCase()}${k.slice(1)}`),
    ...ANSI_KEYS,
  ]
  const out: Record<string, unknown> = { name }
  for (const k of keys) {
    if (!hex(obj[k])) return null
    out[k] = obj[k]
  }
  return out as unknown as ColorScheme
}
