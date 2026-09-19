# 修复 SSH 终端 sz/rz 无法执行（ZMODEM 支持）

## Context

用户在 SSH 终端执行 `sz file` / `rz` 时传输无法完成。根因：sz/rz 触发的是 **ZMODEM 协议**——远端返回二进制帧流（`**\x18B...` ZRQINIT 等），而 [services/ssh.rs](src-tauri/src/services/ssh.rs) 的读循环把所有输出原样透传给 xterm.js，全链路（后端读循环 + 前端终端）没有任何 ZMODEM 拦截，终端只显示二进制乱码。架构文档中 ZModem 标记为「未排期」，现按用户要求落地修复。

**方案**：采用 crates.io 的 `zmodem2` crate（v0.7.2，2026-08 更新）。它是**调用方驱动的状态机**（`Sender`/`Receiver` + `poll()` 返回 `Action`），调用方拥有全部 I/O——`Action::WriteWire` 经既有写转发路径回传、`submit_wire` 喂入读循环输出、文件读写直接 std::fs。纯同步、无 async 运行时依赖（仅 bitflags/hex/thiserror），完美契合现有「读循环 + 写转发」架构，无需管道桥接。

流程（两个方向都以同一哨兵开始，方向由用户在对话框选择，无启发式猜测）：

```
用户敲 rz/sz → 远端发 ZRQINIT → read_loop 检测哨兵 '**\x18B'
  → 哨兵前内容仍显示终端，哨兵起数据改道管道
  → emit zmodem-start → 前端弹对话框（接收文件 / 发送文件 / 取消）
  → 接收：选目录 → zmodem_respond("recv", dir) → Receiver 状态机收文件落盘
    发送：选文件 → zmodem_respond("send", path) → Sender 状态机读文件上传
    取消：zmodem_respond("cancel") → 发 CAN×8 中止远端
  → 期间 zmodem-progress 推进度；结束 emit zmodem-end，读循环恢复常规转发
```

## 后端

### 1. [Cargo.toml](src-tauri/Cargo.toml)

加 `zmodem2 = "0.7"`。注意该 crate 为 edition 2024（rustc ≥1.85），实施第一步先 `rustc --version` 验证。

### 2. 新建 `src-tauri/src/services/zmodem.rs`

- **`ZmodemChoice`** 枚举（`Serialize + specta::Type`，TS 侧 bindings 自动生成）：
  `Recv { dir: String }` / `Send { path: String }` / `Cancel`。
- **`ZmodemEntry`**（存 AppState）：`pipe_tx: std::sync::mpsc::Sender<Vec<u8>>`（读循环→任务管道）+ `respond_tx: std::sync::mpsc::Sender<ZmodemChoice>`（前端选择回传）。
- **`start(app, state, key, initial, write_tx)`**：read_loop 检测到哨兵时调用。创建管道 + 响应通道、把 `initial`（哨兵起的字节，含 ZRQINIT 帧）送入管道、注册条目、`tauri::async_runtime::spawn_blocking` 跑任务、emit `zmodem-start { key }`。
- **任务主循环**（全同步：`recv_timeout` 管道等待 + std::fs + crate 状态机；tokio UnboundedSender::send 本身同步）：
  1. 等前端选择（60s 无响应 → Cancel 兜底）；
  2. **接收（对端 sz）**：`Receiver::with_flow_control(0, true)`（文档明确推荐 TCP/SSH 可靠流用此配置，连续流式传输），`Action::WriteWire` → write_tx；`Event::FileStarted(FileInfo)` → emit 进度事件（含文件名/总大小）；`Action::WriteFile` → 落盘 `dir/<文件名>` → `file_written(n)`；`Event::SessionCompleted` → 结束。
  3. **发送（对端 rz）**：`Sender::new()` + `set_streaming_window(usize::MAX)`（可靠传输免每包 ACK 等待），读本地文件名/大小 → `start_file(FileInfo)`；`Action::ReadFile { offset, max_len }` → 读文件分片 → `submit_file`；`Event::FileCompleted`/`SessionCompleted` → 结束。
  4. 管道 `recv_timeout(1s)` 超时 → 调 crate 的 `timeout()`（内置重试）；连续 ~30 次无任何数据 → 判定链路失效，中止（防会话已断后任务悬挂）。
  5. **Cancel**：经 write_tx 发 CAN×8（`\x18`×8）中止远端。
  6. 结束：移除 AppState 条目（读循环据此恢复常规转发）+ emit `zmodem-end { key, ok, message }`。
