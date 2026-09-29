# FyShell 终端文件传输：彻底剔除 rzsz 命名 + 独立重写为原生 rzsz 模块

## Context（背景）

用户三轮指令演进：
1. 「去掉 rz/sz 改成全部自己编写」——已完成：`rzsz2` crate 完全移除，自研 `rzsz_protocol.rs`（约 2100 行），6 个对局测试全绿。
2. 「rzsz 前端依然存在 而且使用 rz sz 命令导致鼠标无法操作」——报告前端仍存在 + 内存锁死。
3. 「要完全剔除所有 rzsz 相关插件前端后端，基于功能全部进行独立重写」——要求彻底消灭 `rzsz` 字样与第三方结构，改为原生模块。

**已与用户确认的范围**（AskUserQuestion）：
- **命名+结构独立化（推荐）**：保留自研协议逻辑（wire 兼容是刚需），移除**所有** rzsz 命名/组件/事件/命令，改名重构为原生「终端文件传输」模块，彻底消灭 `rzsz` 字样。
- 鼠标根因 = **传输进行中 persistent 对话框挡住终端**（取消已修，1s 内生效；但传输本身可能挂起 → 需真实 lrzsz 兼容性验证）。

**关键约束**：SSH 终端的 rz/sz 必须与真实 lrzsz 互通，因此**线协议必须是 RZSZ（帧名/字节不可改）**。能剔除的只是命名/依赖/外部组件结构。协议帧常量（ZRQINIT/ZRINIT/ZFILE/ZDATA…）是 wire 规范名，本身不含 `rzsz` 字样，**保留**。

现状足迹：15 个文件 216 处 `rzsz|Rzsz|RZSZ`（不含 node_modules/target）。第三方插件依赖**已 100% 移除**（Cargo.lock 0 匹配），剩余全部是项目内自研命名。

## 新命名体系

推荐 `rzsz` 前缀，整体含义「终端 rz/sz 文件传输」：

### 后端（src-tauri）
| 原 | 新 |
|---|---|
| `services/rzsz_protocol.rs`（mod `rzsz_protocol`） | `services/rzsz_protocol.rs`（mod `rzsz_protocol`） |
| `services/rzsz.rs`（mod `rzsz`） | `services/rzsz.rs`（mod `rzsz`） |
| 枚举 `RzszChoice` | `RzszChoice` |
| 结构 `RzszEntry` | `RzszEntry` |
| `RzszStartEvent`/`RzszProgressEvent`/`RzszEndEvent` | `RzszStartEvent`/`RzszProgressEvent`/`RzszEndEvent` |
| 事件字符串 `"rzsz-start"`/`"rzsz-progress"`/`"rzsz-end"` | `"rzsz-start"`/`"rzsz-progress"`/`"rzsz-end"` |
| `state.rzsz_sessions`（state.rs:37,53） | `state.rzsz_sessions` |
| `RZSZ_SENTINEL`（ssh.rs:498） | `RZSZ_SENTINEL` |
| `find_rzsz_sentinel`（ssh.rs:501） | `find_rzsz_sentinel` |
| `rzsz_pipe_tx`（ssh.rs:515） | `rzsz_pipe_tx` |
| 命令 `rzsz_respond`（commands/ssh.rs:84） | `rzsz_respond` |
| lib.rs 两处 `collect_commands!` 注册（:66, :283） | `rzsz_respond` |

### 前端（src）
| 原 | 新 |
|---|---|
| `components/ssh/terminal/RzszDialog.vue` | `RzszDialog.vue`（标题「rz/sz 传输」→「rz/sz 文件传输」） |
| api/ssh.ts `rzszRespond`/`listenRzszStart`/`listenRzszProgress`/`listenRzszEnd` | `rzszRespond`/`listenRzszStart`/`listenRzszProgress`/`listenRzszEnd` |
| api/types.ts `RzszStartEvent`/`RzszProgressEvent`/`RzszEndEvent` | `RzszStartEvent`/`RzszProgressEvent`/`RzszEndEvent` |
| Dialog 内 `RzszXxxPayload` + 样式类 `.rzsz-*` | `RzszXxxPayload` + `.rzsz-*` |
| `bindings.ts`（自动生成，lib.rs export_ipc_bindings 在 debug 构建时覆盖） | 重新生成后出现 `rzszRespond` |
| WorkspaceView.vue:29 import + :3724 `<RzszDialog />` | `RzszDialog` |

### 保留不动
协议帧常量（wire 规范名）：`ZPAD/ZDLE/XON/ZRQINIT/ZRINIT/ZSINIT/ZFILE/ZDATA/ZEOF/ZFIN/ZACK/ZRPOS/ZSKIP/ZABORT/ZCAN/ZCRCE/ZCRCG/ZCRCQ/ZCRCW`、`ENC_ZBIN/ZHEX/ZBIN32`、`CANFC32/CANOVIO`、`SUBPACKET_MAX_SIZE` 等，以及 `Action/Event/Receiver/Sender/ProtocolError/FileInfo/Position` 等协议公开 API 名。

## 执行步骤

### 1. 文件重命名（git mv 保留历史）
```bash
git mv src-tauri/src/services/rzsz.rs           src-tauri/src/services/rzsz.rs
git mv src-tauri/src/services/rzsz_protocol.rs  src-tauri/src/services/rzsz_protocol.rs
git mv src/components/ssh/terminal/RzszDialog.vue src/components/ssh/terminal/RzszDialog.vue
```

