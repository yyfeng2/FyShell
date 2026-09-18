# 三项任务：深色主题修复 + 移除服务器监控 + 混合彩色图标

## Context

用户新反馈三项（在上一个"移除右键 3 项 + Lucide 迁移"任务两个 commit 完成后）：

1. **深色主题不正常**——根因已定位（见 §2）。
2. **去除服务器监控功能**——需要前后端全链路删除（P2 遗留功能，用户决定去除）。
3. **没有彩色的图标吗**——Lucide 单色线性偏素；经 AskUserQuestion 澄清，用户选定 **混合方案：树彩色 + 按钮语义色**（Navicat 式分类彩树，按钮按语义着色，风格仍统一）。

验证基线：`cargo check` + `npx vue-tsc --noEmit` + `npx vite build` 三绿；dev 冒烟。

---

## 1. 深色主题修复（theme.css 根因，纯 CSS）

**根因 A**：`src/styles/theme.css:21-22` 的 `--fy-chrome-bg: #f0f2f5` / `--fy-chrome-border: #d5d9de` 定义于 `:root`，**无深色覆盖**。而 9+ 处 chrome 区组件全部引用该变量：MenuBar.vue:123/122、ToolBar.vue:77/76、StatusBar.vue:147/148、QuickCommandBar.vue:88/87、FlexTabs.vue:516/517、WorkspaceView.vue:1688/1687 与 1851/1852、SettingsDialog.vue:540/606、SshOptionsDialog.vue:239/335 → **深色主题下这些区域仍是浅灰白块**（深色主题浅色文字 + 浅灰底，观感破碎）。

**根因 B**：theme.css:22-23 `color-scheme: light` 固定在 body；手动切深色时 Vuetify 未同步原生控件配色（滚动条/输入件/选中仍是浅色），仅系统偏好 dark 的 `@media` 生效。

**修复**（`theme.css` 文件内追加；Vuetify 3 把 `v-theme--dark` 类挂在 **html** 元素，选择器可靠）：
```css
/* 深色主题：chrome 区随主题切换（覆盖 :root 浅灰变量）+ 原生控件配色同步 */
html.v-theme--dark {
  --fy-chrome-bg: #1f2224;
  --fy-chrome-border: #34383c;
  background-color: #1a1c1e;
  color-scheme: dark;
}
```
同时抽查并修正 1-2 处可能残留的硬编码浅色（若 grep 到 `#fff`/`#f5f6f7`/`#f0f2f5` 非主题语义处）——保持范围克制，以 chrome 变量 + color-scheme 为根因修复。

## 2. 移除服务器监控功能（前后端全链路）

### 前端（删除）
1. `src/views/WorkspaceView.vue`：import MonitorDrawer/MonitorMiniBar（29/30 行）、`showMonitor` ref（1079）、`case 'monitor'`（1189-1191）、模板 MonitorMiniBar(1561-1566) 与 MonitorDrawer(1582-1587)。
2. `src/components/common/MenuBar.vue:57`「服务器监控」菜单项删除。
3. `src/components/common/KeyMappingDialog.vue:129` 键位映射可执行动作列表 `{ title: '服务器监控', value: 'monitor' }` 删除（键位映射 store 无需动，仅动作列表项）。
4. 删除整目录 `src/components/ssh/monitor/`（MonitorDrawer.vue 681 行 + MonitorMiniBar.vue 180 行）。
5. 删除 `src/stores/monitor.ts`（262 行）。
6. 删除 `src/api/monitor.ts`；`src/api/types.ts` 删除 `DockerContainer`（24 行 re-export 与 73 行 `MonitorSample` 接口块）——先 grep 确认无其它 import（已知仅 monitor store/组件引用）。
7. `src/bindings.ts`（specta 生成物）：`monitorStart/monitorStop/dockerList/dockerOperate`（78-84）+ `DockerContainer`（421）+ `MonitorSample`（455）——**由 dev 重启时 specta 自动重生成**（当前 dev 在跑，改后端后 tauri watch 会 rebuild 并重写 bindings.ts）；若未自动更新则手动删对应条目（既有经验：Windows 上 cargo test 崩 0xc0000139，以 dev 自动生成为准）。

### 后端（删除）
8. 删除 `src-tauri/src/commands/monitor.rs`、`src-tauri/src/services/monitor.rs`。
9. `src-tauri/src/commands/mod.rs:9` 与 `src-tauri/src/services/mod.rs:11` 的 `pub mod monitor;` 删除。
10. `src-tauri/src/lib.rs`：`collect_commands!`（52-56）与 `generate_handler!`（247-249）的 4 条 monitor/docker 命令登记删除。
11. **死代码连带**：`src-tauri/src/services/ssh.rs:92 SessionHandle::is_alive` 的唯一使用方是 monitor.rs（134/474）→ 删 monitor 后整函数一起删除（此前 ssh_alive 命令删除时特意保留给 monitor 用，本次一并清理）。

## 3. 混合彩色图标

Lucide 保留线性描边（currentColor），通过 **Vuetify 语义色**（primary/success/warning/error/info/accent + 调色板 amber 等，深色主题自动适配）给图标上色；不改图标库、不加依赖。

