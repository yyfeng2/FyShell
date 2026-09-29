# 数据库右键菜单缺失功能开发计划

## Context

数据库树右键菜单（Navicat 12 项，提交 90de32a）中有 4 项置灰标「开发中」：**编辑数据库 / 打印数据库 / 逆向数据库到模型 / 在数据库中查找**。用户要求开始开发这 4 个功能。已确认方案：ER 图用 **Mermaid.js**（erDiagram 语法，新增依赖约 1MB），打印内容为**结构报告**（每表小节：表名/行数/引擎/注释 + 全部列）。

4 个功能的定位（对照 Navicat）：
| 菜单项 | 功能 | 数据来源 |
|---|---|---|
| 编辑数据库... | 查看并修改库默认字符集/排序规则（ALTER DATABASE） | information_schema.SCHEMATA |
| 打印数据库 | 全库结构报告 → 浏览器打印 | mysqlListTables + information_schema.COLUMNS |
| 逆向数据库到模型... | ER 图（实体框+属性+FK 关系连线） | information_schema.COLUMNS + KEY_COLUMN_USAGE |
| 在数据库中查找 | 全库表数据按关键字 LIKE 搜索 | 新后端命令 mysql_db_find |

关键约束：**右键的库可能与连接默认库不同**，所有查询必须用 `db`.`table` 限定 + `TABLE_SCHEMA = 'name'` 过滤，不依赖连接当前库上下文。新 IPC 模型字段一律 snake_case；新命令须在 lib.rs `collect_commands!` 与 `generate_handler!` 两处登记（lib.rs:18 与 :320）。

## 实现步骤

### 1. 后端：2 个新命令（models + services + commands + 登记）

**`mysql_db_edit`** — models/mysql_db.rs 无需新模型（返回 ()）：
- `services/mysql_db.rs` 加 `edit_database(conn_id, name, charset, collation: Option<&str>)`：校验标识符（复用 `is_valid_identifier`，charset/collation 额外限 alnum+下划线），`ALTER DATABASE \`db\` DEFAULT CHARACTER SET = {charset} COLLATE = {collation}`（collation None 时只设 charset），`take_tx_or_pool` 模式取连接
- `commands/mysql_db.rs` 加薄层 `#[tauri::command] #[specta::specta]`

