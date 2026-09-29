# FyShell 源码定位地图（Source Map）

> **用途**：源码层面（第三层）定位手册——每个功能分支 → 涉及文件 → 关键符号（命令/struct/函数/组件/store）→ 数据流/事件。与已有文档分层：
> - 功能层：[feature-tree 记忆](.claude/projects/e--fyshell/memory/fyshell-feature-tree.md)（做什么）
> - IPC 命令层：[command-params 记忆](.claude/projects/e--fyshell/memory/fyshell-command-params.md) + [src/bindings.ts](../src/bindings.ts)（命令签名，tauri-specta 权威生成）
> - **本文件**：源码定位（在哪、怎么跑）
>
> 更新约定：跟随 [src-tauri/src/lib.rs](../src-tauri/src/lib.rs) 命令注册与模块增删走查；行号为近似锚点，重构后需人工复核。截至 2026-09-29，HEAD 含传输可靠性批次：35ed981（rz 上传卡住根因——ZDLE 转义集对齐 lrzsz + rzsz 可靠投递 + examples/rz_harness 回归工具）+ 97ae419（SFTP 传输 120s 超时兜底/队列轮询对齐/快捷 SFTP 绑定活动终端）。第 5/6 行的记忆链接指向本地记忆文件（不入库）。

---

## 0. 系统拓扑与运行骨架

### 0.1 分层架构

```
┌──────────────────────── 前端 src/（Vue 3 + Vuetify 3 + Pinia + CodeMirror + xterm.js）───────────────┐
│  main.ts → App.vue → views/WorkspaceView.vue（主工作台，唯一顶层视图）                                   │
│  ├─ api/*.ts       30 个文件：invoke 封装层（组件禁止直接 invoke，统一走这里）+ 事件订阅                 │
│  ├─ stores/*.ts    13 个 Pinia store：状态单一来源（session/terminal/transfer/mysql/redis/...）           │
│  ├─ composables/   useXterm（终端核心）/useTargetConnection（MySQL 目标连接）/useDragOutWindow            │
│  ├─ components/    common（全局件）/ ssh / sftp / mysql / redis（业务组件）                               │
│  ├─ views/         WorkspaceView + mysql/TableDesigner + transfer/TransferQueueView + tunnel/TunnelView  │
│  └─ plugins/       vuetify.ts（主题/样式）+ pinia.ts；styles/theme.css（设计体系全部取值）                │
└──────────────────────────── IPC（tauri invoke + Channel + Event emit）──────────────────────────────────┘
┌──────────────────────── 后端 src-tauri/src/（Rust，models/services/commands 三层）───────────────────┐
│  lib.rs run()：resolve_data_dir → 插件注册 → setup 建 AppState + 26 个模块级 init → 命令注册            │
│  ├─ commands/  28 个模块 = IPC 命令薄封装（参数校验 + 调 services，见 0.4）                              │
│  ├─ services/  31 个模块 = 业务逻辑 + 长连接会话（ssh/sftp/mysql/redis/隧道/传输/rzsz/vault...）          │
│  ├─ models/    18 个模块 = serde 模型（TS 同名同构，禁 camelCase，见 serde-naming 记忆）                 │
│  ├─ state.rs   AppState 全局状态（会话句柄/传输队列/挂起点）                                            │
│  ├─ tray.rs    系统托盘                                                                                 │
│  └─ error.rs   AppError 统一错误（{} 兼容 RPC 序列化）                                                  │
└──────────────────────────────── SQLite（fyshell.db 多 Connection）+ 数据目录 ──────────────────────────┘
```

### 0.2 启动链（lib.rs run()，行号锚点）

1. `main.rs` → `fyshell_lib::run()`（lib.rs:254）
2. debug 构建：`specta_builder()` 导出 TS 绑定至 `../src/bindings.ts`（lib.rs:50-251，`export_ipc_bindings_snapshot` 测试兜底）
3. `tauri::Builder::default()`：
   - `.plugin(single_instance)` **最先注册**，重复启动唤起已有窗口（lib.rs:261-267）
   - opener / updater / autostart 插件（lib.rs:269-271）
   - `invoke_handler(generate_handler![...])` 146 命令全量注册（lib.rs:272-449）
4. `.setup()`（lib.rs:450-495）：
   - `resolve_data_dir`：`FYSHELL_DATA_DIR` 环境变量 → exe 同级 `data_dir.conf` → 便携模式 `data/` → 系统应用数据目录（lib.rs:18-44）
   - `ConfigStore::open(&data_dir)` → `app.manage(AppState::new(config_store))`（lib.rs:456-457）
   - 24+ 个服务模块级 `init(&data_dir)`（lib.rs:460-478）：auth_profile / session_log / quick_command_store / tunnel / mysql / redis / mysql_console / mysql_backup / settings_store / sftp_store / key_mapping_store / vault
   - `tray::init` + `setup_close_to_tray` + 主题应用（lib.rs:481-492）

### 0.3 AppState 全局状态（state.rs:7-38）

