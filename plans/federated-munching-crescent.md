# 独立 SFTP 会话 + 工具栏传输按钮跟随可见性

## Context

当前 SFTP 完全依附于 SSH 终端会话：工具栏「传输/传文件」按钮常驻显示，SFTP 双栏 Tab 绑定「最近终端会话」（`lastTerminalSessionId`）。用户要求：

1. 「新建会话」中添加独立的 SFTP 会话类型
2. 工具栏「传输/传文件」按钮跟随 SFTP 可用性显示/隐藏（已确认方案：**有已连接会话时显示，全部断开隐藏**——与数据库工具 4 按钮的连接显示/断开隐藏模式一致）
3. 保持现有 SFTP+SSH 耦合功能不变

关键事实（探索结论）：

- 后端 `sftp_*` 命令按 id 从 `state.ssh_sessions` 注册表取 `SshSessionHandle` 开 SFTP subsystem channel；`ssh_connect`（[ssh.rs:494](src-tauri/src/services/ssh.rs#L494)）**不校验 session_type** —— SFTP 会话可直接复用 `ssh_connect` 建连，**无需新后端命令、无需重生成 bindings.ts**
- `SessionConfig.session_type` 已是自由字符串（'mysql'/'telnet'/'serial' 先例），加 'sftp' 类型零后端改动
- 可复用的现成模式：
  - ToolBar `dbTools` prop（连接显示/断开隐藏）与 `sshConnected` prop（右侧快捷切换跟随）
  - `sshQuickToggles` computed（[WorkspaceView.vue:1988](src/views/WorkspaceView.vue#L1988)，含 `local-` 前缀排除）
  - `terminalStore.closeBySessionId`（[terminal.ts:390](src/stores/terminal.ts#L390)，无终端 Tab 时直接断开）

## 改动

### 1. SessionForm.vue —— 新增 'sftp' 会话类型

[src/components/ssh/session/SessionForm.vue](src/components/ssh/session/SessionForm.vue)

- `SessionKind` 类型（line 318）加 `'sftp'`；`SESSION_KINDS`（line 355）加 `{ value: 'sftp', title: 'SFTP 文件传输' }`
- 字段可见性 v-if 加 `sessionKind === 'sftp'`（与 'ssh' 同款——SFTP 基于 SSH，字段一致）：
  - 用户名（line 86）
  - 认证方式下拉（line 98）
  - 认证配置文件（line 138，`sessionKind === 'ssh' && authType === 'publicKey'` 处）
  - 密码（line 161，`sessionKind === 'ssh' && (authType === 'password' || authType === 'interactive')` 处）
- `buildConfig`（line 572-582）：`profileActive` 的 `sessionKind.value === 'ssh'` 判断与 noAuth 分支的 `sessionKind.value !== 'ssh'` 改为 `!== 'ssh' && !== 'sftp'`（sftp 走 SSH 同款 auth_type 联合，不落 noAuth）
- `runTest`（line 679）：byte-stream 分支条件改为 `sessionKind.value !== 'ssh' && sessionKind.value !== 'sftp'`，让 'sftp' 落到末尾的 `sessionTest(buildConfig())`（SSH 测试路径，复用现有 `session_test` 命令）
- `mapSessionKind`（line 480）加 `t === 'sftp'`；端口默认 22 走现有 default 分支自动生效

### 2. terminal.ts —— 导出 connectSession

[src/stores/terminal.ts](src/stores/terminal.ts) return 块（line 436）加 `connectSession,`。

独立 SFTP 会话连接复用 SSH 建连链路：`connectSession` 中 sessionType 缺省 `'ssh'` → 萰到 `sshConnect(session.id, onOutput, key)`；输出无窗格写入器时被 `pushOutput` 安全丢弃；`sessionTypes` 记录后断开自动路由 `sshDisconnect`。后端按 session.id 加载持久化配置（session_type 'sftp' 不影响），连接注册进 `ssh_sessions` 注册表。

### 3. WorkspaceView.vue —— 树路由 + SFTP Tab 绑定 + 工具栏可见性

[src/views/WorkspaceView.vue](src/views/WorkspaceView.vue)

- `onNodeClick`（line ~777）：`stype === 'sftp'` 分发到新函数 `openSftpSession(target)`（放在 telnet/rlogin/serial 分支之前，default 落 openTerminal 不变）
- 新函数 `openSftpSession(node)`：
  - 创建 `WorkTab { id, type: 'sftp', sessionId: node.id（配置 ID）, connId: genTabId()（连接路由键）, title, color }`
  - `await terminalStore.connectSession({ id: node.id, name: node.name, color: tabColor }, connId)`
  - 失败：移除 Tab + toast（`connectSession` 的 catch 已把 sessionStatus 置 disconnected 并 rethrow）
- `closeTab`（line ~1905）：加 `if (closed?.type === 'sftp' && closed.connId)` → `void terminalStore.closeBySessionId(closed.connId)`（无终端 Tab 命中 → 直接 sshDisconnect 清理注册表）
- DualPane 绑定（line 2876-2881）改为：
  ```vue
  :session-id="tab.connId ?? lastTerminalSessionId"
  :session-name="tab.sessionId ? tab.title : lastTerminalSessionName"
  ```
  - 工具栏单例 Tab（无 sessionId/connId）回退 `lastTerminalSessionId` —— **现有耦合不变**
  - 独立 SFTP 会话 Tab 用自身 connId（`sftp_*` 命令按连接路由键查 `ssh_sessions`）
- `openSftpTab` 单例检查（line 1829）：`t.type === 'sftp'` 改为 `t.type === 'sftp' && !t.sessionId`（避免工具栏「传文件」命中独立会话 Tab）
- 新 computed `sftpTools`（仿 `sshQuickToggles`/`mysqlDbTools`）：
  ```ts
  /** SFTP 工具可见态：任一 SSH 终端已连接（耦合 SFTP 可用）或任一独立 SFTP 会话已连接 */
  const sftpTools = computed(() => {
    const anySshTerminal = tabs.value.some((t) => {
      if (t.type !== 'terminal' || !t.connId || !t.sessionId || t.sessionId.startsWith('local-')) return false
      const stype = findNode(nodes.value, t.sessionId)?.config?.session_type
      // null/undefined/'ssh' = SSH 会话（SFTP 能力）；数据库与 byte-stream 会话不含 SFTP
      return (stype == null || stype === 'ssh' || stype === 'sftp')
        && connStatus.value.get(t.connId) === 'connected'
    })
    const anySftpSession = tabs.value.some(
      (t) => t.type === 'sftp' && !!t.connId && connStatus.value.get(t.connId) === 'connected',
    )
    return anySshTerminal || anySftpSession
  })
  ```
- ToolBar 调用（line 2552 附近）加 `:sftp-tools="sftpTools"`

### 4. ToolBar.vue —— 传输/传文件按钮跟随可见性

[src/components/common/ToolBar.vue](src/components/common/ToolBar.vue)

- 新 prop `sftpTools?: boolean`（注释：有已连接的 SSH/SFTP 会话时 true）
- 「传输」「传文件」两按钮包 `<template v-if="props.sftpTools">`（同 `dbTools`/`sshConnected` 模式，组内竖分隔线保留在「搜索」侧）

### 5. 树图标

[WorkspaceView.vue](src/views/WorkspaceView.vue) `treeIcon` switch（line ~655）加 `case 'sftp': return 'mdi-folder-swap-outline'`（与工具栏「传文件」同图标）；颜色走 `treeIconColor` 默认 success/自定义色，无需额外分支（sftp 会话落 SSH 服务分区，非 db 会话自动）。

## 不改动（保持 SSH 耦合）

- 工具栏「传文件」单例 Tab 语义：仍绑定 `lastTerminalSessionId`（最近终端会话），随终端切换自动跟随
- DualPane/FilePane/transfer store 全部现有逻辑、`sftp_*` 后端命令、bindings.ts

## 验证

1. `cargo check`（后端无改动应保持绿）+ `vue-tsc`
2. 测试服务器（192.168.31.100）真实连接 e2e：
   - 新建会话表单出现「SFTP 文件传输」类型，字段（主机/端口/用户名/认证方式/密码）与 SSH 一致
   - 双击 SFTP 会话节点 → 打开独立 SFTP 双栏 Tab（标题=会话名），远程栏可浏览测试服务器文件
   - 拖拽传输一个文件 → 传输队列出现任务且成功
   - 工具栏可见性：全部断开 → 传输/传文件隐藏；SSH 终端连接 → 出现；独立 SFTP 会话连接（无 SSH 终端）→ 也出现
   - 现有耦合回归：SSH 终端连接状态下工具栏「传文件」打开的单例 Tab 仍跟随最近终端会话
3. CDP 驱动 UI 验证（tauri.conf.json 9222 调试端口，按既有排查方法）