**`mysql_db_find`** — models/mysql_db.rs 加 `MySqlDbFindHit { table: String, columns: Vec<String>, rows: Vec<Vec<Option<String>>> }`（snake_case）：
- `services/mysql_db.rs` 加 `find_in_database(conn_id, name, keyword, max_per_table: Option<u32>) -> Vec<MySqlDbFindHit>`：
  1. 字符串列元数据：`SELECT TABLE_NAME, COLUMN_NAME FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = 'name' AND DATA_TYPE IN ('char','varchar','text','tinytext','mediumtext','longtext','enum','set')`（库名字符串字面量单引号翻倍转义）
  2. 每表（含 ≥1 字符串列）：`SELECT * FROM \`db\`.\`t\` WHERE \`col1\` LIKE '%kw%' OR ... LIMIT n`（LIKE 的 `\` `%` `_` 转义；默认每表限 20 行）
  3. 只收集有命中的表（columns=全列名，rows=命中行）
- `commands/mysql_db.rs` 加薄层（keyword 非空校验）

**登记**：lib.rs 两处列表各加 `commands::mysql_db::mysql_db_edit,` / `commands::mysql_db::mysql_db_find,` → `cargo check` 验证

### 2. 前端：依赖 + api 封装

- `npm install mermaid`
- `src/api/mysql.ts` 加 `mysqlDbEdit(connId, name, charset, collation?)` 与 `mysqlDbFind(connId, name, keyword, maxPerTable)`（invoke 字符串命令名，不依赖 bindings.ts 重生成）
- `src/api/types.ts` 加 `MySqlDbFindHit` 类型

### 3. 编辑数据库 — `src/components/mysql/EditDatabaseDialog.vue`（新建）

参考 ExplainPanel.vue 对话框模式（v-dialog :model-value + v-card）：
- props：`modelValue` / `connId` / `dbName`；打开时加载当前值（`mysqlQuery` 查 `SELECT DEFAULT_CHARACTER_SET_NAME, DEFAULT_COLLATION_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = 'name'`）
- 字符集下拉：`SELECT DISTINCT CHARACTER_SET_NAME FROM information_schema.COLLATIONS`；排序规则下拉随字符集联动（`SELECT COLLATION_NAME FROM information_schema.COLLATIONS WHERE CHARACTER_SET_NAME = ?`）
- 保存 → `mysqlDbEdit` → toast → emit close

### 4. 打印数据库 — WorkspaceView 内联函数（无对话框）

`menuPrintDb()`：`mysqlListTables(connId)` + `mysqlQuery` 查全库列元数据（COLUMNS 按 TABLE_NAME, ORDINAL_POSITION 排序）→ 拼 HTML（标题=数据库+生成时间；每表小节=表名/行数/引擎/注释 + 列表格）→ 隐藏 iframe `contentDocument.write` + `contentWindow.print()`（WebView2 内打印，不弹新窗口）

### 5. 逆向到模型 — `src/components/mysql/ErModelDialog.vue`（新建）

- props 同上；`mysqlQuery` 查列（TABLE_NAME/COLUMN_NAME/COLUMN_TYPE/COLUMN_KEY）+ FK（`SELECT TABLE_NAME, COLUMN_NAME, REFERENCED_TABLE_NAME, REFERENCED_COLUMN_NAME FROM information_schema.KEY_COLUMN_USAGE WHERE TABLE_SCHEMA = 'name' AND REFERENCED_TABLE_NAME IS NOT NULL`）
- 拼 mermaid erDiagram 定义：每实体 `{ type name PK/FK ... }`；关系 `父表 }o--|| 子表 : "外键列"`
- `mermaid.initialize({ startOnLoad: false, theme: 'neutral' })` + `mermaid.render()` → SVG innerHTML 渲染（theme 暂用 neutral 固定，主题跟随后续再说）
- v-dialog 全屏化（width="95%" scrollable）

### 6. 在数据库中查找 — `src/components/mysql/FindInDbDialog.vue`（新建）

- props 同上；关键字输入 + 查找按钮 → `mysqlDbFind(connId, dbName, keyword, 20)` → 按表分组展示命中行（表名小节 + 列表），空结果提示
- loading/error 状态对齐 ExplainPanel（v-progress-circular / v-alert）

### 7. WorkspaceView 接线（[WorkspaceView.vue](e:\fyshell\src\views\WorkspaceView.vue)）

- 4 个置灰项（行 2491/2526/2529/2532）启用：去掉 disabled 与 `title="开发中"`，接 `@click` → 新函数 `menuEditDb/menuPrintDb/menuErModel/menuFindInDb`（模式对齐 menuRunSqlFile：取 `useMysqlStore().connId` 守卫 + 打开对话框；dbName 取 `treeMenu.node.dbName`）
- 新 refs：`showEditDbDialog/showErModelDialog/showFindDialog/findDbName`
- 模板末尾（行 2794 ImportExportDialog 旁）挂载 3 个新对话框组件

### 8. 验证

- `cargo check` + `npx vue-tsc --noEmit` 双绿
- CDP（9222）：右键菜单 4 项已启用、对话框正常打开渲染
- 测试服务器 192.168.31.100（凭据见记忆）真实连接 e2e：打印/ER 图/查找直接验证；编辑数据库可改 charset（测试库可改，改回即可）

## 关键文件

| 文件 | 动作 |
|---|---|
| src-tauri/src/models/mysql_db.rs | 加 MySqlDbFindHit |
| src-tauri/src/services/mysql_db.rs | 加 edit_database / find_in_database |
| src-tauri/src/commands/mysql_db.rs | 加 2 个薄层命令 |
| src-tauri/src/lib.rs | 两处列表登记（:18/:320） |
| src/api/mysql.ts、src/api/types.ts | mysqlDbEdit/mysqlDbFind + 类型 |
| src/components/mysql/EditDatabaseDialog.vue | 新建 |
| src/components/mysql/ErModelDialog.vue | 新建 |
| src/components/mysql/FindInDbDialog.vue | 新建 |
| src/views/WorkspaceView.vue | 4 项启用 + 接线 + 挂载 |

## 复用的现有工具

- `services/mysql_db.rs` 的 `take_tx_or_pool` / `is_valid_identifier` / `mysql_err`（连接与校验基础）
- `mysqlQuery`（通用 SELECT，information_schema 查询免新增命令）
- `ExplainPanel.vue` 对话框状态模式、`menuRunSqlFile` 接线模式
