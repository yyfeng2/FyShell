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

## 第二三梯队扩展 — 已完成部分 ✅（2026-09-17）

1. **updater 配置**：密钥对（`src-tauri/keys/` gitignored）+ endpoint 占位符 + 检测更新菜单（帮助 → 检测更新）+ [docs/updater-config.md](../docs/updater-config.md)；**发布渠道决策未定**（endpoint 为占位符，正式发布时填）
2. **已保存查询**：`saved_queries` 表 4 命令（list/save/rename/delete）+ HistoryDrawer 双 tab + 表格保存入口
3. **原生插入 `mysql_insert_rows`**：事务包裹、参数化、表名列名反引号校验；预览确认后走原生命令保证原子性
4. **字节流终端**：本地终端（ConPTY）/ Telnet（IAC 状态机）/ 串口（serialport）三类型统一模式，14 命令，`stores/terminal.ts` 按 sessionTypes 路由；`writeToSession` 统一写入入口（快捷命令/批量发送/SFTP 终端定位）
5. **SSH 选项对话框**：SecureCRT 会话选项风格（SshOptionsDialog + 8 面板），trace/ssh_proxy/login_script 后端接线（代理连接/跟踪日志/登录脚本解析）

**⛔ 跳过**（2026-09-17 用户决定）：Docker 管理视图、Redis 全套、批量操作+SSH 导入导出/公钥/Ping（对应智能体死亡后未重派）

## tauri-specta 集成 — 已完成 ✅（2026-09-17）

- **依赖**：specta =2.0.0-rc.25 + specta-typescript 0.0.12 + tauri-specta =2.0.0-rc.25（rc 阶段 `=` 锁定）
- **后端**：120 命令全部加 `#[specta::specta]` + 43 处模型 `specta::Type` derive + AppError 手写 impl；lib.rs `specta_builder()`（collect_commands!）+ `export_ipc_bindings()`（debug 启动时导出）
- **前端**：`src/bindings.ts`（963 行，120 命令 + 50 模型）自动生成；types.ts 迁移为 re-export（19 模型 re-export + 2 改名别名；3 个事件 payload / SessionNode 家族 / MonitorSample 保留手写，原因见提交说明）
- **维护注意**：新增命令需在 lib.rs 的 `generate_handler!` 与 `collect_commands!` **两处**同时登记；P2 api/*.ts 内联模型可后续渐进迁移

## 遗留事项

1. **updater 发布渠道未定**：endpoint 为占位符——正式发布时确定渠道并填写
2. **P2 api/*.ts 内联模型未迁移 bindings**：可后续渐进迁移

**已解决**（2026-09-17）：主密码接线、CodeMirror 6 集成、契约治理、Rust warnings 清理、git 版本控制、tauri-specta 集成、updater 配置。

## 文档索引

- [new-version-architecture.md](new-version-architecture.md) — 总架构与功能优先级（P0~P3）
- [spicy-giggling-thompson.md](spicy-giggling-thompson.md) — Xshell 经典浅灰前端重设计方案（52 项 UI 缺陷已修复）
- [ipc-contracts.md](../docs/ipc-contracts.md) — IPC 数据模型与命令契约
- [updater-config.md](../docs/updater-config.md) — updater 配置与发布流程
