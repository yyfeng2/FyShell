# 工具栏数据库工具 4 项：数据传输/数据生成/数据同步/结构同步

## Context

用户要求在工具栏增加 Navicat 风格的 4 个数据库工具（截图：数据传输.../数据生成.../数据同步.../结构同步...），已确认：
- **入口形态**：4 个独立按钮（图标+底部汉字，toolbar__titled 同款），不是下拉菜单
- **显示条件**：MySQL 已连接时显示（`mysqlStore.isConnected`，与激活 Tab 无关），断开后隐藏
- **实现范围**：全部 4 个完整实现（含跨连接目标），不是置灰占位

现状：工具栏 [ToolBar.vue](e:/fyshell/src/components/common/ToolBar.vue) 无 MySQL 感知；对话框接线参考 WorkspaceView 中的 ImportExportDialog 模式；后端连接注册表支持多连接共存（HashMap<String, Pool>），跨连接操作可行。

## 前端改动

### 1. ToolBar.vue（src/components/common/ToolBar.vue）

- 新增 prop `dbTools?: boolean`（MySQL 已连接时为 true）
- 新增 4 个 emit：`db-transfer` / `db-generate` / `db-sync` / `db-structure-sync`
- 「传文件」按钮后插入 `v-divider` + 4 个按钮（`v-if="props.dbTools"`），toolbar__titled 图标+底部汉字结构：

| 按钮 | 图标 | title |
|---|---|---|
| 数据传输 | mdi-database-arrow-right-outline | 数据传输：跨库复制表与数据 |
| 数据生成 | mdi-dice-multiple-outline | 数据生成：批量生成测试数据 |
| 数据同步 | mdi-database-refresh-outline | 数据同步：按主键比对并同步 |
| 结构同步 | mdi-compare-horizontal | 结构同步：比对并同步表结构 |

### 2. WorkspaceView.vue（src/views/WorkspaceView.vue）

- ToolBar 加 `:db-tools="mysqlStore.isConnected"`，4 个事件接线到 4 个 ref（`showDbTransfer`/`showDataGenerate`/`showDbSync`/`showStructureSync`）
- 挂载 4 个对话框组件（WorkspaceView 级，同 ImportExportDialog 模式），connId 取 `mysqlStore.connId`

### 3. 新对话框组件（src/components/mysql/，复用 ImportExportDialog 的两步向导模式）

四者共用「目标连接选择」逻辑：
- 目标连接下拉 = 「当前连接」（connId 直用，不建新连接）+ `store.savedConnections`（选中后用现有 raw API `mysqlConnect`（api/mysql.ts:16）建**独立**目标连接——**禁止用 store.connect，那会断开当前连接**）
- 选中目标连接后 `mysqlDbList(targetConnId)` 加载库下拉；对话框关闭时 `mysqlDisconnect(targetConnId)` 清理

1. **DataTransferDialog.vue（数据传输）**：源=当前连接+源库下拉（默认 `current_db`）；表复选列表（全选/全不选）；传输内容（结构+数据/仅结构/仅数据）+ 目标已有表时覆盖重建开关；执行逐表 loading + 结果（行数），完成后 toast
2. **DataGenerateDialog.vue（数据生成）**：表下拉（`store.tables`）+ 行数 + 生成前清空开关；每列规则行（列名只读 + 生成类型下拉 + 参数 min/max/长度/候选值）；打开时按列类型预填默认规则（int→随机整数、varchar→随机字符串、datetime→日期时间、enum→解析候选、其余→固定值）；预览 3 行样例后执行
3. **DbSyncDialog.vue（数据同步）**：源表下拉 + 目标（连接/库/表三下拉）；「比对」按钮调后端按主键比对，展示三组计数（仅源有/仅目标有/不一致）+ 差异主键明细（各限 20 条）；三个开关（插入缺失行/删除多余行/更新不一致行）；执行按钮调同步（后端执行时重新比对后应用）
4. **StructureSyncDialog.vue（结构同步）**：源库 + 目标（连接/库两下拉）；「比对」调后端结构比对，展示逐表差异计划（目标缺失表=CREATE / 列差异=ALTER，same 不显示）；执行逐条执行并显示结果

### 4. API 层：src/api/mysqlTools.ts（新）

封装 4 命令 invoke（组件禁直接 invoke 惯例），模型类型从 `@/bindings` re-export。invoke 顶层参数 camelCase（Tauri 自动转换），结构体字段 snake_case（serde 命名约定，禁 rename_all）。

## 后端改动

### 新文件三件套（对齐 mysql_db.rs / mysql_io.rs 模式）

