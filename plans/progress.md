# FyShell 项目进度

> 更新日期：2026-09-17
> 对照计划：[new-version-architecture.md](new-version-architecture.md)（P0~P3 分期）
> 状态依据：源码扫描 + 最近一次 `tauri dev` 启动成功（dev.log，仅 10 个 warning 无 error）

## 总览

| 分期 | 状态 | 说明 |
|---|---|---|
| P0 第一期（跑通全链路） | ✅ 完成 | 骨架/连接/终端/SFTP 全链路落地 |
| P1 第二期（对齐 Xshell） | ✅ 完成 | 监控/快捷命令/隧道/会话增强/MySQL 基础 |
| P2 第三期（对齐 Navicat） | 🔶 骨架已落地 | 四个子功能代码均在，深度待验证 |
| P3 第四期（差异化） | ❌ 未开始 | 国产数据库、结构同步等 |
| 前端重设计 | ✅ 已应用 | 经典浅灰 Xshell 风格，defaultTheme: 'light' |

## P0 — 已完成 ✅

1. **应用骨架**：主窗口 + 托盘（`tray.rs`，含关闭到托盘）+ single-instance + GlobalDialog；主题 dark/light 切换
2. **连接管理**：SessionTree（文件夹分组）、SessionForm、5 种认证方式（AuthType）、测试连接、`config_store.rs`（SQLite）
3. **SSH 终端**：TerminalPane + useXterm + `services/ssh.rs`、FlexTabs 多标签、SplitLayout 分屏、HostkeyDialog
4. **SFTP 双栏**：DualPane/FilePane、`sftp.rs` 13 个命令、传输队列视图

## P1 — 已完成 ✅

1. **服务器监控**：MonitorDrawer/MonitorMiniBar + `monitor.rs`（CPU/内存/网络/Docker）
2. **快捷命令**：QuickCommandTree + ComposePane + QuickCommandBar + `quick_command.rs`
3. **SSH 隧道**：TunnelView + `tunnel.rs`（本地/远程/SOCKS）
4. **会话增强**：AuthProfileForm、session_log.rs + LogViewer、拖出新窗口（useDragOutWindow）
5. **MySQL 基础**：MysqlConnectionForm + MysqlDataGrid + `mysql.rs`（8 命令）

## P2 — 骨架已落地 🔶

| 子功能 | 前端 | Rust | 备注 |
|---|---|---|---|
| 表设计器 | TableDesigner.vue | mysql_design.rs（2 命令） | DDL 预览深度待验证 |
| SQL 控制台 | mysqlConsole.ts + HistoryDrawer + ExplainPanel | mysql_console.rs（5 命令） | 补全/格式化依赖 CodeMirror，未装 |
| 数据编辑 | mysqlEdit.ts | mysql_edit.rs（4 命令） | 表单视图/批量编辑待验证 |
| 导入导出 | ImportExportDialog.vue | mysql_io.rs（2 命令） | CSV/Excel/JSON/XML/SQL 覆盖面待验证 |

## 遗留事项（按优先级）

1. **主密码未接线**：`config_store.rs` 的 `has/set/verify_master_password` 均被 dead_code 警告——后端已写、前端无入口（P0.2 收尾）
2. **CodeMirror 6 未安装**：计划选型含 CodeMirror 6，package.json 缺失；SQL 控制台补全/格式化被阻塞
3. **updater 未配置**：`tauri.conf.json` 中 `active: false`、pubkey/endpoints 为空
4. **tauri-specta 未引入**：目前手写 `api/types.ts` 同步 Rust models（见 docs/ipc-contracts.md）
5. **Rust warnings ×10**：unused import/mut、session_log is_enabled、tunnel rule 未读等
6. **项目不是 git 仓库**：无版本控制，建议 `git init` 并首次提交

## 文档索引

- [new-version-architecture.md](new-version-architecture.md) — 总架构与功能优先级（P0~P3）
- [spicy-giggling-thompson.md](spicy-giggling-thompson.md) — Xshell 经典浅灰前端重设计方案（52 项 UI 缺陷已修复）
- [ipc-contracts.md](../docs/ipc-contracts.md) — IPC 数据模型与命令契约
