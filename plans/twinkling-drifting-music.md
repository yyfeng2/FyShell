# SSH 专属设置对话框（会话选项）

> **实施记录（2026-09-17，已全部完成）**。与原方案的偏差：
> - **SSH 安全性页收敛**：russh 0.63 的 `negotiation::Preferred` 算法字段为 `Cow<'static, [Name]>`（`Name` 是 `&'static str` 包装），运行时动态白名单不可行——剔除原方案的 `sshopt_kex`/`sshopt_cipher`/`sshopt_mac` 三个文本键，安全性页只保留 `sshopt_hostkey_strict`（主机 key 变更直接拒绝）+ `sshopt_compression` 两个真实生效项
> - **关键词高亮实现**：`@xterm/addon-search` 0.16 公共 API 无 `highlightAllMatches`，改用 xterm 内置 decorations API（`registerDecoration` + marker，`allowProposedApi` 已开启），扫描缓冲末尾 3000 行、500 decoration 上限、500ms 防抖
> - **登录提示符自动响应**：64 字符滚动缓冲 + 尝试计数 + 2s 冷却，匹配行尾 `login:`/`username:`/`password:`
> - 后端 `cargo check` 与前端 `vue-tsc --noEmit` 均通过（无警告）


## Context

用户要求为 SSH 标签专门设计设置功能，参考图是 SecureCRT 风格的「会话选项」对话框（左侧树形导航：连接/终端/外观/高级四大组 + 缩进子项）。

经确认的需求边界：
- **作用域**：全局生效（对所有 SSH 连接生效，不做按会话覆盖）
- **树结构**：完整照搬图片树（1:1），包括 TELNET/RLOGIN/串口等 FyShell 暂不支持的类型占位页
- **实现深度**：全部面板实现，包括新后端能力（代理连接、登录脚本、响铃、关键字高亮等）
- **入口**：SSH 标签右键菜单「会话设置」

现有基础：全局设置对话框（[SettingsDialog.vue](src/components/common/SettingsDialog.vue)）为平铺分类导航；后端设置存 SQLite `settings` 表（key-value 文本，[settings_store.rs](src-tauri/src/services/settings_store.rs) 的 `get_all`/`get`/`set`），无 schema 迁移成本——新增键即插即用。

**⚠️ 并行工作协调**（重要）：工作区存在另一会话的未提交改动——键位映射全链路（`key_mapping.rs` 后端 + `stores/keyMapping.ts` + useXterm 拦截器，已完成）、鼠标/选择设置（settings.ts 已扩展 11 键）、updater、本地终端（portable-pty）与串口（serialport）依赖。本方案：
- 新设置键一律用 `sshopt_` 前缀，避免与现有 `theme_mode`/`terminal_*`/`mouse_*`/`selection_*` 冲突
- `WorkspaceView.vue`、`settings.ts` 正被并行修改——对它的改动保持纯增量（挂载新组件 + 接线事件），不重构既有代码
- 键位映射后端/store/拦截器已存在（`useXterm.ts` 的 `keyMappingInterceptor` 已接线）；本方案的「键盘」页做管理 UI（CRUD 表格），若并行会话的全局键位映射编辑器组件先落地则直接复用

## 树结构（1:1 照搬图片）

```
连接                              组标题
  ├ 用户身份验证                   (auth)
  │  └ 登录提示符                 (login-prompt)
  ├ 登录脚本                       (login-script)
  ├ SSH
  │  ├ 安全性                     (ssh-security)
  │  ├ 隧道                       (ssh-tunnel)
  │  └ SFTP                       (ssh-sftp)
  ├ TELNET                        (telnet，占位)
  ├ RLOGIN                        (rlogin，占位)
  ├ 串口                          (serial，占位)
  ├ 代理                          (proxy)
  └ 保持活动状态                   (keepalive)
终端                              组标题
  ├ 键盘                          (keyboard)
  ├ VT 模式                       (vt-mode)
  └ 高级                          (term-advanced)
外观                              组标题
  ├ 窗口                          (window)
  └ 突出显示                       (highlight)
高级                              组标题
  ├ 跟踪                          (trace)
  ├ 响铃                          (bell)
  └ 日志记录                       (logging)
```