- 进度事件 `zmodem-progress { key, file_name, transferred, total }` 按 100ms 节流。

### 3. [services/ssh.rs](src-tauri/src/services/ssh.rs) read_loop 改造

- `read_loop` 签名加 `app: AppHandle` + `key: String`（现有 `session_id` 仍作日志键）；`connect()` 中调整顺序：先建写通道并 spawn write_forward，再 spawn read_loop（传 `write_tx` 克隆，zmodem 启动用）。
- 每轮迭代检查 `AppState::zmodem_sessions`：条目存在 → 输出整块 `pipe_tx.send(...)`（不写 xterm、不写会话日志）；`pipe_tx.send` 失败（任务已退出）→ 恢复常规转发。
- **哨兵检测**（常规冲刷前）：扫描缓冲区 `*\x2A \x2A \x18 \x42`（ZPAD ZPAD ZDLE 'B'）；缓冲区尾部是部分哨兵前缀（`*`/`**`/`**\x18`）时先扣留等下一批拼齐（防跨批漏检）。
- 命中位置 `pos`：`buf[..pos]`（回显等）照常 → xterm；`buf[pos..]` → `zmodem::start(..., initial, ...)`，清空缓冲。

### 4. [state.rs](src-tauri/src/state.rs)

加字段 `zmodem_sessions: Mutex<HashMap<String, ZmodemEntry>>`（key = 连接路由键），`new()` 初始化。

### 5. 命令层 + 注册

- [commands/ssh.rs](src-tauri/src/commands/ssh.rs)：加 `zmodem_respond(key: String, action: String, local_path: String)`——查条目、`respond_tx.send(choice)`；条目不存在返回明确错误。
- [lib.rs](src-tauri/src/lib.rs)：两处命令注册表（specta `collect_commands!` 与 `generate_handler!`）加 `zmodem_respond`（bindings.ts 编译期自动生成）。
- `ssh::disconnect` 顺带清理该 key 的 zmodem 条目（会话断开时任务经管道 Disconnected 退出）。

## 前端

### 6. [types.ts](src/api/types.ts) + [ssh.ts](src/api/ssh.ts)

- 事件类型：`ZmodemStartEvent { key }`、`ZmodemProgressEvent { key, file_name, transferred, total }`、`ZmodemEndEvent { key, ok, message }`（与后端契约同名同构，snake_case 字段）。
- 订阅函数：`listenZmodemStart/Progress/End`（沿用 listenHostkeyPrompt 模式）。
- 命令封装：`zmodemRespond(key, action, localPath)`（Uint8Array 归一化惯例不适用，纯字符串参数）。

### 7. 新建 `src/components/ssh/terminal/ZmodemDialog.vue`

HostkeyDialog 同套模式（v-dialog persistent + 队列去重），挂载到 [WorkspaceView.vue](src/views/WorkspaceView.vue)（HostkeyDialog 旁）：

- `zmodem-start` → 弹「检测到 ZMODEM 传输请求」三按钮：
  - **接收文件…**：`open({ directory: true, multiple: false })` → `zmodemRespond(key, "recv", dir)`（远端 `sz` 的文件名由 ZFILE 帧解析，存入所选目录；多文件自动全部接收）；
  - **发送文件…**：`open({ multiple: false })` → `zmodemRespond(key, "send", path)`（远端 `rz`）；
  - **取消**：`zmodemRespond(key, "cancel", "")`；Esc 强制关闭视作取消。
- 传输中：对话框切换为进度条（v-progress-linear，zmodem-progress 驱动，显示文件名与已传/总量）。
- `zmodem-end`：关闭对话框；`ok=false` 时终端区提示错误。

## 不做

- Telnet / 串口 / 本地终端的 ZMODEM（本次仅 SSH 终端；架构上 read_loop 是 SSH 专属，其他链路后续可复用同一 service）。
- 同步浏览（架构文档同列未排期项）。

## 验证

1. `rustc --version` ≥1.85；`cargo build`（src-tauri）绿。
2. `npm run build`（vue-tsc 类型检查）绿。
3. 连接真实 SSH 服务器（研发阶段 CDP 9222 端口驱动 UI 验证）：
   - 终端敲 `rz` → 弹对话框 → 选文件 → 上传进度 → 成功，终端恢复正常提示符；
   - 远端 `sz <file>` → 弹对话框 → 选目录 → 下载成功；
   - 弹框后点取消 → 远端中止，终端无乱码残留；
   - 传输中关闭会话 → 无悬挂任务、无崩溃。