**src-tauri/src/models/mysql_tools.rs**（serde snake_case）：
- `MySqlTransferOptions { target_conn_id, target_db, tables: Vec<String>, include_structure, include_data, recreate }`
- `MySqlTransferTableResult { table, rows, skipped }`
- `MySqlGenerateColumnRule { name, kind(int/decimal/string/uuid/name/phone/email/datetime/fixed), min, max, length, values: Vec<String>, null_ratio: Option<u32> }`
- `MySqlGenerateOptions { table, rows, truncate, columns }`
- `MySqlDataSyncOptions { source_table, target_conn_id, target_db, target_table, insert_missing, delete_extra, update_diff }`
- `MySqlDataSyncOutcome { key_columns: Vec<String>, only_source, only_target, changed: u64, sample_source/sample_target/sample_changed: Vec<Vec<String>> }`
- `MySqlStructureSyncOptions { source_db, target_conn_id, target_db }`
- `MySqlStructureSyncItem { table, kind(create/alter), sql }`
- `MySqlStructureSyncPlan { items: Vec<MySqlStructureSyncItem> }`

**src-tauri/src/services/mysql_tools.rs**：
- `data_transfer(source_conn_id, opts)`：逐表 SHOW CREATE TABLE 源 → 目标端 recreate 时先 DROP（容错表不存在）+ CREATE → SELECT 源 → 每 100 行多值 INSERT 目标；返回逐表结果
- `data_generate(conn_id, opts)`：information_schema.COLUMNS 校验列规则与实际列匹配（防注入）→ 可选 TRUNCATE → 按规则生成值（随机整数/小数/字符串/UUID/中文姓名/手机号/邮箱/日期时间/固定值 + null_ratio 概率置 NULL）→ 批量 INSERT，返回插入行数
- `data_sync(source_conn_id, opts, execute)`：双侧 `information_schema.KEY_COLUMN_USAGE` 读主键（CONSTRAINT_NAME='PRIMARY'，缺失或列序不一致报错）→ 双侧全量拉取 → PK 元组比对三类差异 → `execute=false` 仅返回差异与样例，`execute=true` 应用（缺失行多值 INSERT / 多余行 DELETE WHERE PK IN / 不一致行 REPLACE INTO）
- `structure_sync(source_conn_id, opts, execute)`：双侧 `information_schema.COLUMNS` 读列定义 → 目标缺失表（源端 SHOW CREATE TABLE 原文重限定表名后在目标执行）+ 双方共有表列差异（ADD/DROP/MODIFY，按 COLUMN_TYPE+IS_NULLABLE+归一化 DEFAULT 比对）→ `execute=false` 返回计划，`execute=true` 逐条执行返回结果

关键设计（复用现有方案）：
- 全部 SQL 以 `` `db`.`table` `` 限定，不依赖连接当前库上下文（同 mysql_db.rs `find_in_database` 方案）
- 标识符校验/反引号包裹、值转义复用 mysql_io.rs 的 `quote_ident`/`quote_value` 模式
- 连接获取复用 services::mysql 的 `pool_of`/`take_tx_conn`/`give_tx_conn`；目标连接由前端 `mysql_connect` 预先建立并注册在同一注册表，`target_conn_id` 直接 `pool_of` 可得
- mysql_err 归一化同款（`AppError::general`）

**src-tauri/src/commands/mysql_tools.rs**：4 个 `#[tauri::command]` 包装（confirmed 兜底模式按需）：
- `mysql_data_transfer(source_conn_id, options) -> Vec<MySqlTransferTableResult>`
- `mysql_data_generate(conn_id, options) -> u64`
- `mysql_data_sync(source_conn_id, options, execute) -> MySqlDataSyncOutcome`
- `mysql_structure_sync(source_conn_id, options, execute) -> MySqlStructureSyncPlan`

**lib.rs**：`specta_builder` commands 与 `invoke_handler` 两处注册 4 命令（对齐 mysql_io 分组注释）。bindings.ts 由 debug 构建时 tauri-specta 自动重生成（134 → 138 命令）。

## 验证

1. `cargo check` + `vue-tsc` 绿；启动 dev 使 bindings.ts 重生成
2. 测试服务器 192.168.31.100（凭据见记忆 test-servers）真实连接 e2e：
   - **数据传输**：命令列建 src/dst 两库，src 建表插数 → 工具栏传输到 dst → 验证行数与数据一致
   - **数据生成**：选表生成 100 行 → 验证行数、数据随机性与 NULL 概率
   - **数据同步**：同服务器两库同构表，命令列改 src 数据 → 比对显示差异 → 执行后两侧一致
   - **结构同步**：src 加列/改列类型 → 比对显示 ALTER → 执行后两侧列结构一致
   - **按钮可见性**：断开 MySQL 后 4 按钮隐藏，重连后出现（CDP 实测）
3. 提交（单次提交，范围虽大但四功能共享同一批基建）
