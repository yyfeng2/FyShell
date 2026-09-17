# FyShell 项目进度

> 更新日期：2026-09-17
> 对照计划：[new-version-architecture.md](new-version-architecture.md)（P0~P3 分期）
> 状态依据：`cargo check` 零 error 零 warning + `vue-tsc --noEmit` 全绿（2026-09-17 多智能体并行清理后）

## 总览

| 分期 | 状态 | 说明 |
|---|---|---|
| P0 第一期（跑通全链路） | ✅ 完成 | 骨架/连接/终端/SFTP 全链路落地 |
| P1 第二期（对齐 Xshell） | ✅ 完成 | 监控/快捷命令/隧道/会话增强/MySQL 基础 |
| P2 第三期（对齐 Navicat） | ✅ 完成 | 后端 + 前端双绿，契约治理完成 |
| P3 第四期（差异化） | ⛔ 剔除不做 | 用户决定，仅作历史记录 |
| 前端重设计 | ✅ 已应用 | 经典浅灰 Xshell 风格，defaultTheme: 'light' |
| 遗留清理 | ✅ 完成 | 主密码接线/CodeMirror 6/契约治理/警告清理/git 版本控制 |

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

## P2 — 已完成 ✅（2026-09-17 契约治理后双绿）

| 子功能 | 前端 | Rust | 备注 |
|---|---|---|---|
| 表设计器 | TableDesigner.vue | mysql_design.rs（2 命令） | DDL 生成 + 预览 |
| SQL 控制台 | SqlEditor.vue（CodeMirror 6）+ HistoryDrawer + ExplainPanel | mysql_console.rs（5 命令） | 高亮/补全/Ctrl+Enter 执行 |
| 数据编辑 | mysqlEdit.ts | mysql_edit.rs（4 命令） | 主键行编辑/显式 NULL/预览→确认→执行 |
| 导入导出 | ImportExportDialog.vue | mysql_io.rs（2 命令） | CSV/JSON/SQL 向导 |

## Navicat 对齐阶段 — 已完成 ✅

1. **数据库对象统一命令**：视图/函数/过程/触发器/事件（kind 驱动）
2. **用户管理**：列表/授权/创建/删除
3. **备份/还原 + 自动运行档案**：SQLite 持久化，mysqldump 风格；前端 MysqlDbWorkspace 工具条 + BackupPanel/AutoRunPanel

## HexHub 对齐定制 — 已完成 ✅（2026-09-17，5 智能体并行，排除外部搜索/代理 Chrome）

对照 [功能菜单清单.md](../功能菜单清单.md)（HexHub 逆向）的差距分析后实施：

1. **高功能设置对话框**：SettingsDialog.vue（外观/终端/SFTP/数据/安全/关于六分区），主题浅色/深色/跟随系统（matchMedia 实时切换）、终端字体/字号/缓冲/光标实时生效到已开终端、SFTP 默认下载目录、清除查询历史/无效数据、主密码入口；后端 settings_store.rs（SQLite settings 表）持久化
2. **标签页右键管理**：复制名称/固定/重命名/新窗口打开/关闭其他·全部·左右；固定标签受所有关闭路径保护（批量跳过/X 隐藏/中键/拖出屏蔽）
3. **数据库级管理**：数据库切换（重建连接池方案，规避 mysql_async COM_RESET_CONNECTION 问题）/新建/删除（强确认）/复制 Host；表操作菜单（复制表结构 SQL/清空/截断/优化/重命名）
4. **SFTP 增强 + 隧道**：chmod 九宫格权限对话框（八进制双向联动）、收藏路径 SQLite 持久化、终端定位到当前目录、隧道一键全启（StartAllReport 汇总）
5. **单元格套件 + SQL 编辑器**：复制为 SQL 变体（Where/Insert 单条批量/InsertOrUpdate/Update/Delete/表格文本）、填充（NULL/日期/UUID/自定义）、跳转行、排序、列锁定（sticky）、粘贴/新建并粘贴、克隆行/插入 N 行（前端构造 INSERT 走现有预览→确认→执行管道）；SQL 格式化/大小写转换/压缩/仅运行选中的（sql-formatter）

## 遗留事项（2026-09-17 多智能体清理后剩余）

1. **updater 未配置**：`tauri.conf.json` 中 `active: false`、pubkey/endpoints 为空——留到准备发布时做（需发布渠道决策）
2. **tauri-specta 未引入**：目前手写 `api/types.ts` 同步 Rust models（见 docs/ipc-contracts.md）

**已解决**（2026-09-17 多智能体并行）：主密码接线（master_password 三命令 + MasterPasswordDialog）、CodeMirror 6 安装并集成 SQL 控制台、4 个死模型删除 + 契约同步、Rust warnings 19→1（仅第三方 future-incompat）、git 版本控制建立（基线提交 + 本地身份 yyfeng14）。

## 文档索引

- [new-version-architecture.md](new-version-architecture.md) — 总架构与功能优先级（P0~P3）
- [spicy-giggling-thompson.md](spicy-giggling-thompson.md) — Xshell 经典浅灰前端重设计方案（52 项 UI 缺陷已修复）
- [ipc-contracts.md](../docs/ipc-contracts.md) — IPC 数据模型与命令契约
