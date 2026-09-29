# FyShell 二期 · 第二战区「MySQL 工作区」实施计划

## Context（为什么做这个）

一期与骨架战区（批 1-4 已交付：21377dc/7f5ab04/cc7c27c/075f9c9）之后，MySQL 工作区（19 组件 / 10629 行）进入二期打磨范围。

**关键勘探结论（本计划的立足点）**：MySQL 模块在一期里已被打磨到行业级成熟，**不存在"简陋粗糙"**——
- 数据网格已 Excel 式改造（冻结行号 gutter + 锁定列表格、NULL 灰斜体、双击编辑 → 预览 → 提交管道、Shift 矩形选区、危险 SQL 二次确认、服务端分页、查询错误/空白三态 v-alert）——**绝不重做**。
- 查询 Tab 多结果集逐条展示、选段运行、危险确认、保存查询 + 历史抽屉、Explain、代码段；CLI 控制台 ASCII 表格 + 历史 + use 切库；工具对话框全部是规范 title+divider+× + `.fy-field-row` + compact density——全部成熟。
- 错误体验：`friendlyError` 已在全部 19 组件 catch 全覆盖，`ui.confirm` 危险项红确认、`ui.toast` 折叠详情均已接入。

**因此本战区定位 = 精准抛光两批，不为改而改**（避免引入回归、违背"不推翻既有参数"）。真实差距只有两类：
1. **6 处"单行灰字空态"**仍是骨架战区 EmptyState 升级前的旧样式——这是用户最常驻空窗的观感短板。
2. 少量交互微缺口（空 SQL 点运行无反馈、结果集 note 无语义色）与一致性抽查。

## 战区边界

只动 4 个组件模板/样式 + 零星微逻辑：`MysqlDataGrid.vue`、`MysqlQueryTab.vue`、`MysqlDbWorkspace.vue`、`HistoryDrawer.vue`。
**不动**：store（mysql.ts）、后端命令、编辑管道核心、网格单元格逻辑、5 工具对话框（已规范）。

全部新样式复用既定令牌（`--fy-*` / `rgba(var(--v-theme-*), A)` comma 形式）、复用 [EmptyState.vue](e:\fyshell\src\components\common\EmptyState.vue)；弹层内动态值禁 CSS `v-bind` 用 inline `:style`。

---

## 批 1：查询流空窗统一（EmptyState 升级，感知最直接）

**文件**：MysqlDataGrid / MysqlQueryTab / MysqlDbWorkspace / HistoryDrawer

| 位置 | 现状（单行灰字） | 改为 |
|---|---|---|
| MysqlDataGrid 366-368（主空窗） | `在左侧选择表，或在上方输入 SQL 执行` | EmptyState：`mdi-table-search` + 标题「选择表或运行查询」+ desc「在左侧选择表查看数据，或在上方输入 SQL 执行」 |
| MysqlDataGrid 357-361（空结果集） | `空结果集` | 轻量居中：`mdi-empty` + 「查询成功但无匹配行」+ 「该查询未返回任何数据」 |
| MysqlQueryTab 119-121（结果空态） | `运行 SQL 后在此显示结果` | EmptyState：`mdi-sql-query`（或 mdi-play）+ 「运行 SQL 查看结果」+ desc「输入语句后按 Ctrl+Enter 执行，多语句逐条展示」 |
| MysqlDbWorkspace 176-180（连接空态） | `暂无已保存的连接` | EmptyState 轻量版：`mdi-database-off` + 「暂无可连接数据库」+ desc+ 保留下方「新建连接」按钮动作 |
| MysqlDbWorkspace 412-418（权限空态 ×2） | `暂无权限记录` / `在左侧选择用户查看权限` | 未选择：`mdi-account-key` + 「在左侧选择用户查看权限」；已选无记录：`mdi-shield-alert` + 「暂无权限记录（或无权限查看）」 |
| HistoryDrawer 71-75 & 117-121（抽屉空态） | `暂无查询历史` / `无匹配记录` / `暂无已保存查询` | 抽屉内轻量居中 EmptyState（窄容器自适应，`density` 收紧）：`mdi-history` / `mdi-magnify` / `mdi-bookmark-outline`，附 desc |