| 字段 | 类型 | 用途 |
|---|---|---|
| `ssh_sessions` | Mutex<HashMap<id, SshSessionHandle>> | 已建 SSH 会话句柄（写通道/读循环管理） |
| `transfer_tasks` | Mutex<Vec<TransferTask>> | 传输队列快照（list 返回源） |
| `cancelled_transfers` | Mutex<HashSet<id>> | 取消标记，传输循环每批检查 |
| `pending_hostkey` | Mutex<HashMap<id, oneshot::Sender<bool>>> | HostKey 首次确认挂起点（ssh_connect 挂起 → ssh_hostkey_accept 放行） |
| `local_sessions` | Mutex<HashMap<key, LocalShellHandle>> | 本地终端（ConPTY），key=每标签连接路由键 |
| `telnet_sessions` | Mutex<HashMap<key, TelnetSessionHandle>> | Telnet 会话 |
| `serial_sessions` | Mutex<HashMap<key, SerialSessionHandle>> | 串口会话 |
| `config_store` | ConfigStore | 会话/文件夹 SQLite（自带 Mutex<Connection>） |
| `rzsz_sessions` | Mutex<HashMap<key, RzszEntry>> | 进行中 rz/sz 传输（读循环检测哨兵时写入，结束移除） |

### 0.4 命令注册拓扑（commands/ 28 模块 → 146 命令）

> 命令名=命令函数名。注册处：specta 收集 lib.rs:52-229 + runtime invoke lib.rs:272-449。TS 签名见 bindings.ts。

| 模块文件 | 命令数 | 命令 |
|---|---|---|
| [session.rs](../src-tauri/src/commands/session.rs) | 7 | session_list/save/delete/test/folder_save/session_reorder/session_clone |
| [ssh.rs](../src-tauri/src/commands/ssh.rs) | 6 | ssh_connect/disconnect/write/resize/hostkey_accept/rzsz_respond |
| [sftp.rs](../src-tauri/src/commands/sftp.rs) | 22 | sftp_list/mkdir/delete/rename/chmod/read_text/write_text/upload/download + transfer_list/cancel/clear + local_list/list_roots/pick_dialog/save_dialog/mkdir/rename/delete + sftp_favorite_list/add/remove |
| [local_shell.rs](../src-tauri/src/commands/local_shell.rs) | 5 | local_shell_connect/disconnect/write/resize/alive |
| [telnet.rs](../src-tauri/src/commands/telnet.rs) | 4 | telnet_connect/disconnect/write/alive |
| [serial.rs](../src-tauri/src/commands/serial.rs) | 5 | serial_list/connect/disconnect/write/alive |
| [tunnel.rs](../src-tauri/src/commands/tunnel.rs) | 6 | tunnel_list/save/delete/start/stop/start_all |
| [quick_command.rs](../src-tauri/src/commands/quick_command.rs) | 4 | qc_list/qc_save_command/qc_save_folder/qc_delete |
| [session_log.rs](../src-tauri/src/commands/session_log.rs) | 3 | session_log_toggle/list/read |
| [auth_profile.rs](../src-tauri/src/commands/auth_profile.rs) | 3 | auth_profile_list/save/delete |
| [mysql.rs](../src-tauri/src/commands/mysql.rs) | 9 | mysql_connect/disconnect/list_tables/query/execute/cli_exec/begin/commit/rollback |
| [mysql_design.rs](../src-tauri/src/commands/mysql_design.rs) | 2 | mysql_table_design_get/save |
| [mysql_console.rs](../src-tauri/src/commands/mysql_console.rs) | 8 | mysql_history_list/search/clear + mysql_explain + mysql_saved_query_list/save/rename/delete |
| [mysql_edit.rs](../src-tauri/src/commands/mysql_edit.rs) | 4 | mysql_edit_preview/update_rows/delete_row/insert_rows |
| [mysql_io.rs](../src-tauri/src/commands/mysql_io.rs) | 2 | mysql_export/import |
| [mysql_tools.rs](../src-tauri/src/commands/mysql_tools.rs) | 4 | mysql_data_transfer/data_generate/data_sync/structure_sync |
| [mysql_objects.rs](../src-tauri/src/commands/mysql_objects.rs) | 4 | mysql_object_list/ddl/save/drop |
| [mysql_db.rs](../src-tauri/src/commands/mysql_db.rs) | 9 | mysql_db_list/create/drop/switch/table_show_create/table_optimize/table_rename/db_edit/db_find |
| [mysql_user.rs](../src-tauri/src/commands/mysql_user.rs) | 4 | mysql_user_list/create/drop/grants |
| [mysql_backup.rs](../src-tauri/src/commands/mysql_backup.rs) | 6 | mysql_backup/restore + backup_profile_list/save/delete + backup_run_list |
| [master_password.rs](../src-tauri/src/commands/master_password.rs) | 3 | master_password_status/set/verify |
| [vault.rs](../src-tauri/src/commands/vault.rs) | 5 | vault_status/unlock/lock/encrypt/decrypt |
| [settings.rs](../src-tauri/src/commands/settings.rs) | 7 | settings_get_all/get/set + user_data_clear + sshopt_session_list/set/delete |
| [tray.rs](../src-tauri/src/commands/tray.rs) | 1 | tray_set_close_to_tray |
| [color_scheme.rs](../src-tauri/src/commands/color_scheme.rs) | 2 | scheme_read_file/write_file |
| [key_mapping.rs](../src-tauri/src/commands/key_mapping.rs) | 3 | key_mapping_list/save/delete |
| [debug.rs](../src-tauri/src/commands/debug.rs) | 1 | debug_log |

