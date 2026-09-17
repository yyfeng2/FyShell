/**
 * SSH 选项对话框树结构（参考 SecureCRT「会话选项」布局）
 *
 * 左侧两级树：连接 / 终端 / 外观 / 高级 四个组标题 + 缩进叶子项；
 * placeholder = true 的叶子为占位页（FyShell 暂不支持该连接类型，仅保留分组对齐布局）。
 */

/** 组 key（顶级分类） */
export type SshOptionsGroup = 'connection' | 'terminal' | 'appearance' | 'advanced'

/** 叶子 key（具体设置页） */
export type SshOptionsLeaf =
  | 'auth'
  | 'login-prompt'
  | 'login-script'
  | 'ssh-security'
  | 'ssh-tunnel'
  | 'ssh-sftp'
  | 'telnet'
  | 'rlogin'
  | 'serial'
  | 'proxy'
  | 'keepalive'
  | 'keyboard'
  | 'vt-mode'
  | 'term-advanced'
  | 'window'
  | 'highlight'
  | 'trace'
  | 'bell'
  | 'logging'

/** 树节点（叶子项） */
export interface SshOptionsNavLeaf {
  key: SshOptionsLeaf
  title: string
  icon: string
  /** 占位页（FyShell 暂不支持，仅保留布局） */
  placeholder?: boolean
}

/** 树节点（组） */
export interface SshOptionsNavItem {
  key: SshOptionsGroup
  title: string
  children: SshOptionsNavLeaf[]
}

/** 左侧树定义（1:1 对齐 SecureCRT「会话选项」布局） */
export const SSH_OPTIONS_NAV: SshOptionsNavItem[] = [
  {
    key: 'connection',
    title: '连接',
    children: [
      { key: 'auth', title: '用户身份验证', icon: 'mdi-account-key-outline' },
      { key: 'login-prompt', title: '登录提示符', icon: 'mdi-form-textbox' },
      { key: 'login-script', title: '登录脚本', icon: 'mdi-script-text-outline' },
      { key: 'ssh-security', title: 'SSH：安全性', icon: 'mdi-shield-lock-outline' },
      { key: 'ssh-tunnel', title: 'SSH：隧道', icon: 'mdi-tune-vertical' },
      { key: 'ssh-sftp', title: 'SSH：SFTP', icon: 'mdi-swap-horizontal' },
      { key: 'telnet', title: 'TELNET', icon: 'mdi-lan-disconnect', placeholder: true },
      { key: 'rlogin', title: 'RLOGIN', icon: 'mdi-lan-disconnect', placeholder: true },
      { key: 'serial', title: '串口', icon: 'mdi-serial-port', placeholder: true },
      { key: 'proxy', title: '代理', icon: 'mdi-server-network' },
      { key: 'keepalive', title: '保持活动状态', icon: 'mdi-heart-pulse' },
    ],
  },
  {
    key: 'terminal',
    title: '终端',
    children: [
      { key: 'keyboard', title: '键盘', icon: 'mdi-keyboard-outline' },
      { key: 'vt-mode', title: 'VT 模式', icon: 'mdi-console' },
      { key: 'term-advanced', title: '高级', icon: 'mdi-tune' },
    ],
  },
  {
    key: 'appearance',
    title: '外观',
    children: [
      { key: 'window', title: '窗口', icon: 'mdi-window-restore' },
      { key: 'highlight', title: '突出显示', icon: 'mdi-marker' },
    ],
  },
  {
    key: 'advanced',
    title: '高级',
    children: [
      { key: 'trace', title: '跟踪', icon: 'mdi-routes' },
      { key: 'bell', title: '响铃', icon: 'mdi-bell-outline' },
      { key: 'logging', title: '日志记录', icon: 'mdi-text-box-outline' },
    ],
  },
]