### 2. 内容批量替换（三种大小写模式，大小写敏感 `-creplace`）
对 `src-tauri/src/**/*.rs`、`src/**/*.{ts,vue}`（排除 bindings.ts，它由构建覆盖；README.md 单独处理）执行：
```
rzsz  -> rzsz
Rzsz  -> Rzsz
rz/sz  -> RZSZ
```
可一次 PowerShell 循环完成（示例，执行时按需调整）：
```powershell
$pat = 'src-tauri/src','src'; $files = Get-ChildItem -Recurse -File -Include *.rs,*.ts,*.vue $pat
foreach ($f in $files | Where-Object { $_.Name -ne 'bindings.ts' }) {
  $c = Get-Content -Raw $f.FullName
  $c = $c -creplace 'rzsz','rzsz' -creplace 'Rzsz','Rzsz' -creplace 'RZSZ','RZSZ'
  Set-Content -NoNewline -Encoding utf8 $f.FullName $c
}
```
校验：`git grep -i rzsz` 应只剩 README.md、bindings.ts（旧，构建后消失）、协议注释内帧名（不匹配，因帧名无 rzsz 字样）。

### 3. 手工润色（批量替换后的注释/文案）
- 模块 doc 与注释里变形的「rz/sz 协议/传输」→ 中文自然表述「rz/sz 文件传输」「终端文件传输协议」（rzsz.rs、rzsz_protocol.rs、ssh.rs、state.rs、commands/ssh.rs 顶部 doc）。
- 用户可见文案：对话框标题 → 「rz/sz 文件传输」；错误信息里「未知的 rz/sz 操作」「没有进行中的 rz/sz 传输」等 → 改「rz/sz」表述（commands/ssh.rs、rzsz.rs 的 Err 字符串）。
- README.md 两处 `rzsz` 字样 → 移除或改「rz/sz 文件传输」。

### 4. 后端验证
```bash
cd src-tauri && cargo check
# 自研协议对局测试（重命名后回归）：rustc --test 独立编译绕过 DLL 问题（记忆：STATUS_ENTRYPOINT_NOT_FOUND）
rustc --edition 2021 --test src/services/rzsz_protocol.rs -o /tmp/rzsz_test && /tmp/rzsz_test
```
6 个对局测试必须全绿（纯重命名不得改变协议行为）。

### 5. 前端验证
- debug 构建重新生成 bindings.ts：`cargo build`（debug）后确认 `src/bindings.ts` 出现 `rzszRespond`，无 `rzszRespond`。
- `npx vue-tsc --noEmit`（或项目现有类型检查脚本）通过（WorkspaceView/RzszDialog/api/*.ts 引用一致）。

### 6. 发布构建
```bash
npx tauri build --features portable --no-bundle
```
（完成前重启 9222 调试端口为 --noerrdialogs 的既有配置已就位。）

### 7. 真实 lrzsz 兼容与「传输挂起」根因（附带，需真实环境）
自研对局测试是 self↔self，从未与真实 lrzsz 对局——这是「传输挂起」最可能的根因方向。流程：
1. **帧级诊断日志**：在 rzsz.rs 的收发路径加可选诊断输出（写用户数据目录 log 文件，不经终端/管道，避免污染线协议）。关键点：ZRINIT 后对端首个 ZACK/ZFILE 到来前、ZDATA 子包 CRC、ZFIN 完成握手阶段各打一行。
2. **请用户用最新便携版（步骤 6 产物）在测试服务器 192.168.31.100 复现** rz/sz，回传诊断日志与现象（「传输进行中对话框」是否仍挂起、取消是否 1s 内退出）。
3. **对照真实字节流**：若仍挂起，抓 lrzsz 实际输出帧头序列（可让用户 `script`/另终端记录，或测试服务器 `strace`），与自研 receiver 期望态逐帧比对，定位握手假设偏差（如 ZRINIT 应答时机、CANOVIO/buffer_len=0 声明、ZDATA 首个子包前是否需要先收 ZACK）。

## 验证清单（完成标准）
- [ ] `git grep -i rzsz` 全仓库仅剩 0 处（README、注释全清；bindings.ts 重新生成后无残留）
- [ ] `cargo check` + 自研协议 6 对局测试全绿
- [ ] debug 构建重新生成 bindings.ts，`rzszRespond` 出现
- [ ] 前端 vue-tsc 通过
- [ ] portable 便携版构建成功
- [ ] 用户真实 rz/sz e2e：无「传输挂起」，取消 ≤1s 生效，鼠标全程可操作

## 风险与注意
- **协议行为零变化**是硬约束：本次纯重命名，禁止顺手改协议逻辑；协议 bug 单独在步骤 7 处理。
- bindings.ts 在 debug 构建被覆盖 —— 不要手工维护，靠重新生成。
- 事件字符串重命名后，旧前端事件监听会失效——前端同步改（RzszDialog + api/ssh.ts 同时落地），不存在跨版本混用窗口（桌面应用同包升级）。
- 这轮改动面广（15 文件），分后端→前端→构建三步各自验证，避免一处失败连锁。
- memory 更新：在 `rzsz-self-written-protocol.md` 补一条重命名落地记录（命名已 rzsz 化、帧常量保留、真实对局验证结果），并更新 MEMORY.md 索引条目标题。