> 一切命令走 `services/` 实现；commands 层只做参数透传 + 错误包装。命令总数核对：7+6+22+5+4+5+6+4+3+3+9+2+8+4+2+4+4+9+4+6+3+5+7+1+2+3+1 = 146 ✓

### 0.5 services/ 模块清单（31）

- **长连接会话**：[ssh.rs](../src-tauri/src/services/ssh.rs)（会话调度/读循环/写队列/引脚）、[sftp.rs](../src-tauri/src/services/sftp.rs)、[local_shell.rs](../src-tauri/src/services/local_shell.rs)（ConPTY）、[telnet.rs](../src-tauri/src/services/telnet.rs)、[serial.rs](../src-tauri/src/services/serial.rs)、[tunnel.rs](../src-tauri/src/services/tunnel.rs)、[mysql.rs](../src-tauri/src/services/mysql.rs)（连接池+事务+结果）、[redis.rs](../src-tauri/src/services/redis.rs)
- **传输/协议**：[transfer.rs](../src-tauri/src/services/transfer.rs)（SFTP 传输队列任务循环+超时兜底）、[rzsz.rs](../src-tauri/src/services/rzsz.rs)（rz/sz 会话编排/方向识别/读循环截流）、[rzsz_protocol.rs](../src-tauri/src/services/rzsz_protocol.rs)（ZMODEM 协议自研实现、纯逻辑）
- **SQLite store**：config_store（会话/文件夹/meta）/ auth_profile / session_log / quick_command_store / tunnel / mysql_console（query_history+saved_queries）/ mysql_backup（backup_profiles+runs）/ settings_store / sftp_store / key_mapping_store / vault(crate + DEK 信封)
- **MySQL 业务**：mysql_design（表设计器 DDL）/ mysql_console（历史+EXPLAIN）/ mysql_edit（行编辑预览执行）/ mysql_io（CSV/JSON/SQL 导入导出）/ mysql_objects（视图/函数/过程/触发/事件）/ mysql_user / mysql_db / mysql_tools（传输/生成/同步/结构同步）/ mysql_backup
- **辅助**：[trace.rs](../src-tauri/src/services/trace.rs)、ssh_proxy（SOCKS5/HTTP 代理）、login_script、error.rs 等

### 0.6 models/ 模块清单（18）

session / transfer / mysql / redis / tunnel / quick_command / auth_profile / mysql_design / mysql_console / mysql_edit / mysql_io / mysql_user / mysql_objects / mysql_db / mysql_tools / mysql_backup / key_mapping / vault
> serde 序列化命名统一 `snake_case`（`AuthType` 等可辨识联合除外），见 [serde-naming 记忆](.claude/projects/e--fyshell/memory/serde-naming-convention.md)。

### 0.7 SQLite 表清单（fyshell.db，多 Connection 各自建表）

| 表 | 所属服务 | 建表处 |
|---|---|---|
| folders / sessions / meta | config_store | config_store.rs:45-64 |
| auth_profiles | auth_profile | auth_profile.rs:37-38 |
| quick_folders / quick_commands | quick_command_store | quick_command_store.rs:48-55 |
| tunnels | tunnel | tunnel.rs:81-83 |
| query_history / saved_queries | mysql_console | mysql_console.rs:51-61 |
| backup_profiles / backup_runs | mysql_backup | mysql_backup.rs:618-629 |
| settings | settings_store | settings_store.rs:49-51 |
| sftp_favorites | sftp_store | sftp_store.rs:48-50 |
| key_mappings | key_mapping_store | key_mapping_store.rs:48-50 |
| meta（DEK 信封）+ settings + sessions + auth_profiles（vault 专属派生表） | vault | vault.rs:63-64 / 654-656 / 736-737 |

### 0.8 事件流（后端 emit → 前端 listen）