### 3a. 会话树类型彩色（WorkspaceView 树图标 1445-1455 分支）
给树图标 switch 的 `v-icon` 分支加 `:color`：
- SSH（mdi-console）→ `success`（绿）
- MySQL（mdi-database）→ `primary`（蓝）
- Telnet（mdi-console-network）→ `warning`（橙）
- RLOGIN（mdi-send）→ `accent`（紫）
- 串口（mdi-usb-port）→ `info`（青蓝）
- 文件夹（mdi-folder/folder-open）→ `amber`（树内文件夹用琥珀，区分层次）
- 库叶子（mdi-database-outline）与已保存连接节点按数据库蓝
- 数据库服务分区（section-db 图标）保持 primary
实现：在 FlatNode 上已有 `sessionType` 字段，加 computed color 映射（如 `treeIconOf(node)` 返回 `{ icon, color }`），替换现有模板三元链。

### 3b. 数据库对象树彩色（MysqlDbWorkspace 对象 tab，765-792 的 kind→icon 映射）
现有 `DB_OBJECT_KINDS` / kind→icon 仅在 icon 名层面。新增一张 `kind→color` 表（Navicat 分类色）：
- 表 table → `primary`（蓝）
- 视图 view → `success`（绿）
- 函数 function → `warning`（橙）
- 过程 procedure → `accent`（紫）
- 触发器 triggger → `error`（红）
- 事件 event → `info`（蓝青）
- 用户 user → `teal`/`cyan`（或 info 区隔）
对象树图标渲染处按 kind 绑 color（实现时定位对象树 `<v-icon>` 行并接入色表；icon 名与 color 可由同一 computed 返回）。

### 3c. 按钮语义色（高价值操作/状态，抽样原则——不逐处全覆盖）
给下列**关键/状态性**图标补 Vuetify 语义色（`color="…"` 或 `text-…` 类），操作主色用 primary、成功用 success、危险用 error、编辑用 primary：
- 成功/校验/完成：`mdi-check-circle` → success；`mdi-check`（保存成功类）→ success；`mdi-check-all` → success。
- 失败/危险/断开：`mdi-alert-circle`/`mdi-close-circle` → error；删除（Trash2）与断开（lan-disconnect）→ error（若出现处强调）。
- 信息提示：`mdi-information-outline`/`mdi-help-circle` → info。
- 收藏/标记：`mdi-star` → amber；`mdi-pin`/`pin-off`（固定标签）→ warning。
- 连接/在线状态点/运行执行：`mdi-play`（执行）→ success；连接成功状态 → success 绿点。
- 编辑 pencil/square-edit → primary（统一动作色）。
- 新增 plus/database-plus/account-plus → primary（与操作按钮 color 一致时可保留按钮自带色，避免叠色；以实际按钮有无 color 为准）。
代表文件：MysqlDataGrid.vue（保存/删除/执行/刷新）、MysqlDbWorkspace.vue（表操作/导入导出）、SessionForm/ByteStreamPanel（测试连接状态）、SFTP FilePane/DualPane（上传/下载/删除/对称）、FlexTabs（固定/关闭）、MonitorDrawer 随监控移除不涉及。
★ 实现纪律：**只加语义色，不硬编码 hex**；与按钮自带 `color="primary"`（VBtn）的场合以按钮色为准不加重复；深色/浅色都由 Vuetify 语义色自适应。

---

## 验证

1. `npx vue-tsc --noEmit` 绿；`npx vite build` 绿（模板结构完整性）。
2. `cargo check` 绿（监控命令/is_alive 删除后 Rust 零警告）。
3. `npm run tauri dev` 冒烟（当前 dev 窗口正在运行，改后端后 tauri watch 自动 rebuild + 自动重生成 bindings.ts）：
   - 切「深色主题」：菜单栏/工具栏/标签栏/状态栏/会话树/快速命令栏均转深灰，滚动条/输入件随 dark；切回浅色正常。
   - 监控：菜单栏「服务器监控」项消失、快捷命令/键位映射动作列表无 monitor、界面无监控迷你条/抽屉，dev console 无报错。
   - 彩色：会话树类型图标（SSH 绿/MySQL 蓝/Telnet 橙/RLOGIN 紫/串口青）与文件夹琥珀；MySQL 对象树（表蓝/视图绿/函数橙/过程紫/触发器红）；按钮成功绿/错误红/信息蓝；深色下对比正常。
   - 回归：右键菜单 4 项、图标映射（mdi 名→Lucide）总体不回归。

## 提交策略
按依赖分 3 个 commit：
1. `深色主题修复: chrome 区变量随主题 + color-scheme 同步`（theme.css）
2. `移除服务器监控功能（前后端全链路 + is_alive 死代码）`（前端组件/store/api/types + 后端命令/services/mod.rs/lib.rs/ssh.rs + bindings 由 dev 重生成）
3. `图标混合彩色: 会话树分类色 + 数据库对象分类色 + 按钮语义色`（WorkspaceView/MysqlDbWorkspace/各高价值组件）

## 遗留（维持现状）
updater endpoint 渠道决策（用户侧）；vault 单测待可跑环境（Windows 0xc0000139）。
