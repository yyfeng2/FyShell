# 键盘和鼠标设置页 + 键位映射编辑器

## Context

用户要求按参考截图（Xshell「键盘和鼠标」设置页）为 FyShell 添加并设计该功能，包含三组设置：**按键对应**（键位→动作自定义，带编辑按钮）、**鼠标**（中/右键行为、Ctrl+单击移动光标、URL 超链接）、**选择**（双击分隔符、自动复制、复制后处理等）。

经确认：
- 按键对应**完整实现**（键位映射管理对话框，支持「发送字符串」与「执行菜单命令」两类动作）
- 鼠标中/右键下拉只做两项：没做什么 / 粘贴剪贴板内容（当前终端无右键菜单，不冲突）

现有基础（探索结论）：
- 设置为 key-value 文本存 SQLite settings 表（`commands/settings.rs`），Pinia store `src/stores/settings.ts` 统一加载/写回；IPC 字段一律 snake_case
- 设置对话框 `src/components/common/SettingsDialog.vue`：左侧导航 + 右侧内容区，现有 外观/终端/SFTP/数据/安全/关于 六个分区
- 终端：`src/composables/useXterm.ts` 持有 Terminal 实例（设置 watch 联动），`src/components/ssh/terminal/TerminalPane.vue` 每窗格组件，onData → sshWrite；**目前无任何鼠标事件处理、无 URL 链接、无选中文本自动复制**
- xterm.js 5 已具备所需 API：`wordSeparator` 选项、`onSelectionChange`、`registerLinkProvider`、`paste()`、`getSelection()`（`@xterm/addon-web-links` 未安装，用内置 `registerLinkProvider` 实现 URL 链接，可自定义前缀过滤）
- `navigator.clipboard.readText/writeText` 在 Tauri WebView2 中可用（MysqlDataGrid 已在用）；`@tauri-apps/plugin-opener` 已安装且 capability 已含 `opener:default`
- 快捷命令的 SQLite 存储模式（`services/quick_command_store.rs`：OnceLock<Mutex<Connection>> + init/list/save/delete）可作为键位映射存储的样板
- ui store 已有 `shortcutOf(e)` 键盘事件归一化（"ctrl+t" 形式）可复用；菜单动作在 WorkspaceView `onMenuAction` 接线
- 注意：xterm `attachCustomKeyEventHandler` 单实例只挂一个 handler，`attachGlobalKeyPassthrough` 与键位映射拦截需合成一个 handler

## 实施步骤

### 1. 设置项扩展 — `src/stores/settings.ts`

新增 SETTING_KEYS（snake_case）+ DEFAULTS + ref 状态 + setter（复用现有 `persist`）：

| key | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `mouse_middle_button` | string | `'nothing'` | 中键行为：`nothing` / `paste` |
| `mouse_right_button` | string | `'paste'` | 右键行为 |
| `mouse_ctrl_click_move_cursor` | bool | `true` | Ctrl+左单击移动终端光标 |
| `mouse_url_hyperlink` | bool | `false` | 使用 URL 超链接 |
| `mouse_url_prefixes` | string | `http://\|https://\|ftp://\|ssh://\|telnet://\|sftp://` | URL 前缀（`\|` 分隔） |
| `mouse_ctrl_click_open_hyperlink` | bool | `false` | Ctrl+单击打开超链接 |
| `selection_word_separators` | string | `\:\~\-!@#$%^&*()-=+[]{}` | 双击选择分隔符 |
| `selection_shift_double_click` | bool | `false` | Shift+双击按分隔符而非空格选择 |
| `selection_auto_copy` | bool | `true` | 选中文本自动复制到剪贴板 |
| `selection_soft_tabs` | bool | `false` | 复制时制表符转空格 |
| `selection_copy_include_newline` | bool | `true` | 复制包含最后一个换行字符 |
| `selection_copy_trim_whitespace` | bool | `false` | 复制时删除尾部空白 |
| `selection_copy_nonblank_only` | bool | `false` | 复制时排除仅含空白的行 |

`ensureLoaded` 中按现有模式逐条解析；与图片默认勾选状态一致。

### 2. 终端接线 — `src/composables/useXterm.ts` + `src/components/ssh/terminal/TerminalPane.vue`

useXterm 内部（term 实例所有者）新增：

- **双击分隔符**：`term.options.wordSeparator` = 设置值（加入现有 settings watch 联动，新终端创建时同样生效）
- **选中文本后处理**（复制共用工具函数）：
  - `selection_soft_tabs`：`\t` → 4 空格（用 `term.options.tabStopWidth`）
  - `selection_copy_trim_whitespace`：删除每行尾部空白
  - `selection_copy_nonblank_only`：剔除仅含空白的行
  - `selection_copy_include_newline`：选中末尾补换行
