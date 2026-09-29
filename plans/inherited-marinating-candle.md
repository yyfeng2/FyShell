# 导航树拖拽 + SFTP 快捷打开（绑定 SSH 实时 cwd）

## Context

用户提出两个需求：

1. **导航树拖拽**：目前左侧会话树只能按「星标 → 文件夹 → 名称」自动排序，完全不可拖拽。需要支持把**会话/文件夹**拖到其它文件夹里归类、同层拖拽调整顺序，且顺序要**持久化**（重启不丢）。
2. **SFTP 快捷打开**：
   - 工具栏「传文件」下拉里目前依次是「新建 SFTP 会话…」「快捷双栏」，要求把「快捷双栏」移到**首位**；
   - 「快捷双栏」打开时，远程栏应**自动跳到绑定那个 SSH 终端的实时 cwd**（用户明确选择：解析终端提示符/OSC 7 序列实时跟踪，而非 $HOME 或记住上次路径）。

已确认决策：会话+文件夹都可拖；快捷打开=「传文件」菜单的快捷双栏；ssh 所在目录=终端实时 cwd（OSC 7 追踪）。

---

## Part A：导航树拖拽（后端持久化 + 前端拖放）

### A1. 后端子模型
- `src-tauri/src/models/session.rs`
  - `SessionConfig`、`SessionFolder` 各加 `#[serde(default)] pub sort_order: u32`（0=未手工排序，按名称兜底）。
  - 新增排序载荷结构（specta 导出到 bindings）：
    ```rust
    /// 批量重排项：kind 区分文件夹/会话，parent_id 为其新父级
    #[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
    #[serde(rename_all = "camelCase")]  // 注意：普通字段 snake_case，与现有模型一致（AuthType 除外）
    pub struct NodeReorderItem {
        pub kind: String,          // "folder" | "session"
        pub id: String,
        pub parent_id: Option<String>,
        pub order: u32,
    }
    ```
    （遵守记忆约定：字段一律 snake_case，不加 rename_all。）

### A2. 后端存储与命令
- `src-tauri/src/services/config_store.rs`
  - `init_schema` 补两列迁移（与 profile_id 同款「duplicate column name」守卫）：
    `ALTER TABLE sessions ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0`、`ALTER TABLE folders ADD COLUMN sort_order ...`。
  - `list_nodes`：`FROM sessions ORDER BY sort_order, name`（替换现有 `ORDER BY name`）；文件夹同样 `ORDER BY sort_order, name`。
  - `save_session`：INSERT/ON CONFLICT 纳入 `sort_order` 列；`save_folder` 同理。
  - 新增 `reorder_nodes(&self, items: &[NodeReorderItem])`：单事务；folder 项 `UPDATE folders SET parent_id=?, sort_order=? WHERE id=?`，session 项 `UPDATE sessions SET folder_id=?, sort_order=? WHERE id=?`；对 folder 项做轻量防环（父链不得落在自身后代里，遇环报错、整体回滚）。
- `src-tauri/src/commands/session.rs`：新增 `session_reorder(state, items: Vec<NodeReorderItem>)`（校验 kind 合法 / id 非空），转发 config_store。在 `src-tauri/src/lib.rs` 的 `collect_commands!` 注册（bindings.ts 随下次 debug 构建自动重新生成）。
- `src/api/session.ts`：新增 `sessionReorder(items: NodeReorderItem[]): Promise<void>` 封装 + `NodeReorderItem` 类型（镜像 bindings）。`src/stores/session.ts`：`SessionConfig`/`SessionFolder` 补 `sort_order?: number`；新增 `reorder(items)` action（调 api → `load()` 刷新树）。

### A3. 前端拖放（`src/views/WorkspaceView.vue`）
- 行模板（`.workspace__tree-node`）：仅 `isFolder` / 普通会话行加 `draggable`（**db 叶子 / savedConn / 分区头 / 分隔线不参与**）；`@dragstart` 写 `dataTransfer.setData('application/x-fyshell-tree', JSON.stringify({id, kind}))`。
- 容器 `.workspace__tree` 挂 `@dragover / @drop / @dragleave` 统一处理：
  - 计算悬停行 `dropState = { targetId, position }`：落在文件夹行中部 → `'into'`（移入该文件夹末尾）；落在会话行 → 按行上下半区给 `'before'/'after'`；落在空白区 → 目标为所属分区根级。
  - 行加高亮 class（`into` 文件夹高亮 / before-after 顶部或底部指示线），`dragover` 里 `preventDefault()` 放行 drop。
