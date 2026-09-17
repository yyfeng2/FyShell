/**
 * FyShell P0 中文文案字典
 *
 * 供其他模块按需 import，例如：
 *   import { t } from '@/i18n/zh-CN'
 *   t.buttons.connect
 *
 * P0 简化方案：单语言包对象 + 嵌套命名空间；后续如需多语言再升级为 vue-i18n。
 */

export const zhCN = {
  /** 应用名 */
  app: {
    name: 'FyShell',
  },
  menus: {
    file: '文件',
    edit: '编辑',
    view: '查看',
    tools: '工具',
    help: '帮助',
  },
  buttons: {
    confirm: '确认',
    cancel: '取消',
    save: '保存',
    delete: '删除',
    rename: '重命名',
    close: '关闭',
    refresh: '刷新',
    test: '测试连接',
    connect: '连接',
    disconnect: '断开连接',
    upload: '上传',
    download: '下载',
    cancelTask: '取消任务',
    clearFinished: '清除已完成',
    newFolder: '新建文件夹',
    newSession: '新建会话',
    browse: '浏览',
  },
  tabs: {
    terminal: '终端',
    sftp: '文件传输',
    closeTab: '关闭标签',
    closeOthers: '关闭其他标签',
  },
  tree: {
    sessions: '会话',
    searchPlaceholder: '搜索会话或文件夹…',
    empty: '暂无会话，右键新建',
    root: '根目录',
  },
  status: {
    connecting: '连接中…',
    connected: '已连接',
    disconnected: '未连接',
    hostkeyVerify: '主机密钥确认',
    queued: '排队中',
    running: '传输中',
    completed: '已完成',
    failed: '失败',
    cancelled: '已取消',
  },
  dialog: {
    newSession: '新建会话',
    editSession: '编辑会话',
    deleteConfirm: '删除确认',
    deleteMessage: '确定要删除所选项目吗？此操作不可恢复。',
    testSuccess: '连接成功',
    testFailed: '连接失败',
    hostkeyTitle: '确认主机密钥',
    hostkeyFingerprint: '主机指纹',
    hostkeyMessage: '首次连接该主机，请确认以下指纹是否可信。',
  },
  auth: {
    password: '密码认证',
    publicKey: '公钥认证',
    interactive: '交互式认证',
    noAuth: '不验证',
    jump: '跳板机',
    passwordLabel: '密码',
    privateKeyPath: '私钥路径',
    passphrase: '私钥口令（可选）',
    jumpSession: '跳板会话',
  },
  transfer: {
    queue: '传输队列',
    localPath: '本地路径',
    remotePath: '远程路径',
    progress: '进度',
    speed: '速度',
    emptyQueue: '暂无传输任务',
  },
  terminal: {
    encoding: '编码',
    reconnect: '重新连接',
    connected: '已连接',
    disconnected: '连接已断开',
  },
  common: {
    name: '名称',
    host: '主机',
    port: '端口',
    username: '用户名',
    folder: '文件夹',
    ok: '确定',
    cancel: '取消',
    yes: '是',
    no: '否',
    loading: '加载中…',
    empty: '暂无数据',
    search: '搜索',
  },
} as const

export default zhCN