- **自动复制**：`term.onSelectionChange` → `getSelection()` → 后处理 → `navigator.clipboard.writeText`
- **URL 超链接**：`registerLinkProvider` 按 `mouse_url_prefixes` 前缀正则匹配行内 URL；`mouse_ctrl_click_open_hyperlink` 为 true 时要求按住 Ctrl 才触发（`ILink` 的 `activate` 内检查 `ctrlKey`），经 `@tauri-apps/plugin-opener` 的 `openUrl` 打开；设置关闭时 dispose provider
- **鼠标事件**（容器级监听，`open()` 中注册、`dispose()` 中清理）：
  - 右键 `contextmenu` → `preventDefault`；设置为 paste 时 `term.paste(await navigator.clipboard.readText())`
  - 中键（`button === 1`）`auxclick` → 同上
  - Ctrl+左单击（`mousedown` + `ctrlKey`）：设置开启时计算点击 cell（`offsetX/Y / cellWidth/Height`，cell 尺寸 = screen 元素尺寸 / cols、rows）与 `term.buffer.active.cursorX/cursorY` 的行列差，发送对应数量的方向键序列（`\x1b[A/D/C/B`），走 `options.onData`；若点击位置命中 URL 链接（本组件 provider 已提供的范围）且开启 Ctrl 打开链接则跳过移动光标
- **键位映射拦截**：`attachGlobalKeyPassthrough` 重构为内部维护拦截器数组，单个 `attachCustomKeyEventHandler` 先过拦截器（命中返回 false），再走现有全局快捷键放行逻辑；新增 `registerKeyInterceptor(fn)` 返回注销函数

TerminalPane.vue：无大改，`send_string` 动作经 `registerKeyInterceptor` 的回调走现有 `sshWrite` 路径。

### 3. 键位映射 Rust 侧（新文件 ×3）

模仿 quick_command 模式（契约风格，IPC 字段 snake_case）：

- `src-tauri/src/models/key_mapping.rs`：
  ```rust
  pub struct KeyMapping {
      pub id: String,              // uuid v4
      pub key_combo: String,       // 归一化键位，如 "ctrl+shift+x"
      pub action_type: String,     // "send_string" | "menu_command"
      pub payload: String,         // 发送的字符串 或 菜单 action 名
  }
  ```
- `src-tauri/src/services/key_mapping_store.rs`：仿 `quick_command_store.rs` 的 `OnceLock<Mutex<Connection>>` 模式；`init`（建表 key_mappings：id/key_combo/action_type/payload）+ `list` + `save`（upsert）+ `delete`
- `src-tauri/src/commands/key_mapping.rs`：`key_mapping_list` / `key_mapping_save` / `key_mapping_delete` 薄层命令
- `src-tauri/src/lib.rs`：invoke_handler 注册三命令 + setup 中 `key_mapping_store::init(&data_dir)`

### 4. 键位映射前端

- `src/api/keyMapping.ts`：`keyMappingList` / `keyMappingSave` / `keyMappingDelete` invoke 封装
- `src/stores/keyMapping.ts`：Pinia store，启动加载映射列表（key_combo → 映射 Map，供 useXterm 拦截读取）、CRUD 动作并写回后端
- `src/components/common/KeyMappingDialog.vue`：管理对话框（v-dialog + v-card，风格对齐 SettingsDialog）：
  - 映射列表（键位 + 动作类型 + payload 摘要）+ 添加/编辑/删除
  - 表单：键位输入框（「捕获」按钮 keydown 捕获，复用 ui store `shortcutOf` 归一化）+ 动作类型下拉（发送字符串/执行菜单命令）+ payload 文本框（发送字符串用 textarea；菜单命令用下拉，选项对齐 MenuBar MENUS 的 action 集）
- `src/stores/ui.ts`：新增 `requestMenuAction(action)` + `menuActionRequest` ref（供终端内触发菜单命令）
- `src/views/WorkspaceView.vue`：watch `ui.menuActionRequest` → 调用现有 `onMenuAction`

### 5. 设置对话框「键盘和鼠标」分区 — `src/components/common/SettingsDialog.vue`

- `SettingsSection` 类型与 SECTIONS 数组新增 `'keyboard-mouse'`（图标 `mdi-keyboard-outline`，位置在「终端」之后）
- 内容区按图片三组布局（复用 `settings-dialog__row` / `__row-title` / `__row-desc` / `__hint` 样式）：
  - **按键对应**：说明文字 + 「编辑(E)...」按钮 → 打开 KeyMappingDialog
  - **鼠标**：中键/右键 v-select（没做什么/粘贴剪贴板内容）、Ctrl+左单击移动光标 v-switch、使用 URL 超链接 v-switch（下挂 URL prefix 文本框 + Ctrl+单击打开超链接 v-switch，联动显示）
  - **选择**：说明文字、分隔符文本框 + 重置按钮（恢复默认值）、Shift+双击选择、自动复制、软标签、包含换行、删尾部空白、仅非空白字符 v-switch

## 验证

1. `cd src-tauri && cargo check`（Rust 编译通过）
2. `npm run build`（vue-tsc + vite build 无错误）
3. `npm run tauri dev` 端到端：
   - 设置 → 键盘和鼠标：勾选/修改各项 → 重启应用确认持久化（SQLite settings 表）
   - 鼠标：终端选中文本验证自动复制（粘贴验证）、双击按分隔符选词、右键粘贴、Ctrl+单击移动光标、输入 URL 验证链接高亮与 Ctrl+单击打开
   - 键位映射：编辑器添加「发送字符串」与「执行菜单命令」映射各一条 → 终端按键触发验证生效 → 删除后失效