## 设置项清单（新增键均 `sshopt_` 前缀，存 settings 表 key-value）

| 页面 | 键 | 类型 | 默认值 | 控件 |
|---|---|---|---|---|
| 用户身份验证 | `sshopt_default_auth_type` | string | `password` | v-select（5 种认证方式，新建会话预填） |
| | `sshopt_default_profile_id` | string | `''` | v-select（认证配置文件，新建会话预填） |
| 登录提示符 | `sshopt_prompt_auto_respond` | bool | false | v-switch |
| | `sshopt_prompt_username` / `sshopt_prompt_password` | string | `''` | 文本/密码框 |
| | `sshopt_prompt_max_attempts` | number | 3 | 数字（1-10） |
| 登录脚本 | `sshopt_script_enabled` | bool | false | v-switch |
| | `sshopt_script_content` | string | `''` | textarea 多行命令 |
| | `sshopt_script_delay` | number | 200 | 数字（毫秒，行间隔） |
| SSH 安全性 | `sshopt_hostkey_strict` | bool | false | v-switch（主机 key 变更直接拒绝不弹窗） |
| | `sshopt_compression` | bool | false | v-switch（russh flate2 特性已启用） |
| | `sshopt_kex` / `sshopt_cipher` / `sshopt_mac` | string | `''` | 文本框（逗号分隔白名单，空=默认） |
| SSH 隧道 | `sshopt_tunnel_auto_start` | bool | true | v-switch（会话连接时自动启动已启用隧道） |
| | `sshopt_tunnel_listen_host` | string | `127.0.0.1` | 文本框（新隧道规则预填） |
| SSH SFTP | 复用 `sftp_download_dir` | string | `''` | 文本 + 浏览按钮（与全局设置读写同一 key） |
| 代理 | `sshopt_proxy_enabled` / `sshopt_proxy_type` / `sshopt_proxy_host` / `sshopt_proxy_port` / `sshopt_proxy_username` / `sshopt_proxy_password` | 见左 | false / `socks5` / `''` / 1080 / `''` / `''` | v-switch + v-select + 文本框组 |
| 保持活动状态 | `sshopt_keepalive_enabled` | bool | true | v-switch（关 = 全局禁用 keepalive） |
| | `sshopt_keepalive_interval` | number | 30 | 数字（1-3600 秒，新建会话默认值） |
| 键盘 | （无新键）键位映射管理 UI，走已有 `keyMappingStore` CRUD | — | — | CRUD 表格 |
| VT 模式 | `sshopt_vt_term_type` | string | `xterm-256color` | v-select（xterm-256color/xterm/vt100/vt220/linux/ansi） |
| 终端高级 | （滚动缓冲复用 `terminal_scrollback`） | | | |
| | `sshopt_term_webgl` | bool | true | v-switch（关闭回退 DOM 渲染，排查用） |
| 窗口 | `sshopt_default_tab_color` | string | `''` | 颜色选择（新建会话预填） |
| 突出显示 | `sshopt_highlight_enabled` | bool | false | v-switch |
| | `sshopt_highlight_rules` | string(JSON) | `[]` | 规则表格（关键词/颜色），`{keyword,color}[]` |
| 跟踪 | `sshopt_trace_enabled` | bool | false | v-switch |
| 响铃 | `sshopt_bell_style` | string | `off` | v-select（off/sound/visual/both） |
| 日志记录 | `sshopt_log_auto_start` | bool | false | v-switch（新会话自动落盘）+ 打开日志目录按钮 |

## 后端改造