约束：EmptyState 是 `max-width:360 + padding:40px` 的居中大版，**抽屉/小容器内需size 收窄**——EmptyState 提一个 `size` prop（`normal`/`compact`，compact 时 icon 24、padding 24、行距紧凑）或在抽屉内用纯 CSS 精调，不一刀切。空结果集 `td` 内直接放紧凑排版即可。

**验证**：CDP（9222）驱动——未连接态开工作台 Tab（连接空态）、grid 无结果（placeholder、执行 `SELECT * FROM a WHERE 1=0` 的空结果集）、查询 Tab 未运行（结果空态）+ 历史抽屉三态、权限 Tab 两态；GLM-vision 截图核对深浅双主题。

---

## 批 2：交互微收口 + 一致性扫描 + 冒烟

**1. 空 SQL 运行无反馈（MysqlQueryTab 351 `if (!text.trim()) return`）**
点「运行」而 SQL 为空时静默——给 `runAll` 增加 `if (!sql.value.trim()) { ui.toast('请输入 SQL 后再运行', 'warning'); return }`，或禁用运行按钮（有选中分支需保活按钮）；二选一取 toast 方案（低侵入）。

**2. 查询 Tab 结果集 note 语义化（MysqlQueryTab 100-103）**
`res.note` 现已含「N 行（LIMIT 100）」/「Query OK，N 行受影响」/「已取消」。在结果 label 前缀语义色点：SELECT=primary、写操作=success、已取消=warning——text 不变，仅视觉效果（3 行 CSS + 一个 `noteKind(res)` 小函数，按 `note.startsWith('Query OK') / '已取消'` 判类）。低风险、直提查询反馈质感。

**3. loading 抽查（只查不重做）**
网格 `gridLoading`、对象列表 `objectsLoading`、权限「加载权限中…」（407）三处现用法是否一致（v-progress-linear 顶部薄条 vs v-btn loading vs 文本）——若明显不一致，统一为「容器顶部 2px v-progress-linear + 局部按钮 loading」；若可接受则不动（记录说明即可）。

**4. 硬编码色抽查（mysql 组件内 grep `#hex`，除已知 scrollbar #c6c9ce 外）**
命中即改 `rgba(var(--v-theme-*), A)` + 补 `.v-application.v-theme--dark` 覆盖；预期为 0 或极少。深色双态截图全检（复用骨架批 4 归档目录 `fulltest-v2/`）。

**5. 构建 + 真实冒烟**：`vue-tsc` + `vite build`；`npm run tauri dev` 连测试机 192.168.31.100（SSH→MySQL 真连）走：开表 → 网格编辑提交 → 查询 Tab 跑 SELECT/多语句/危险确认 → CLI 控制台 ASCII 表 + use 切库 → 历史抽屉召回 → 空窗双态截图归档。

## 交付节奏

1. 每批 `git add -A && git commit`（单批一提交，信息含「二期 MySQL 战区批N」+ 改动摘要）。
2. 每批 CDP 截图（.impeccable/fulltest-v2/）+ GLM-vision 验证 + 你确认后再进下一批。
3. 批 2 结束连测试机冒烟，归档全部截图。

## 风险与对策

- 改动面小且全部在模板/样式层，不动 store/后端/编辑管道 → 回归面极小。
- 不推翻既有参数：全部新增走新版 EmptyState 与 --fy-* 令牌，不覆盖 component 既有色值。
- 抽屉/网格内窄容器：EmptyState 尺寸自适应（size prop），避免 360px 大版在 280px 抽屉内撑爆。
- HistoryDrawer 仅改空态块，不动列表/搜索/重命名逻辑。

## 关键文件

- [src/components/mysql/MysqlDataGrid.vue](e:\fyshell\src\components\mysql\MysqlDataGrid.vue)（placeholder 366 / 空结果 357）
- [src/components/mysql/MysqlQueryTab.vue](e:\fyshell\src\components\mysql\MysqlQueryTab.vue)（结果空态 119 / note 100 / run 351）
- [src/components/mysql/MysqlDbWorkspace.vue](e:\fyshell\src\components\mysql\MysqlDbWorkspace.vue)（连接空态 176 / 权限 412-418）
- [src/components/mysql/HistoryDrawer.vue](e:\fyshell\src\components\mysql\HistoryDrawer.vue)（空态 71-75 / 117-121）
- 复用：[src/components/common/EmptyState.vue](e:\fyshell\src\components\common\EmptyState.vue)、[src/stores/ui.ts](e:\fyshell\src\stores\ui.ts) toast/confirm