- `@drop`：解析拖源；防环（folder 不能拖进自身或后代）；计算受影响父集合（源父 + 目标父，可能不同）→ 各自重建有序 id 列表（`parent_id` + 1..n 序号）→ `sessionStore.reorder(items)`；失败 toast。
- 排序规则改动（`stores/session.ts` `sortByStar`）：`(星标?0:1) → (sort_order ?? 0) → name`，文件夹按 `(sort_order ?? 0) → name`；**保留现有「每层文件夹恒在会话上方」惯例**（拖拽排序仅在同类内生效，不做跨类交叉混排）。
- 分区不改：拖拽仅作用于各分区可见的会话/文件夹行。

---

## Part B：SFTP 快捷打开

### B1. 菜单顺序（`src/components/common/ToolBar.vue`）
「传文件」菜单把 `快捷双栏` v-list-item 挪到 `新建 SFTP 会话…` 之前（纯模板顺序调整，emit 不变）。

### B2. 终端实时 cwd（OSC 7 追踪）
- `src/composables/useXterm.ts`：`UseXtermOptions` 加 `onCwd?: (path: string) => void`。
  - `write()` 内对解码后的 `decoded` 增量解析 OSC 7：`ESC ] 7 ; <uri> ST|BEL`（ST=`ESC \`）。保留尾部分段缓冲（序列可能跨 chunk 拆开）；解码 `file://<host>/<path>` URI → 绝对路径（`decodeURIComponent`；裸路径也接受）；命中即 `options.onCwd?.(path)`。
  - xterm 会把 OSC 7 序列内部消费（不产生可见字符），旁路解析无副作用。
- `src/stores/terminal.ts`：加 `sessionCwd` 响应式记录（key=connId）；`setSessionCwd(id, path)` / `getSessionCwd(id)`；`cleanupSession` 里删除（断开重连后 shell 重新发 OSC 7 覆盖）。
- `src/components/ssh/terminal/TerminalPane.vue`：`useXterm` 传 `onCwd: (p) => terminalStore.setSessionCwd(props.sessionId, p)`。

### B3. 快捷双栏打开落到该 cwd
- `src/views/WorkspaceView.vue`：
  - 新增 `sftpRemotePaths = ref<Record<string, string>>({})`（按 tab id 记远程栏当前路径）。
  - `openSftpTab()`：创建单例 Tab 前，`boundKey = lastTerminalSession.value?.id`，初始路径 `boundKey ? terminalStore.getSessionCwd(boundKey) : ''`，写入 `sftpRemotePaths[tab.id]`。
  - 模板 DualPane 补 `:remote-path="sftpRemotePaths[tab.id] ?? ''"` + `@update:remotePath="(p) => (sftpRemotePaths[tab.id] = p)"`（导航后继续持有自己的路径，互不干扰）。
  - `closeTab` 时 `delete sftpRemotePaths[tab.id]`。
  - 独立 SFTP 会话 Tab 不设初始路径（沿用 DualPane 内部 `/` 默认）。

---

## 关键文件清单
- 后端：`src-tauri/src/models/session.rs`、`config_store.rs`、`commands/session.rs`、`lib.rs`（注册）；`src/bindings.ts`（构建自动再生）
- 前端：`src/api/session.ts`、`src/stores/session.ts`、`src/views/WorkspaceView.vue`
- SFTP：`src/components/common/ToolBar.vue`、`src/composables/useXterm.ts`、`src/stores/terminal.ts`、`src/components/ssh/terminal/TerminalPane.vue`

## 复用与约定
- 复用 `findNode(nodes, id)`（WorkspaceView 已有）、`sortByStar` 排序骨架、`lastTerminalSession` 绑定会话判定。
- 遵循 serde snake_case 记忆约定；新 IPC 命令在命令参数表记忆里 +1（143）。

## 验证
1. `npm run tauri dev`（debug 构建再生 bindings.ts）编译通过。
2. **树拖拽**：同层上下拖会话/文件夹改序 → 重启应用顺序保持；拖会话/文件夹进另一文件夹 → 树归类正确；拖文件夹进自身/后代被拒绝；db 叶子/分区头不可拖。
3. **菜单**：传文件下拉「快捷双栏」位于首位。
4. **cwd**：SSH 终端配 OSC 7（bash `PROMPT_COMMAND` 或 zsh precmd 输出 `ESC ] 7;file://host$PWD ESC \`），终端 `cd /tmp` → 点传文件 → 快捷双栏 → 远程栏落在 `/tmp`；断开重连后仍能跟随新目录。
5. 端到端可用局域网测试机（192.168.31.100）验证真实 SSH/SFTP 行为。
