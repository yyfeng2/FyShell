# 新任务：移除右键菜单 3 项 + 全项目图标统一替换为 Lucide

## Context

上一任务（Telnet/RLOGIN/串口完整会话化）已完成并提交（2aa7877 + bindings 同步 15ff637），工作区干净。本任务两个子任务（用户原话 + AskUserQuestion 澄清）：

1. **移除会话树右键菜单 3 项**：新建本地终端 / Telnet 连接 / 串口终端（用户截图中红框精确框选的三项）。
2. **全项目图标统一替换**：用户认为当前 mdi 图标"太单调、太粗糙、太丑"，选定更换为 **Lucide** 风格的图标。

### 关键技术事实（已核实）

- 右键三项入口只在 `WorkspaceView.vue` 右键菜单里（`menuNewLocal` / `menuNewByteStream('telnet')` / `menuNewByteStream('serial')` 均仅该菜单调用），**无菜单栏/工具栏/其它入口**。因此移除菜单后相关函数/组件/表单全部成为死代码。
- `tsconfig noUnusedLocals: false` → 删模板调用后残留未用函数**不会**被 vue-tsc 报错，但按项目惯例（健康检查清死代码）应一并清理。
- Vuetify 3.13.4 图标机制（`node_modules/vuetify/lib/composables/icons.js` + `VIcon.js` 已读源码确认）：
  - `createVuetify({ icons: { defaultSet, sets, aliases } })`，`IconSet = { component }`。
  - `icon` 以 `$` 开头 → 替换为 `aliases[name]`；否则按 `setName:` 前缀分派；**无前缀 → 走 `defaultSet` 的 set**，组件收到**移除前缀后的 icon 字符串**（如显式 `icon="mdi-plus"` 收到 `mdi-plus`，内部 `$close` 经 aliases 解析后收到别名值）。
  - `VIcon` 渲染 `createVNode(set.component, { tag, icon, class: 'v-icon …', style: {fontSize/height/width, color} }, …)` → **set 组件只需渲染 `h(tag, {class, style}, [svg])`，继承 `currentColor` 即可无缝接管尺寸/颜色**。
- 全项目无 `class="mdi"` 直接用法、无 `mdi-spin` 动画；`@mdi/font` 仅 `plugins/vuetify.ts:14` 一处 CSS import。

---

## 任务一：移除右键菜单 3 项 + 清理死代码

全部改动集中在 `E:\fyshell\src\views\WorkspaceView.vue` 与一个组件文件：

1. **模板右键菜单**（约 1542-1576 区域的 treeMenu v-list）：
   - 删除注释 `<!-- 新建连接：本地终端 / Telnet / 串口（byte-stream 终端） -->` 与 3 个 `<v-list-item>`（新建本地终端 / Telnet 连接 / 串口终端），并删除因此多出的**一个** `<v-divider>`（删除项前后的两个 divider 保留其一）。
   - 最终菜单：连接 → 设置 → divider → 重命名 → divider → 删除。
2. **删除失去唯一入口的脚本死代码**：
   - `import ByteStreamForm from '@/components/ssh/session/ByteStreamForm.vue'`（24 行）。
   - `openLocalTerminal()`（767）、`openByteStreamTerminal()`（796）、`menuNewLocal()`（889）、`menuNewByteStream()`（895）。
   - `const showByteStreamForm = ref(false)`（1099）。
   - 模板挂载块（约 1675-1678，`<ByteStreamForm v-model="showByteStreamForm" @saved="openByteStreamTerminal" />`）。
3. **删除组件文件** `E:\fyshell\src\components\ssh\session\ByteStreamForm.vue`（不再被引用）。
4. **注释修正**：`src/components/ssh/options/panels/ByteStreamPanel.vue:100` 注释提到"与 ByteStreamForm 一致"→ 改为"常用波特率选项"（仅措辞）。

**明确保留**（byte-stream 持久会话能力不受影响）：`openByteStreamSession()`（持久会话路由）、WorkspaceView 会话树 telnet/rlogin/serial 图标与双击路由、SshOptionsDialog 的 ByteStreamPanel 会话级编辑（记录编辑 SessionConfig，非即时连接）、SessionForm 的五类会话新建。

---

## 任务二：mdi → Lucide 图标全量替换

**方案（已定，零组件改动）**：Vuetify 自定义 SVG 图标集接入 `lucide-vue-next`，所有现有 `icon="mdi-x"` / `prepend-icon` / `:icon` / `icon` 对象字段与**内部别名**（`$close`/`$prev`/`$next`/`$checkboxOn` 等 45 键）统一经一张映射表落到 Lucide 组件。现有 138 个唯一 mdi 图标名一个不落全部映射。

### 2a. 安装依赖
```bash
npm i lucide-vue-next
npm un @mdi/font
```

