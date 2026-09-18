# 打通 Telnet / RLOGIN / 串口：完整会话化

## Context

用户发现 SshOptionsDialog 的 TELNET / RLOGIN / 串口三个分组是死占位（点选后只显示"暂不支持该连接类型 / FyShell 当前仅支持 SSH 连接"），但**后端 Telnet 与串口实现是真实的**（`services/telnet.rs` IAC 状态机 + `services/serial.rs` serialport），仅 RLOGIN 无后端。用户已确认范围（AskUserQuestion）：

- **完整会话化**：telnet / rlogin / serial 成为可保存的会话（会话树、会话级设置入口、双击直连），与 MySQL 会话（session_type='mysql'）对称。目前仅有即时连接（ByteStreamForm）而无会话树持久化。
- **RLOGIN 复用 Telnet 兼容连接（推荐）**：Rust 侧**零改动**，前端入口标记 `session_type='rlogin'` 但传输走 `telnet_connect`（端口默认 513），面板注明兼容说明。

成果目标：三个分组全变为真实可编辑面板；会话树右键"设置"打开对应面板编辑会话并保存；树内双击/连接菜单直接路由到 byte-stream 会话。

验证基线固定在 `cargo check` + `npx vue-tsc --noEmit` 双绿（本机 `cargo test` 二进制会 0xc0000139 崩溃，不做）。

---

## 1. 后端：SessionConfig 扩展串口字段

### 1a. `src-tauri/src/models/session.rs`
`SessionConfig` 增加（serde default，与既有 `session_type`/`profile_id` 同模式）：
```rust
/// 串口会话：端口名（session_type=="serial" 时生效）
pub serial_port: Option<String>,
/// 串口会话：波特率（默认 115200）
pub baud_rate: Option<u32>,
```
更新 `session_type` 注释：`None/"ssh"`=SSH，`"mysql"`=数据库，`"telnet"/"rlogin"`=Telnet 兼容，`"serial"`=串口。

### 1b. `src-tauri/src/services/config_store.rs`
仿照既有 `session_type` 迁移（约 78 行）追加幂等 ALTER：
```sql
ALTER TABLE sessions ADD COLUMN serial_port TEXT;
ALTER TABLE sessions ADD COLUMN baud_rate INTEGER;
```
`save_session`（INSERT + ON CONFLICT UPDATE，约 197-223 行）与加载 SELECT（约 369 行）补两列读写。**加载时 baud_rate 缺省回退 115200**（`row.get::<_, Option<u32>>`，前端默认值处理亦可，二选一；建议后端保持 Option，前端默认 115200）。

> 说明：`baud_rate` 存 `INTEGER`（SQLite 无 u32），Rust 侧 `Option<u32>`。

---

## 2. 前端：类型镜像 + 终端路由

### 2a. `src/stores/session.ts`
前端 `SessionConfig` 接口补 `serial_port?: string|null`、`baud_rate?: number|null`（可使用 bindings 类型，若已迁移）。

### 2b. `src/stores/terminal.ts` —— **零改动**
路由映射策略：RLOGIN 会话在 WorkspaceView 转 `TerminalSessionRef` 时 `sessionType: 'telnet'`（端口 513），`sessionTypes` Map 无需新增键。串口沿用现有 `serial` 分支（serialPort/baudRate 已支持）。

---

## 3. 前端：SessionForm 五类会话

`src/components/ssh/session/SessionForm.vue`：
- `SESSION_KINDS` 扩为 5 项：`ssh` / `mysql` / `telnet` / `rlogin` / `serial`（telnet/rlogin 标签区分"Telnet"/"RLOGIN"）。
- 字段条件渲染：
  - **telnet/rlogin**：host + port（port 默认 23 / 513，复用现有 host/port 字段区）。
  - **serial**：隐藏 host/port，显示串口列表下拉（复用 `ByteStreamForm` 的 `serialList` 逻辑，api/serial.ts `serialList()`）+ 波特率输入（默认 115200）。
- `initForm`：编辑恢复按 `session_type`；映射 `serial_port`/`baud_rate`。
- `buildConfig`：组装 `session_type` + `serial_port`/`baud_rate`。
- port watch：`mysql→3306 / ssh→22 / telnet→23 / rlogin→513`，serial 不设端口。
- `runTest`：byte-stream 类型走 `telnetConnect`/`serialConnect` 建连即断（临时 connKey，仿 MySQL 测试模式）。

---

## 4. 前端：WorkspaceView 会话路由 + 树

