# 远程文件文本编辑（SFTP 对齐 WinSCP）

## Context（背景与目标）

用户反馈「没有文本编辑功能」。现状缺口：远程文件右键菜单只有 新建/重命名/权限/删除/传输/终端定位，双击文件提示「暂不支持打开文件…」（[FilePane.vue:543-551](../../src/components/sftp/FilePane.vue#L543-L551)），后端也无读写远程文件内容的命令。WinSCP 语义（右键「编辑」+ 双击文本文件进入编辑器、保存写回）完全缺失。

**用户已确认的设计决策（AskUserQuestion）：**
- **形态**：对话框内嵌编辑器（v-dialog + textarea，复用现有 v-dialog 体系，符合 12px/紧凑密度）。
- **范围与触发**：仅远程文件；右键「编辑」+ 双击文本文件均进入编辑；UTF-8 读取，>2MB 或含 NUL/非 UTF-8（二进制）报错引导下载；本地窗格双击维持现状。

**已确认的复用点：**
- 后端读文件可复用 `download_file` 的 `sftp.open(path)` + 循环 `read()` 模式（[sftp.rs:293](../../src-tauri/src/services/sftp.rs#L293)）；写文件复用 `open_with_flags` + `write_all` + `sync_all`（[sftp.rs:217](../../src-tauri/src/services/sftp.rs#L217)）。
- 等宽字体用现成 CSS 变量 `--fy-mono`（[theme.css:29](../../src/styles/theme.css#L29)）。
- bindings.ts 由 tauri-specta 在 debug 构建时自动生成（[lib.rs:232-236](../../src-tauri/src/lib.rs#L232-L236)），新增命令重编译即自动同步，无需手改。

## 改动清单

### 1. 后端服务层：读/写远程文件文本（[sftp.rs](../../src-tauri/src/services/sftp.rs)）
在文件末尾 `download_file` 之后新增两个公开函数（复用 `open_sftp`/`sftp_err`/`FileAttributes`）：

- `pub async fn read_text(state, id, path) -> Result<String, AppError>`
  - 常量 `EDIT_MAX_BYTES: u64 = 2 * 1024 * 1024`（2MB 编辑上限）。
  - `metadata(path)` 预检：size > 上限 → `AppError::general("文件过大（.. 字节），超过编辑上限 2MB，请下载后编辑")`。
  - `sftp.open(path)` + 4KB 循环 `read()` 收集到 `Vec<u8>`（途中累计超上限同样报错）。
  - `String::from_utf8(buf)` **严格校验**：失败（二进制/含 NUL/非 UTF-8 编码）→ `AppError::general("文件不是 UTF-8 文本（可能为二进制或其它编码），无法编辑")`。
- `pub async fn write_text(state, id, path, content: &str) -> Result<(), AppError>`
  - `open_with_flags(path, WRITE | CREATE | TRUNCATE)` + `write_all(content.as_bytes())`；`sync_all()` 失败不视为失败（与 upload_file 一致）。
  - 全量覆盖语义：保存即整文件写回。

`AsyncReadExt`/`AsyncWriteExt` 已在文件顶部导入（L8），`OpenFlags` 已导入（L7）。

### 2. 命令层：两个薄命令（[commands/sftp.rs](../../src-tauri/src/commands/sftp.rs)）
仿照现有 `sftp_list`/`sftp_chmod` 模式：
- `sftp_read_text(state, id: String, path: String) -> Result<String, AppError>` → `sftp_service::read_text`
- `sftp_write_text(state, id: String, path: String, content: String) -> Result<(), AppError>` → `sftp_service::write_text(&content)`

均带 `#[tauri::command]` + `#[specta::specta]`。重编译后 bindings.ts 自动新增 `sftpReadText` / `sftpWriteText`。

### 3. 前端 API 封装（[api/sftp.ts](../../src/api/sftp.ts)）
末尾追加（仿现有封装风格，注明契约）：
- `sftpReadText(id, path): Promise<string>`
- `sftpWriteText(id, path, content): Promise<void>`

### 4. 新组件：远程文本编辑对话框（新建 [src/components/sftp/RemoteTextEditorDialog.vue]）
- **props**：`modelValue: boolean`（v-model 显隐）、`sessionId?: string`、`path: string`（远程绝对路径）。
- **emits**：`update:modelValue`、`saved`（保存成功后通知 FilePane 刷新）。
- **行为**：
  - `watch(modelValue)` 打开 → 调 `sftpReadText` 加载内容；`loading` 遮罩；错误在对话框内 `v-alert` 展示（含「过大/二进制」提示），并关闭按钮仍在。
  - `v-textarea` 编辑区：`font-family: var(--fy-mono)`、`spellcheck="false"`、约 `60vh` 高 + auto-grow、`density="compact"`。
  - 标题栏沿用现有对话框样式（`d-flex align-center` + 关闭图标），副行展示 `path`。
  - 保存 → `sftpWriteText` → `ui.toast('已保存 …', 'success')` → `emit('saved')`、关闭对话框；失败 toast 错误。
  - Ctrl+S 快捷键保存；`executing` 时按钮 loading 防重入。
- 复现 ImportExportDialog/DataGenerateDialog 的 v-dialog 结构（[ImportExportDialog.vue:1](../../src/components/mysql/ImportExportDialog.vue#L1)）。

### 5. 接入 FilePane（[FilePane.vue](../../src/components/sftp/FilePane.vue)）
- 引入并挂载 `<RemoteTextEditorDialog v-model="editorOpen" :session-id="props.sessionId" :path="editorPath" @saved="refresh" />`。
- 状态：`editorOpen = ref(false)`、`editorPath = ref('')`。
- **右键菜单**（L174-196 的 v-menu）：在「重命名」上方新增 `<v-list-item v-if="side === 'remote' && menu.entry && !menu.entry.is_dir" @click="actionEdit">编辑</v-list-item>`。
- `actionEdit()`：`menu.visible = false`；`editorPath.value = joinPath('remote', currentPath.value, menu.entry.name)`；`editorOpen.value = true`。
- **双击**（`openEntry` L543-551）：文件分支增加「远程侧 → 打开编辑器」；本地侧文件维持「暂不支持打开文件」提示；远程目录/本地目录行为不变。
- 保存成功后经 `@saved="refresh"` 刷新当前目录（后端 TRUNCATE 覆盖，列表大小/时间已变）。

> 仅远程侧接入；本地侧不加（用户已确认范围只远程）。

## 验证（端到端，测试服务器 192.168.31.100）

1. **编译**：后台 tauri dev 自动重编译；`cargo check` 通过；bindings.ts 检查新增 `sftpReadText`/`sftpWriteText`。
2. **右键编辑**：CDP 打开独立双栏 → 在远端建一个 `.txt`（内容已知）→ 右键 → 「编辑」→ 对话框加载内容 → 修改 → 保存 → paramiko `cat` 校验远端内容 == 修改后内容。
3. **双击编辑**：双击远程文本文件 → 直接进入编辑对话框（不再提示「暂不支持打开文件」）。
4. **二进制拦截**：右键一个 `.bin`（含 NUL）→ 「编辑」→ 后端报「不是 UTF-8 文本…无法编辑」，toast/对话框内展示。
5. **过大拦截**：上传一个 >2MB 文件 → 编辑 → 报「文件过大…请下载后编辑」。
6. **回归**：本地窗格双击文件仍提示不支持；远程双击目录仍进入；右键菜单其余项（重命名/权限/删除/传输）不受影响；保存后当前目录列表刷新。

## 涉及文件
- [src-tauri/src/services/sftp.rs](../../src-tauri/src/services/sftp.rs)（改动 1）
- [src-tauri/src/commands/sftp.rs](../../src-tauri/src/commands/sftp.rs)（改动 2）
- [src/api/sftp.ts](../../src/api/sftp.ts)（改动 3）
- src/components/sftp/RemoteTextEditorDialog.vue（新建）＋ [src/components/sftp/FilePane.vue](../../src/components/sftp/FilePane.vue)（改动 4/5）
- src/bindings.ts（自动生成，不手改）

> 备注：tauri.conf.json 仍含未提交的研发 CDP 端口 `--remote-debugging-port=9222`，正式打包前替换为 `--noerrdialogs`（非删除）；本会话不触碰、不提交其变更。