| 事件 | 发送（后端） | 接收（前端） | 载荷 |
|---|---|---|---|
| `session-status` | [ssh.rs:196](../src-tauri/src/services/ssh.rs#L196) emit_status（连接/断开/异常） | [stores/terminal.ts:116](../src/stores/terminal.ts#L116) 应用级注册 + [api/ssh.ts:77](../src/api/ssh.ts#L77) 订阅入口 | `{id, status}`（connected/disconnected） |
| `rzsz-start` / `rzsz-progress` / `rzsz-end` | [rzsz.rs:258/381/299](../src-tauri/src/services/rzsz.rs#L258) | [api/ssh.ts:104-126](../src/api/ssh.ts#L104) + [RzszDialog.vue:107-122](../src/components/ssh/terminal/RzszDialog.vue#L107) | 方向请求/进度/结束（ok+message） |
| `transfer-status` | [transfer.rs:252](../src-tauri/src/services/transfer.rs#L252) | [stores/transfer.ts:205](../src/stores/transfer.ts#L205)（引用计数）+ [WorkspaceView.vue:3082](../src/views/WorkspaceView.vue#L3082) | `{task}` 完整任务快照 |
| `tray-settings-requested` | [tray.rs:44](../src-tauri/src/tray.rs#L44) 托盘菜单"设置" | [WorkspaceView.vue:3091](../src/views/WorkspaceView.vue#L3091) 打开设置对话框 | `()` |
| `tray-close-to-tray-changed` | [tray.rs:52](../src-tauri/src/tray.rs#L52) 托盘菜单切换 | [WorkspaceView.vue:3098](../src/views/WorkspaceView.vue#L3098) 更新 UI | `boolean` |

> Channel（输出流）不属于事件：ssh_connect / sftp_upload / sftp_download / local_shell_connect 用 tauri `Channel<Vec<u8>|TaskSnapshot>` 推送高频流，见 [api/channels.ts](../src/api/channels.ts)。

### 0.9 前端架构导读

- **入口链路**：[main.ts](../src/main.ts)（createApp + pinia + vuetify + errorHandler）→ [App.vue](../src/App.vue)（ensureLoaded 设置 + 全局右键菜单压制）→ [WorkspaceView.vue](../src/views/WorkspaceView.vue)（唯一工作台：导航树/标签页/状态栏/事件注册中枢）
- **api 封装层** 30 文件：每命令模块一个文件，`invoke` 签名是 bindings.ts 的子集，事件订阅函数（onXxx）集中在同类文件（如 [api/ssh.ts](../src/api/ssh.ts)）
- **store 体系** 13 个：session（会话树）/ terminal（终端 Tab+连接态）/ transfer / mysql / redis / tunnel / quickCommand / sshOptions / settings / keyMapping / colorScheme / log / ui（toast+对话框）
- **全局 UI**：GlobalDialog（统一对话框/toast/主题入口）、MenuBar/StatusBar/ToolBar/AddressBar/FlexTabs（common/）

---

<!-- ===== 以下各分支由分模块梳理填充 ===== -->

## 1. 应用骨架与框架层

> 详细：[docs/_draft/settings-frontend.md](_draft/settings-frontend.md) 第 4/6/8 节。

- **单实例**：`lib.rs:261-267` single_instance 插件最先注册，重复启动唤起主窗口。
- **系统托盘**：[tray.rs:18-108](../src-tauri/src/tray.rs#L18)（init 建图标+四菜单 / `on_menu_event`:39 分发——设置菜单 emit `tray-settings-requested`:44、托盘勾选 emit `tray-close-to-tray-changed`:47、「退出」app.exit:55；`setup_close_to_tray`:88-99 CloseRequested 拦截隐藏）；命令 `commands/tray.rs:9` tray_set_close_to_tray；接线 lib.rs:481/485；前端 stores/settings.ts setTrayCloseToTray:359 + WorkspaceView listen:3091/3098。
- **主题模式**：settings 表 `theme_mode` 单一来源 → [settings.ts applyThemeMode:327](../src/stores/settings.ts#L327) + matchMedia 跟随系统:311 → ui.theme(:123) → **GlobalDialog watch 唯一出口**（GlobalDialog.vue:79-91 → vuetifyTheme.global.name）；启动窗口主题应用 lib.rs:483-491。
- **全局 UI**：MenuBar（七菜单 MENUS:47-113，Alt 导航 openMenu:164）→ StatusBar（连接态绿/红/灰:44-60）→ ToolBar（字体/编码/配色随 SSH 连接显示:226-306）→ AddressBar（地址栏解析:78）→ FlexTabs（标签页组件）→ GlobalDialog（确认/toast/主题出口）→ EmptyState。均 in `src/components/common/`。
- **快捷键三处联动**：设置对话框捕获入口（SettingsDialog.vue startCapture:625-658，校验含 Ctrl/Alt + 查重）→ ① WorkspaceView `registerGlobalShortcuts:3021-3060`（按下注册`shortcutNewSession/CloseTab/NextTab`:3032-3036 + Alt 菜单字母:3039 + Alt+1-9:3051，watch 变更重注册:3069）/ ② useXterm `attachGlobalKeyPassthrough:846-893`（终端剪贴板键按设置值判定 Ctrl+C/X/V/A 与 Ctrl+Shift 并存）/ ③ MenuBar 动态标注（ACTION_SETTING_KEYS:133-150）。
- **键位映射**（key_mappings 表，与快捷键独立）：useXterm `keyMappingInterceptor:136-149`（send_string→sshWrite / menu_command→ui.requestMenuAction→WorkspaceView onMenuAction:2978-2983）注册进 keyInterceptors:563，先于剪贴板键:848-851；编辑入口 KeyMappingDialog startCapture:201-214；store comboIndex O(1):23-29。

## 2. 会话管理
`→ docs/_draft/session-mgmt.md`

## 3. 终端系统（SSH / rzsz / 本地 / Telnet / 串口）

> 详细清单：[docs/_draft/local-telnet-serial-tunnel.md](_draft/local-telnet-serial-tunnel.md)（本地/Telnet/串口/隧道）+ `docs/_draft/ssh-terminal.md`（SSH/rzsz，待填充）。
> 共性模式：本地/Telnet/串口三类无持久会话终端用**前端生成连接路由键**（connId）路由；输出走 `tauri::ipc::Channel<Vec<u8>>`（≤4KB 批量）；状态推 `session-status`（[ssh.rs:196](../src-tauri/src/services/ssh.rs#L196) emit_status）；读循环/写转发在**独立 std 线程**阻塞执行；三个句柄 map 存 AppState（[state.rs:23/26/29](../src-tauri/src/state.rs#L23)）。

### 3.1 本地终端（ConPTY / portable-pty）

| 命令 | commands | services | 机制 |
|---|---|---|---|
| local_shell_connect | [local_shell.rs:15](../src-tauri/src/commands/local_shell.rs#L15) | [connect:158](../src-tauri/src/services/local_shell.rs#L158) | openpty(80×24) → spawn PowerShell（[build_shell_command:147](../src-tauri/src/services/local_shell.rs#L147)）→ 读写线程 → 注册 |
| local_shell_disconnect | [:30](../src-tauri/src/commands/local_shell.rs#L30) | [disconnect:223](../src-tauri/src/services/local_shell.rs#L223) | remove 句柄 + kill 子进程（try_wait×5 双保险） |
| local_shell_write/resize/alive | [:38/:49/:61](../src-tauri/src/commands/local_shell.rs#L38) | write:251 / resize:260 / alive:269 | 经 `StreamWriteMsg{Data\|Resize}` 写转发线程 |

- 结构：`LocalShellHandle`（local_shell.rs:51-57，write_tx + Arc<Mutex<Child>>）、`StreamWriteMsg`（:41-46）；同键先 remove 旧会话重建（HMR 坑，:166-171）。
- 前端：`api/localShell.ts`（connect:18 / write:35 注意 number[] 归一化：41）；store `terminal.ts connectSession:314`（type='local' → localShellConnect）、`writeTerminal:255`；WorkspaceView `openLocalTerminal:2002-2028`（路由键 `local-<tabId>`）、`onBlankDblclick:2031`（双击空白开终端）。

### 3.2 Telnet（TCP + 最小 IAC 协商）

| 命令 | commands | services | 机制 |
|---|---|---|---|
| telnet_connect | [telnet.rs:15](../src-tauri/src/commands/telnet.rs#L15) | [connect:229](../src-tauri/src/services/telnet.rs#L229) | tokio connect(:248)→into_std；port 缺省 23 |
| telnet_disconnect/write/alive | [:37/:45/:52](../src-tauri/src/commands/telnet.rs#L37) | disconnect:277(shutdown Both) / write:289 / alive:301 | write 侧 0xFF 双写转义（:205-212） |

- 结构：`IacState`（telnet.rs:60-69）+ `iac_handle`（:72-123）：WILL/DO 一律回 DONT/WONT、SB 跳过、IAC IAC 还原字面 0xFF；写侧 0xFF 双写对称。
- 前端：`api/telnet.ts` telnetConnect:20；store connectSession:316；WorkspaceView `openByteStreamSession:2072-2107`（Telnet/串口持久会话入口，**RLOGIN 复用 Telnet 传输**只需端口 513，:2096-2097）。

### 3.3 串口（serialport）

| 命令 | commands | services | 机制 |
|---|---|---|---|
| serial_list | [serial.rs:14](../src-tauri/src/commands/serial.rs#L14) | [list:136](../src-tauri/src/services/serial.rs#L136) | `available_ports()` 枚举，port_type 描述（:149-156） |
| serial_connect | [:23](../src-tauri/src/commands/serial.rs#L23) | [connect:161](../src-tauri/src/services/serial.rs#L161) | 8N1 + 读超时 100ms；baud 缺省 115200 |
| serial_disconnect/write/alive | [:53/:61/:68](../src-tauri/src/commands/serial.rs#L53) | disconnect:213 / write:226 / alive:244 | 有界写通道 cap1024 + Arc<AtomicBool> alive |

- 机制：串口无 shutdown 语义，`read_loop`（serial.rs:68-116）以 100ms 读超时轮询检查 alive 实现断开；`write`（:226-241）try_send 满则丢批告警（防无界积压，与 rzsz 可靠投递语义区分）。
- 前端：`api/serial.ts` serialList:17 / serialConnect:29（baud null→115200）；SessionForm `loadSerialPorts:510-529` 仅 serial 会话调 serialList 填下拉；打开路径复用 `openByteStreamSession`。

## 3.4（待 SSH/rzsz）→ 见 `docs/_draft/ssh-terminal.md`

## 4. SFTP 文件传输 + 传输队列
`→ docs/_draft/sftp-transfer.md`（含本批未提交改动定位：transfer.rs 120s 超时 / TransferQueueView 轮询 / 快捷 SFTP 绑定活动终端）

## 5. 快捷命令
`→ docs/_draft/session-mgmt.md`

## 6. SSH 隧道

> 详细：[docs/_draft/local-telnet-serial-tunnel.md](_draft/local-telnet-serial-tunnel.md) 第 4 节。**与前端会话终端不同：不占 AppState**，用模块级注册表 `RUNTIMES`（[tunnel.rs:39](../src-tauri/src/services/tunnel.rs#L39)）+ 独立 SQLite `CONN`（:44，`tunnels` 表，init 于 lib.rs:463）。

| 命令 | commands | services | 机制 |
|---|---|---|---|
| tunnel_list | [tunnel.rs:14](../src-tauri/src/commands/tunnel.rs#L14) | [list_rules:133](../src-tauri/src/services/tunnel.rs#L133) | SQLite 查询，session_id 可过滤 |
| tunnel_save | [:21](../src-tauri/src/commands/tunnel.rs#L21) | [save_rule:161](../src-tauri/src/services/tunnel.rs#L161) | upsert，id 空后端生成 uuid |
| tunnel_delete | [:32](../src-tauri/src/commands/tunnel.rs#L32) | [delete_rule:198](../src-tauri/src/services/tunnel.rs#L198) | 先 stop 再删 |
| tunnel_start | [:40](../src-tauri/src/commands/tunnel.rs#L40) | [start_tunnel:249](../src-tauri/src/services/tunnel.rs#L249) | **Remote 未实现**（P1 占位 :251-255）→ bind + accept_loop |
| tunnel_stop | [:49](../src-tauri/src/commands/tunnel.rs#L49) | [stop:293](../src-tauri/src/services/tunnel.rs#L293) | watch 广播 true 全停 |
| tunnel_start_all | [:56](../src-tauri/src/commands/tunnel.rs#L56) | [start_all:312](../src-tauri/src/services/tunnel.rs#L312) | 返回 StartAllReport |

- 转发实现：`accept_loop:342` → `forward_one:382` → Local=[local_forward_connection:405](../src-tauri/src/services/tunnel.rs#L405)（`SshSessionHandle.open_direct_tcpip` + `copy_bidirectional`）/ Socks=[socks5_connection:433](../src-tauri/src/services/tunnel.rs#L433)（**本地最小 SOCKS5 服务端**，无认证仅 CONNECT，支持 IPv4/6/域名）。
- 辨析：`services/ssh_proxy.rs`（出站代理客户端，ProxyKind:12-18，供 SSH 选项-代理用）与隧道 SOCKS5 服务端**方向相反、无代码调用**。
- 事件 `tunnel-status`（[tunnel.rs:217-241](../src-tauri/src/services/tunnel.rs#L217) set_status）前端**无监听**——状态靠 store refresh 快照同步。
- 前端：`api/tunnel.ts`（6 封装）、`stores/tunnel.ts`（useTunnelStore:53 含 normalize/refresh:75/start:106）、[TunnelView.vue](../src/views/tunnel/TunnelView.vue)（startAll:225 / toggle:349 / submit:454 Socks 时 target 置空）。

## 7. MySQL 数据库

> 详细：[mysql-core.md](_draft/mysql-core.md)（连接/查询/事务/网格/设计器/控制台/编辑）+ [mysql-advanced.md](_draft/mysql-advanced.md)（导入导出/工具箱/对象/库级/用户/备份）。

### 7.1 连接与查询核心（services/mysql.rs，617 行）

- **三个模块级注册表**（mysql.rs:25-36）：`POOLS`(conn_id→Pool) / `TX_CONNS`(事务独占连接) / `CONFIGS`(原始配置存档，切库重建池用)；`init()` lib.rs:464。
- **连接**：`connect:118`（OptsBuilder→Pool min1/max10→真实连接探测→uuid conn_id→POOLS+CONFIGS）返回 conn_id；**切库** `mysql_db_switch`→`switch_database:148` 用 CONFIGS 重建池返回新 conn_id（前端 store.switchDb 代际校验替换）；断开 `disconnect:162`（先 ROLLBACK 事务连接再池 remove）。
- **查询三条路**（统一 take_conn：事务独占优先→池取，用完归还）：
  | 命令 | 实现 | 行为 |
  |---|---|---|
  | mysql_query | [mysql.rs:273](../src-tauri/src/services/mysql.rs#L273)/do_query:304 | COUNT(*) 算 total；无 LIMIT 自动包 `LIMIT ? OFFSET ?`；首页成功记 history |
  | mysql_execute | :384/do_execute:406 | affected_rows；危险 SQL 需 confirmed |
  | mysql_cli_exec | :417/do_cli_exec:439 | 命令列：不分页不包 COUNT，≤1000 行 |
  | mysql_list_tables | :181/do_list:198 | information_schema 单查询（db 参数限定库） |
  - 危险 SQL 检测：`is_dangerous_sql:524`（DROP/TRUNCATE/ALTER 前缀、无 WHERE 增删改）+ 前端 isDangerousSql 预检 + confirmed 管道。
- **事务**：begin:483（池取连接 START TRANSACTION → TX_CONNS 独占）→ 期间查询/编辑走 take_conn 复用它 → commit:501 / rollback:512（remove 独占→执行→conn 归还池；池 reset_connection 隐式回滚故必须独立持有）。
- **查询历史自动写**：query/execute/cli_exec 成功内联 `mysql_console::history_add`（mysql.rs:293/395/428）+ query_history SQLite（SQL 截断 10_000，mysql_console.rs:87）。

### 7.2 控制台 / 历史 / 设计器 / 编辑增强

- 控制台：mysql_console.rs（history_list:98 / search:114 / clear:132 / saved_query 四连:146-253 / explain:264——analyze 先校验首词 select，EXPLAIN FORMAT=TREE）；前端 HistoryDrawer.vue doSearch:219、MysqlCliConsole.vue runLine:144（exit/clear/help/use 内置）、SqlEditor.vue（CodeMirror6:88-101 表名补全 compartment）。
- 表设计器：mysql_design.rs（do_get_design:60 信息模式组装 / build_ddl:251 生成 CREATE 或逐条 ALTER / apply_change:230）；模型 MySqlDesignChange（is_new + 列/索引/外键 diff）；前端 TableDesigner.vue（loadDesign:475 / save:749 组装 change→真实 DDL 展示）。
- **编辑「预览→确认→执行」**：mysql_edit.rs——`mysql_edit_preview`(:72) 只算不写（COUNT 估算+danger，复用事务连接）；真写三命令：`mysql_update_rows`(:123 隐式事务整批包裹)、`mysql_delete_row`(:187 主键恒含 WHERE)、`mysql_insert_rows`(:235 逐行 `?` 占位符参数化)；前端 MysqlDataGrid.vue commitEdits:2390→confirmPreview:2457→mysqlEditPreview/mysqlUpdateRows 等。

### 7.3 高级运维工具（6 类）

| 类 | 命令 → service | 机制要点 |
|---|---|---|
| 导入导出 | mysql_export→[mysql_io.rs:59](../src-tauri/src/services/mysql_io.rs#L59)/import:300 | CSV（RFC4180 无损往返）/JSON/SQL 三格式；CSV 批量 INSERT、SQL 按分号拆（跳过字面量注释内分号）；SAVEPOINT 事务包裹 |
| 工具箱 4 项 | mysql_data_transfer→:266、data_generate→:446、data_sync→:667、structure_sync→:975 | 传输=SHOW CREATE+多值 INSERT 跨库；生成=StdRng 9 类规则每 100 行 INSERT；同步=主键比对 INSERT/DELETE/REPLACE 三元；结构同步=差异计划 CREATE/ALTER；execute=false 先比对返回计划 |
| 数据库对象 | object_list:95 / ddl:181 / save:257 / drop:353 | 视图/函数/过程/触发/事件 DDL；save 校验首词 CREATE+防注入；视图免 DROP 用 OR REPLACE |
| 库级管理 | mysql_db 9 命令（list:53/create:97/drop:130/switch:148/show_create:164/optimize:216/rename:231/edit:248/find:286） | db_find 全库表 LIKE 搜索字符串列，每表限 20 行 |
| 用户管理 | mysql_user 4 命令（list:43/create:73/drop:90/grants:99） | quote_account 防引号注入 |
| 备份还原 | mysql_backup:210 / restore:389 + profile/run SQLite:612 | 转储 DDL+可选数据+DROP；还原分号拆分三分支（纯 DML 段独立事务防 FK）；AutoRunPanel=档案+立即运行 |

### 7.4 右键菜单与组件入口

- **库叶子 12 项**：WorkspaceView.vue 模板 3351-3408（关闭/打开→编辑→新建/删除→新建查询→命令列→运行 SQL 文件→转储 SQL 两种→打印数据库 buildStructureReport:1746→逆向模型→查找→刷新；处理器 1470-1700）；连接节点菜单 3411-3447。
- **表右键 21 项**：MysqlDataGrid.vue 模板 597-675（设计/删除/清空/截断/复制表/导入导出向导/转储/打印/维护四态/逆向/复制名/重命名；处理器 1086-1290）。
- **工具箱 4 项入口**：ToolBar.vue 177-224 emit → WorkspaceView 事件:3184-3187 → 4 对话框挂载 3703-3718。
- **前端 API**：mysqlIo.ts/mysqlTools.ts/mysqlObjects.ts/mysqlUsers.ts/mysqlBackup.ts + **db_edit/db_find 在 [api/mysql.ts:75/:90](../src/api/mysql.ts#L75)**（注意不在 mysqlDb.ts）；store：stores/mysql.ts（连接代际守卫 bumpConnGen:52-77 / connect:215 / loadTables:412 / confirmExecute:510 / 事务:558）。
- 数据网格核心：MysqlDataGrid.vue（3139 行）runQuery:2585（resultToken 过期丢弃）/ resolvePk:1488 / 单元格编辑 pendingEdits:1395 / 行高列宽拖拽 / 排序筛选分页。

## 8. Redis
`→ 待批次 B`

## 9. 安全体系
`→ 待批次 B`

## 10. 设置与持久化

> 详细：[docs/_draft/settings-frontend.md](_draft/settings-frontend.md) 第 1/2/3 节。

- **settings 表**：类别 [settings_store.rs](../src-tauri/src/services/settings_store.rs)（CONN OnceLock:19，`key VALUE` 单表 init_schema:48-58）；命令 `commands/settings.rs:12-64`（settings_get_all:12 → get_all:61 / get:19→76 / set:26→88 ON CONFLICT 幂等）；会话级覆盖 `sshopt_session_{id}_*`（session_list:106 LIKE ESCAPE 防短 id 误命中、session_set:130、session_delete:135）；`user_data_clear:58` → clear_all_user_data 逐表 DELETE（**14 张表常量** USER_DATA_TABLES:149-164）。
- **设置键清单**：`stores/settings.ts SETTING_KEYS:26-59`（含 `shortcut_*`:51-57、`ui_font_size`:58）+ SHORTCUT_DEFAULTS:65-73；ensureLoaded 幂等全量加载:207-303；布局缩放 CSS zoom applyFontSize:376。
- **键位映射**：[key_mapping_store.rs:23-99](../src-tauri/src/services/key_mapping_store.rs#L23)（key_mappings 表 id/key_combo/action_type/payload，save ON CONFLICT upsert:80）；命令 `commands/key_mapping.rs:13-45`（save 白名单:20-35）；模型 models/key_mapping.rs ACTION_SEND_STRING/ACTION_MENU_COMMAND:12-13。
- **配色方案**：命令 [color_scheme.rs:8-19](../src-tauri/src/commands/color_scheme.rs#L8) 仅文件 IO（JSON 前端序列化）；数据 [colorSchemes.ts:44-353](../src/data/colorSchemes.ts#L44)（11 内置 + schemeToTheme + parseScheme）；持久化存 settings 表 `color_schemes`/`terminal_color_scheme`（stores/colorScheme.ts:13-14）；应用点 useXterm term.options.theme:369-372。
- 设置对话框 8 分区：SettingsDialog.vue SECTIONS:494-503，打开 ensureLoaded:544，清除查询历史:683/清除无效数据:699/清除全部数据:718。

## 11. 交付与更新

> **Rust 侧无 updater 命令**——`tauri-plugin-updater` 前端直调（插件注册 [lib.rs:270](../src-tauri/src/lib.rs#L270)，另有 opener:269 / autostart:271）。
> 详细：[docs/_draft/settings-frontend.md](_draft/settings-frontend.md) 第 9 节。

- `api/updater.ts`：check_update（异常包成结果）:18 / install_update:35（downloadAndInstall）。
- 入口 1 手动：MenuBar.vue:109 帮助→「检测更新」→ WorkspaceView runCheckUpdate:2987-3013（无更新/有更新确认下载安装/网络错误 toast）。
- 入口 2 启动自动：WorkspaceView:3113-3116 等待 ensureLoaded 后按 `autoUpdateCheck` 静默检测（开关 SettingsDialog.vue:433-448，默认关）。
- 签名/发布约定：见 [updater-config.md](updater-config.md) 与记忆 tauri-cdp-ui-verification（正式打包 --noerrdialogs）。

## 12. 前端设计体系

> 详细：[docs/_draft/settings-frontend.md](_draft/settings-frontend.md) 第 7 节 + [design-params 记忆](.claude/projects/e--fyshell/memory/fyshell-design-params.md)（全部实际取值）。设计体系代码集中在 [styles/theme.css](../src/styles/theme.css)（600 行）：

| 章节 | 行号 | 内容 |
|---|---|---|
| :root 令牌 | 13-51 | `--fy-font` 微软雅黑 / chrome 浅灰 / mono 等宽 / 终端深色 / design-token |
| 全局基础 | 53-72 | body 浅灰默认背景 + overflow:hidden |
| 深色系统偏好 | 75-81 | 挂载前背景/color-scheme |
| 紧凑密度 | 90-102 | v-list-item/v-field/v-btn 行高、表单元格 |
| 滚动条 | 104-153 | WebKit + Firefox，深浅两套 |
| **全局 12px 字号** | 155-383 | rem 14px 基准 / `.v-application`+`.v-overlay-container` 双前缀 12px / 字段 26px / `.fy-field-row` 标签左置 / 全 Vuetify 字号类 !important |
| 列表/子菜单紧凑 | 386-443 | v-list-item 28px、Lucide svg 1em |
| 右键/下拉菜单 | 446-483 | 24px/20px 行高 |
| 终端区 | 486-500 | `.fy-terminal` 满铺透明 |
| 深色主题 | 502-549 | `.v-application.v-theme--dark` 覆盖 chrome 令牌 + 滚动条 |
| 工具类回写 | 551-563 | 字体回 `--fy-font` |
| switch/checkbox/radio | 565-600 | primary 选中态（radio 橙 ring `--v-theme-warning`） |

- **Vuetify 配置**：[plugins/vuetify.ts](../src/plugins/vuetify.ts)：Lucide 图标集接入:17-22；light/dark 双色板:24-60（primary 统一 `#2E6FDB`）；defaults 全组件 compact:62-70。
- **Lucide 图标**：[plugins/icons/lucide.ts](../src/plugins/icons/lucide.ts)：mdiMap:170-345 全映射 + RadioOnDot:357 + aliases:378 + 兜底 CircleHelp:427。
- **i18n**：[i18n/zh-CN.ts](../src/i18n/zh-CN.ts)（单语言字典）；**配色数据** `data/colorSchemes.ts`（12 章/第 10 节）。