### 2b. 新建 `src/plugins/icons/lucideMap.ts`
- **具名 import** 全部映射用到的 lucide 组件（保持 tree-shaking 与体型最小）：
  ```ts
  import { Plus, Search, Trash2, X, RefreshCw, Check, Pencil, Database, …, CircleHelp } from 'lucide-vue-next'
  import type { Component } from 'vue'
  import type { IconSet, IconAliases } from 'vuetify'
  ```
- **`mdiMap: Record<string, Component>`**：key = mdi 图标名**去 `mdi-` 前缀后的小写名**（同义映射，如 `'magnify': Search`、`'delete': Trash2`、`'close': X`、`'console': Terminal`、`'console-network': SquareTerminal`、`'usb-port': Usb`、`'send': Send`、`'database': Database`……）。完整清单实现第一步由
  `grep -rhoE "mdi-[a-zA-Z0-9-]+" src --include=*.vue --include=*.ts | sort -u`
  导出，逐条对照 lucide 官方图标名（lucide.dev）映射；**无直接等价的用语义近义词**（如 `swap-vertical→ArrowUpDown`），极少数（预计 <10）无法近义的映射到 `CircleHelp` 并在交付时向用户列出例外。
- **导出 `aliases: IconAliases`**：覆盖 Vuetify 全部 45 个内部别名键，值直接用 **lucide PascalCase 字符串名**（如 `close:'X'`、`prev:'ChevronLeft'`、`next:'ChevronRight'`、`checkboxOn:'SquareCheckBig'`、`checkboxOff:'Square'`、`checkboxIndeterminate:'Minus'`、`dropdown:'ChevronDown'`、`loading:'LoaderCircle'`、`ratingFull:'Star'`、`ratingHalf:'StarHalf'`、`first:'ChevronsLeft'`、`last:'ChevronsRight'` 等）。
- **导出 `lucideIconSet: IconSet`**：`component` 为下面的 `LucideIcon`；同时导出一张 pascal 名→组件表（`lucideByName`，由具名 import 手工构建）。

### 2c. 新建 `src/plugins/icons/LucideIcon.ts`（渲染组件）
```ts
export const LucideIcon = defineComponent({
  name: 'LucideIcon',
  props: { tag: [String, Object, Function], icon: [String, Array, Object, Function] },
  setup(props) {
    return () => {
      const n = typeof props.icon === 'string' ? props.icon.replace(/^mdi-/, '') : ''
      const Icon = lucideByName[n] ?? mdiMap[n.toLowerCase()] ?? CircleHelp
      return h(props.tag, null, [h(Icon, { size: '1em', 'aria-hidden': 'true' })])
    }
  },
})
```
关键点：`inheritAttrs` 默认 true，VIcon 传入的 `class="v-icon …"`/style(尺寸+颜色) 自动落到 `<i>` 根元素；svg `size="1em"` 随 font-size 缩放、`currentColor` 继承颜色 → 现有尺寸/颜色用法全部保持。

### 2d. 改 `src/plugins/vuetify.ts`
- 删除 `import '@mdi/font/css/materialdesignicons.css'`（14 行）。
- 引入 `lucideIconSet` / `aliases`，`icons: { defaultSet: 'lucide', sets: { lucide: lucideIconSet }, aliases }`。

### 2e. 需人工校对的关键映射抽样（确保视觉还原度）
删除/关闭、放大镜搜索、加号、刷新、勾选、铅笔编辑、数据库、眼睛、切换上下、复制粘贴、文件夹/文件、齿轮设置、菜单、chevron 方向、表格/网格、终端控制台、锁定/钥匙、主题日/夜 等高频与语义图标——实现时在映射表注释中标注对应 mdi 原名以便复核。

---

## 验证

1. `npx vue-tsc --noEmit` 绿。
2. `npx vite build` 绿（模板结构完整性，本项目历史教训：结构性错误 vue-tsc 可能漏）。
3. `cargo check` 绿（后端零改动，仅确认未受影响）。
4. `npm run tauri dev` 手工：
   - 会话树右键菜单只剩 连接/设置/重命名/删除 4 项（+ divider）。
   - 全局界面无图标缺失/空框/异常：菜单栏、会话树、工具栏、MySQL 工作台、数据表操作、分页/下拉/复选框/单选框/评分、对话框关闭按钮、SFTP 双栏、FEXtabs、ByteStreamPanel 等；Vite/控制台无 `Could not find aliased icon` 警告。
   - 回归：连接 SSH/MySQL 会话正常、持久 Telnet 会话双击可开、互联 Tab、右键设置（SshOptionsDialog）仍可用；ByteStream 即时连接入口已按预期消失（仅右键过）。

## 提交策略

两个独立 commit（先任务一后任务二，本地身份沿用 yyfeng14）：
1. `移除会话树右键菜单"新建本地终端/Telnet/串口"入口并清理死代码`
2. `图标全量迁移 mdi → Lucide（Vuetify 自定义 SVG 图标集）`

## 遗留（保持现状，不属本任务）
- updater endpoint 占位渠道决策（用户侧）。
- vault 单测待可跑环境（Windows 0xc0000139）。