### 1. 新文件 `src-tauri/src/services/ssh_proxy.rs`（代理）
- `ProxyConfig { kind, host, port, username, password }` + `from_settings()`（读 `sshopt_proxy_*`，未启用或 host 空返回 None）
- `pub async fn connect(target_host, target_port, &proxy) -> Result<TcpStream, AppError>`：
  - SOCKS5：TCP 连代理 → greeting（0x05 + 方法）→ 可选 RFC 1929 用户名/密码子协商 → CONNECT 请求（域名模式，避免本地解析 IP）→ 校验回应码
  - HTTP CONNECT：`CONNECT host:port HTTP/1.1` + 可选 `Proxy-Authorization: Basic`（base64 手工编码约 20 行，无 base64 crate）→ 读到 `\r\n\r\n` 校验 "200"
- **ssh.rs `connect()` 改造**（[ssh.rs:473-486](src-tauri/src/services/ssh.rs#L473-L486)）：命中代理时 `ssh_proxy::connect(...)` → `russh::client::connect_stream(Arc::new(config), stream, handler)`（russh 0.63 已确认有该 API）；未命中保持原 `connect(...)`

### 2. 新文件 `src-tauri/src/services/login_script.rs`（登录脚本）
- `parse(content) -> Vec<ScriptLine>`（按行拆分，`=>` 分隔 expect/send 语法保留解析，本期固定间隔逐行发送，面板提示中说明）
- ssh.rs `request_shell` 成功后（[ssh.rs:503-512](src-tauri/src/services/ssh.rs#L503-L512)）读 `sshopt_script_*`，启用时 spawn 任务：经既有 `SshWriteMsg::Data` → write_forward 路径逐行发送，行间 sleep delay，发送前校验会话存活

### 3. ssh.rs 其余改造点
- **VT 模式**：`request_pty(true, "xterm-256color", ...)` 硬编码改为读 `sshopt_vt_term_type`（白名单校验，缺省回退 xterm-256color）
- **保持活动状态**：`config.keepalive_interval`——`sshopt_keepalive_enabled` 为 false 时置 `None`（全局禁用）；开启时保持按会话值
- **跟踪**：新薄层 `services/trace.rs`（`AtomicBool` + `refresh_from_settings()` + `enabled()`），把 ssh.rs 中 6 处诊断类 `eprintln!`（connect start / tcp+kex+hostkey done / auth done / pty/shell ready 等）包进 `if trace::enabled()`；错误输出保持无条件
- **SSH 安全性**：`sshopt_hostkey_strict` 开启时 `check_server_key`（[ssh.rs:196-246](src-tauri/src/services/ssh.rs#L196-L246)）中主机 key 变更直接 `Ok(false)` 不弹窗；kex/cipher/mac 白名单非空时覆盖 `Config.preferred`（russh 0.63 `negotiation::Preferred`，实现时核验字段名）；`sshopt_compression` 开启时启用压缩算法
- **日志记录**：connect 成功后读 `sshopt_log_auto_start`，为 true 时调 `session_log::toggle(key, true)`

### 4. 登录提示符（纯前端）
自动响应提示符：`TerminalPane.vue`/useXterm 输出流上匹配行尾 `login:` / `username:` / `password:` 提示，自动发送配置的用户名/密码（防抖 + `sshopt_prompt_max_attempts` 尝试计数，开关关闭时不干预）。

## 前端改造

### 1. 新 Pinia store `src/stores/sshOptions.ts`
镜像 `stores/settings.ts` 结构：`SSH_OPT_KEYS` 常量 + `DEFAULTS` + 状态 ref + 单键 setter（统一 `persist` 静默写回）；`ensureLoaded()` 复用 `api/settings.ts` 的 `settingsGetAll`（一次拉全，幂等），按 key 解析（bool/number/JSON）。API 层无新增。

### 2. 新对话框 `src/components/ssh/options/`（按组拆分）

```
options/
├── SshOptionsDialog.vue      # 壳：v-dialog(max-width 860) + 树形两级导航 + 面板切换 + 占位页
├── types.ts                  # 组/叶子 key 类型 + NAV 注册表（标题/图标/组件映射）
└── panels/
    ├── ConnectionAuthPanels.vue   # 用户身份验证 + 登录提示符
    ├── LoginScriptPanel.vue
    ├── SshPanels.vue              # 安全性/隧道/SFTP（page prop 切换）
    ├── ProxyPanel.vue
    ├── KeepAlivePanel.vue
    ├── TerminalPanels.vue         # 键盘/VT 模式/高级
    ├── AppearancePanels.vue       # 窗口/突出显示
    └── AdvancedPanels.vue         # 跟踪/响铃/日志记录
```

- 壳组件：左侧树复用 SettingsDialog 的样式模式（拷贝其 scoped CSS 扩展 `--group` 组标题加粗、`--leaf` 缩进 22px 两级），右侧 `__section-title`/`__field`/`__hint`/`__row` 样式类保持一致；`max-width="860"`；底部仅「关闭」（变更即写回，与现有设置一致）；标题「SSH 选项」+ 副标题「设置对所有 SSH 连接全局生效」
- 占位页（telnet/rlogin/serial）：共用内联模板块，图标 + 文案「FyShell 暂不支持该连接类型，此分组保留以对齐 SecureCRT 设置布局」
- 键盘页：键位映射 CRUD 表格（复用 `keyMappingStore` 的 `save`/`remove`/`reload` 与 `api/keyMapping.ts`；若并行会话的全局键位映射编辑器组件已落地则复用该组件）

### 3. useXterm 联动（[useXterm.ts](src/composables/useXterm.ts)，3 处独立改造）
1. **响铃**：`open()` 设 `bellStyle='none'` 自管 + `term.onBell()` 回调——`sound` 时 Web Audio（OscillatorNode）短促 beep，`visual` 时容器 CSS class 闪烁；读 sshOptions store
2. **关键字高亮**：安装 `@xterm/addon-search`（现仅有 fit/webgl），写入后防抖调用 `highlightAllMatches`（关键词组合为正则，命中高亮样式取规则颜色）；设置变更时清空重扫；`dispose()` 清理
3. **VT 模式**：无需前端改动（term 名在后端 request_pty 读取）

### 4. 入口接线
- [FlexTabs.vue](src/components/common/FlexTabs.vue)：菜单加「会话设置」项（复制名称之后），`emit('session-settings', tabId)`（FlexTabs 未被并行会话修改，安全）
- [WorkspaceView.vue](src/views/WorkspaceView.vue)：`@session-settings` 接线 + 挂载 `<SshOptionsDialog v-model="showSshOptionsDialog" />`（纯增量改动）

## 实施顺序

1. 后端独立模块（可并行）：`trace.rs` → `ssh_proxy.rs` → `login_script.rs`
2. ssh.rs connect() 改造：代理分支 → VT term → keepalive → 登录脚本 → trace 包裹 → hostkey strict → 日志默认
3. 前端 store：`sshOptions.ts`
4. 对话框：`types.ts` → `panels/*` 8 组件 → `SshOptionsDialog.vue` 壳
5. useXterm 联动：响铃 + 高亮 + 登录提示符自动响应
6. 入口接线：FlexTabs 菜单项 → WorkspaceView 挂载

## 验证

1. `cargo check`（src-tauri）+ `vue-tsc --noEmit`（前端）无错误
2. 对话框：SSH 标签右键 → 会话设置 → 树形导航逐页切换，各设置项变更后重启应用仍保留（settings 表）
3. 代理：配置 SOCKS5（无认证/带认证）与 HTTP CONNECT 代理后连接，`connect_stream` 路径经代理建流成功；无代理时回归原路径
4. 登录脚本：连接后命令按行间隔自动到达 shell；VT 模式切换后终端内 `echo $TERM` 验证
5. 响铃：终端内 `echo -e '\a'` 触发声音/视觉提示；高亮：配置关键词后终端输出中命中词着色
6. 跟踪：开启后 stderr 输出连接过程详细诊断日志；hostkey strict：变更 key 的主机连接时直接拒绝（不弹确认框）