`src/views/WorkspaceView.vue`：
- 本地 `FlatNode` 增加 `sessionType?: string | null`（walk 时从 `config.session_type` 填）。
- 树图标分支（现 1445-1447）扩展：
  `mdi-console`（ssh）/ `mdi-database`（mysql）/ `mdi-console-network`（telnet）/ `mdi-serial-port`（serial）/ `mdi-send`（rlogin，或统一 `mdi-console-network`）。串口/网络图标用 Material Design Icons 现有集内图标。
- **路由（onNodeClick 528 行 / menuConnect 563 行）**增加 byte-stream 分支：`session_type` 为 `telnet|rlogin|serial` 时调用新 `openByteStreamSession(node)` → 构造 `TerminalSessionRef`（`sessionType: type==='rlogin'?'telnet':type`，host/port 或 serialPort/baudRate 取自 `config`），connKey=node.id 走持久会话路径（双击第二次聚焦既有 tab，与 SSH/MySQL 会话一致）。
- 即时 `openByteStreamTerminal`（781-825）保持原样不动。
- **右键"设置"**（menuSessionSettings 656）已按 sessionId 打开 SshOptionsDialog 会话模式，无需改路由；SshOptionsDialog 内部按新的 session 感知逻辑渲染（见 §5）。

---

## 5. 前端：SshOptionsDialog 占位移除 + 真实面板

### 5a. `src/components/ssh/options/types.ts`
`SSH_OPTIONS_NAV` 中 telnet/rlogin/serial 三叶子去掉 `placeholder: true`；各叶子补 `requires?: 'telnet'|'rlogin'|'serial'` 标签（或按现有 leaf id 推断）。

### 5b. 新面板组件（`src/components/ssh/options/` 下）
- **TelnetPanel.vue / RloginPanel.vue**：编辑会话字段 port（Telnet 默认 23；RLOGIN 默认 513 + 说明"与 Telnet 协议兼容，复用 Telnet 实现"，host/port 字段区），保存走 `session_save` 并刷新会话树。
- **SerialPanel.vue**：串口列表下拉 + 波特率（默认 115200），保存同上。

### 5c. `src/components/ssh/options/SshOptionsDialog.vue`
- 会话模式（`sessionId` props）下：从 `session_list` 拉取该会话 `config.session_type`，据此过滤导航树到对应分组（telnet→Telnet 分组，serial→串口分组等），并把默认 `initialLeaf` 定位到对应类型面板。
- 连接分组导航：byte-stream 会话只显示 Telnet/RLOGIN/串口相关叶子 + 共享的「终端/外观/高级」叶子（复用现有 TerminalPanels/AppearancePanels/AdvancedPanels，它们本来就是通用 sshopt 面板，byte-stream 同样适用）。
- 移除过时文案"FyShell 当前仅支持 SSH 连接"的兜底块（保留为极简 fallback）。
- 全局模式（新建连接上下文）下三个分组同真渲染。
- 面板保存与 sshopt 机制的关系：telnet/serial 面板编辑的是**会话字段**（SessionConfig），不是 sshopt 会话级键——保存调 `session_save` + 树刷新，与现有 sshopt 会话覆盖（sshopt_session_*）互不冲突。

---

## 6. 不改动的部分

- `services/telnet.rs`、`services/serial.rs`（后端实现本就真实，无缺陷）。
- `ByteStreamForm` 即时连接流程（保留两条路径：即时 + 会话）。
- `stores/terminal.ts`、`stores/sshOptions.ts`、`stores/session.ts` 的既有机制。

---

## 7. 验证

1. `npx vue-tsc --noEmit` 绿。
2. `cargo check` 绿（后端子命令：telnet/serial 与会话 CRUD 不受影响，但仍编译验证）。
3. 手工端到端（dev 启动 `npm run tauri dev`）：
   - 新建会话选 Telnet → 填 host/port → 测试连接通过 → 保存 → 会话树出现 telnet 图标 → 双击直连打开终端（作为已有 tab 聚焦）。
   - 同流程验证 RLOGIN（默认端口 513，标注兼容 Telnet）。
   - 新建串口会话 → 串口列表下拉可选（本机无串口则测试按钮给友好提示）。
   - 会话树右键 byte-stream 会话"设置" → 打开对应类型面板（Telnet/RLOGIN/串口）→ 改 port 保存 → 会话字段更新生效。
   - 回归：MySQL 会话、SSH 会话、即时 ByteStream 连接不受影响。
4. 若个别图标不存在（`mdi-serial-port`），fallback 到 `mdi-usb-port` / `mdi-console-network`，以 vue-tsc 不报错、图标实际渲染为准。

---

## 遗留（本任务不涉及）
- updater endpoint 渠道决策（用户侧）。
- vault 单测待可用环境（Linux/mac/CI）。
