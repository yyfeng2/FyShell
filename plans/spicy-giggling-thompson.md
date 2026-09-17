# FyShell 前端 Xshell 风格重设计方案

## Context

用户要求参考 Xshell 对前端整体重新设计：窗口简洁、细腻、功能详细。经确认：

- **范围**：布局 + 视觉都改
- **功能增强**：交互细节补充（tooltip/右键/快捷键提示）+ 树与状态栏信息增强 + 快速命令栏
- **视觉基调**：经典浅灰（Xshell 传统桌面风格：浅灰工具栏带、白色内容区、蓝色主色、高密度布局）

当前骨架已是 Xshell 形态（菜单栏+工具栏+左树+标签+终端+状态栏），本次在其上做经典浅灰视觉重塑、布局细排与功能补全。所有 52 项 UI 缺陷已修复、vue-tsc 全绿，为重设计提供了干净基线。

## 设计方向：经典浅灰 Xshell 风格

- **Chrome 区**（菜单栏/工具栏/标签条/状态栏/左树）：浅灰系 `#F0F2F5` / `#E8EAED` 分层，1px 分隔线，扁平无阴影（悬浮导航除外）
- **内容区**：白色 `#FFFFFF`
- **主色调**：蓝色 `#2E6FDB` 系（Xshell 经典蓝），状态色沿用现有 success/error/warning
- **密度**：行高 24px（树/列表）、菜单栏 26px、工具栏 30px；字号 12px 基线
- **终端区**：保持深色背景（浅灰 chrome + 深色终端是常见组合，可读性最佳）

## 改动清单

### 1. 设计令牌与主题基础

**`src/plugins/vuetify.ts`**
- `defaultTheme: 'dark'` → `'light'`
- 重塑 light 主题为经典浅灰：`background: '#F5F6F7'`、`surface: '#FFFFFF'`、`primary: '#2E6FDB'`、`'surface-variant': '#E8EAED'`，补 `secondary/accent/error/warning/info/success` 浅色系完整色板
- dark 主题 primary 同步换为 `#2E6FDB` 系保持两主题一致

**`src/styles/theme.css`**
- 挂载前防白闪改为浅色默认：`background-color: #f5f6f7; color-scheme: light`，dark 偏好转 media query 分支
- 滚动条浅色系按新令牌调整
- 新增 chrome 区 CSS 变量：`--fy-chrome-bg`、`--fy-chrome-border`、`--fy-row-height-tree: 24px`

### 2. 布局重构（WorkspaceView + 工具栏）

**`src/components/common/ToolBar.vue`** — 重排为 Xshell 经典工具栏：
- 图标按钮分组（新建|连接/断开|传输|视图切换），组间竖分隔线 `1px solid chrome-border`
- 统一按钮规格：`size="small" variant="text"` + tooltip（title 属性）
- 快速连接输入框保留在右端，宽度收窄（180px）

**`src/views/WorkspaceView.vue`**
- 布局细排：chrome 区背景统一浅灰、内容区白色
- 新增快速命令栏挂载位（终端区与状态栏之间）
- 左树行高 24px、节点双行信息（见 §4）

### 3. 新组件：快速命令栏 `src/components/common/QuickCommandBar.vue`

参考 Xshell 快速命令栏：
- 从 `useQuickCommandStore().nodes` 读取**根级命令**，横向渲染为紧凑按钮条（`v-btn size="x-small" variant="tonal"`，带命令名）
- 点击按钮：将 `command_text` 写入当前活动会话（复用 ComposePane 的 sshWrite 模式，写入前补 `\n`），多会话场景取 activeTab 的 sessionId
- 无命令时显示引导文案"在快捷命令中添加常用命令"
- 栏体高 28px、浅灰背景、右上角折叠开关；`ui` store 增加 `quickBarVisible` 状态（默认开），菜单"查看"加切换项
- 按钮超宽 ellipsis、溢出时横向滚动

### 4. 树与状态栏信息增强

**左树（WorkspaceView 内树节点）** — 参考 Xshell 会话树：
- 会话节点双行显示：主行 `名称`（带色图标）、副行 `user@host:port`（11px、on-surface/50%、等宽字体），文件夹保持单行
- 数据源：`SessionNodeLeaf.config`（host/port/username 字段现成）；`FlatNode` 增加 `hostLabel` 字段
- 文件夹副行显示子节点计数（可选，视宽度）
- 行高调整为 30px（双行）+ 保持紧凑

**`src/components/common/StatusBar.vue`** — 分段信息扩展：
- 新增 props：`hostIp?: string`（当前会话 IP）、`encoding?: string`（编码）、`sessionCount?: number`（打开标签数）
- 布局改为 Xshell 式分段：`[状态点+名称] [IP] [编码] [路径] ... [传输] [标签数]`，段间竖分隔线
- 保持 26px 单行紧凑，溢出省略

**WorkspaceView 接线**：从 activeTab 对应会话 config 取 host/encoding 传入 StatusBar

### 5. 交互细节补充

- **全局 tooltip**：所有图标按钮补 `title`（MenuBar/ToolBar/FlexTabs/树/面板已部分有，统一补全）
- **菜单快捷键列**：MenuBar 菜单项加右侧快捷键提示（`Ctrl+T` 等，用 v-list-item action 槽右对齐灰色小字），复制/粘贴等菜单项接到真实 Tauri clipboard API 而非仅提示
- **树右键菜单**：WorkspaceView 会话树补右键菜单（连接/重命名/删除），与双击行为对齐
- **焦点环**：沿用上轮修复的 `:focus-visible` 模式统一应用到新组件

## 实施顺序

1. 主题令牌（vuetify.ts + theme.css）—— 全局基础，先做
2. ToolBar 重排 + WorkspaceView chrome 统一
3. QuickCommandBar 新组件 + ui store + 菜单接线
4. 树双行信息 + StatusBar 扩展
5. 交互细节补全（tooltip/菜单/右键）

## 验证方式

1. `npx vue-tsc --noEmit` 保持全绿
2. `npm run tauri dev` 启动后重点检查：
   - 浅灰主题整体观感（chrome/内容区分层、对比度）
   - 深浅色切换后 chrome 区均正常
   - 快速命令栏：添加命令 → 点击发送 → 终端收到内容
   - 左树双行信息显示（user@host:port）
   - 状态栏各分段信息与溢出行为
   - 终端区仍为深色、无白边
