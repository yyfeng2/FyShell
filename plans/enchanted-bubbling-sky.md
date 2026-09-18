# 文件菜单新增"终端"入口（默认打开本地终端）

## Context

用户要求在菜单栏"文件"菜单中添加"终端"功能，点击后默认打开本地终端（本机 shell，Windows 走 ConPTY）。

探索确认：本地终端的**完整链路已存在**，本次仅为 UI 入口接线，无需新增业务代码：

- 后端服务：[local_shell.rs](src-tauri/src/services/local_shell.rs)（portable-pty 封装），5 个命令已在 [lib.rs](src-tauri/src/lib.rs) 注册
- 前端 API：[localShell.ts](src/api/localShell.ts) 命令封装，[terminal.ts](src/stores/terminal.ts) 已支持 `sessionType: 'local'`
- 打开函数：[WorkspaceView.vue:767](src/views/WorkspaceView.vue#L767) 的 `openLocalTerminal()` —— 直接创建"本地终端"Tab 并连接，失败时 toast + 自动回收 Tab
- 现有同类入口：树右键菜单（`menuNewLocal()`，[WorkspaceView.vue:889](src/views/WorkspaceView.vue#L889)）与新建连接对话框的"新建本地终端"

## 改动

### 1. [MenuBar.vue](src/components/common/MenuBar.vue) —— 添加菜单项

在"文件"菜单 items 中（`新建文件夹`之后、`退出`的分隔线之前）插入：

```ts
{ title: '终端', action: 'local-terminal' },
```

### 2. [WorkspaceView.vue](src/views/WorkspaceView.vue) —— onMenuAction 接线

在 `onMenuAction` 的 switch 中（`new-folder` case 之后）添加：

```ts
case 'local-terminal':
  openLocalTerminal()
  break
```

## 验证

1. `npm run build`（或 `npm run tauri dev`）确认前端编译通过——本次不改 Rust 代码，后端无需重编译
2. 启动应用，点击菜单栏"文件 → 终端"，应新建一个标题为"本地终端"的 Tab 并显示 PowerShell 提示符
3. 在该终端内输入命令（如 `dir`）、`exit` 退出，确认读写正常、Tab 关闭后无残留进程
4. 回归：确认"文件"菜单原有项（新建会话/新建文件夹/退出）及树右键"新建本地终端"不受影响
