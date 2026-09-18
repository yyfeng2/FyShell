# Redis 数据库连接管理工具（多智能体并行开发）

## Context

FyShell 现有 MySQL 全链路（连接管理 + 会话树 + 工作台 Tab）已成熟。用户要求新增 **Redis 连接管理工具**，明确选择「完整版」范围：**连接管理（连接/断开/测试 + 已存连接 vault 加密对齐）+ Redis 工作台 Tab（Keys 浏览面板 + 命令执行条 CLI）+ 会话树与独立 Tab 双入口**（与 MySQL 完全对齐）。

Redis 不属于已剔除的 P3（P3=国产数据库/结构同步等），是此前多智能体 wave 中跳过的独立项，现重新启动。整套实现**逐级复刻 MySQL 已建成的模式**（模块级注册表、薄命令层、vault 感知 store、会话树集成），无新架构决策。

## 技术选型

- **依赖**：`redis` crate（redis-rs）新增到 [Cargo.toml](src-tauri/Cargo.toml)，版本由 `cargo add redis --features tokio-comp` 解析最新（0.2x）；无需 deadpool/池化库——对齐项目手写 `OnceLock<Mutex<HashMap>>` 注册表模式，注册表存 **`redis::aio::MultiplexedConnection`**（单多路复用连接，Redis 单线程模型足够）。
- **后端模型** `RedisConnection`：host/port/username*Option*（ACL）/password*Option*/db*u8*（默认库）。字段全 snake_case（bindings 无 camelCase 转换）。
- **任意命令执行**统一走后端 `redis_exec`，返回类型化 `RedisExecResult { kind, value }`，前端据此渲染（string/hash/list/set/zset 值查看均用 exec 组装，不另设后端命令）。

## 智能体分工（3 个并行 + 集成）

### Agent A — 后端全部（无依赖，最先动）

新增文件 + 改动清单：

1. **Cargo.toml**：加 `redis` 依赖（`cargo add` 拉最新）。
2. **src-tauri/src/models/redis.rs**（新）：`RedisConnection`（host/port/username: Option<String>/password: Option<String>/db: u8）+ `RedisExecResult { kind: String, value: serde_json::Value }`，均 `#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]`；mod 登记进 models/mod.rs。
3. **src-tauri/src/services/redis.rs**（新）：照抄 services/mysql.rs 骨架——
   - `static CONNS: OnceLock<Mutex<HashMap<String, Mutex<MultiplexedConnection>>>>` + `pub fn init()`（无参，lib.rs setup 调）+ `get_or_init` 惰性兜底
   - `connect(&RedisConnection) -> Result<String, AppError>`：`Client::open` + `get_multiplexed_async_connection`（建连即验证，含 SELECT db）→ PING 探活 → `Uuid::new_v4()` 存注册表；错误归一 `redis_err`
   - `disconnect(&str)`
   - `test(&RedisConnection) -> Result<String, AppError>`：建连→PING→断（SessionForm/连接表单测活用，不落注册表）；返回服务器版本摘要
   - `info(&str) -> Result<HashMap<String,String>, AppError>`（服务器信息，工作台展示用，可选）
   - `select_db(&str, u8)`：SELECT 后**更新该连接注册条目**（MultiplexedConnection 在同一连接内 SELECT 会改变默认库，重新获取连接实例）
   - `keys(&str, &str) -> Vec<String>`：KEYS pattern
   - `exec(&str, args: &[String]) -> RedisExecResult`：`redis::cmd(args[0]).arg(&args[1..])` → `redis::Value` → JSON 递归转换（nil→null、int→number、Data 优先 UTF-8 字符串、非 UTF-8 用 `base64:` 前缀字符串、Bulk/Map/Status→对应 json 结构），错误转 AppError
4. **src-tauri/src/commands/redis.rs**（新）：薄层命令，每命令 `#[tauri::command]` + `#[specta::specta]` + `Result<T, AppError>`：
   `redis_connect` / `redis_disconnect` / `redis_test` / `redis_info` / `redis_select_db` / `redis_keys` / `redis_exec`（共 7 个）
5. **src-tauri/src/commands/mod.rs / services/mod.rs**：`pub mod redis;`
6. **src-tauri/src/lib.rs**：`collect_commands!`（~L72 MySQL 段后）与 `generate_handler!`（~L208）**两处**逐行登记 7 命令；setup 内加 `crate::services::redis::init();`
7. **自检**：`cargo check` 零警告。完成后告知集成者触发 bindings 重生成。

### 集成步骤（A 完成后执行）

bindings.ts 需含新类型/命令（生成依赖 tauri dev 启动，Windows 上 `export_ipc_bindings_snapshot` 测试会崩 0xc0000139，勿依赖）。生成方式：`npm run tauri dev` 启动一次后 Ctrl+C（specta 自动重写 src/bindings.ts；项目有先例）。若启动受阻，按项目先例手工补 bindings 条目（snake_case 字段手编）。

### Agent B — 前端接入（依赖 bindings 生成，与 C 并行）

1. **src/api/redis.ts**（新）：7 个 invoke 封装 + `import type { RedisConnection, RedisExecResult } from '@/api/types'` re-export。
2. **src/api/types.ts**：从 bindings re-export `RedisConnection`/`RedisExecResult`。
3. **src/stores/redis.ts**（新）：对齐 stores/mysql.ts——
   - state：connId/connLabel/connecting/connError/activeSavedId/savedConnections/vaultLocked/db(默认 0)/keys/keysLoading/pattern
   - `SAVED_CONN_KEY = 'redis_saved_connections'`；`loadSavedConnections`/`persistSavedConnections` 走 vault（vaultStatus→密文 `v1$` 则 vaultDecrypt、写 vaultEncrypt，同 mysql）
   - `connect(config)`（先 disconnect 再连，连后自动 `loadKeys`）/`disconnect`/`saveConnection`（host+port 去重）/`removeConnection`/`unlockVault`/`connectSaved`/`selectDb`/`loadKeys`/`refreshKeys`/`exec(args)`
4. **src/components/ssh/session/SessionForm.vue**：
   - `SessionKind` type 加 `'redis'`；`SESSION_KINDS` 加 `{ value: 'redis', title: '数据库 (Redis)' }`
   - username 显示条件加 redis、密码显示条件加 redis
   - 端口联动 `kind === 'redis' ? 6379`
   - `buildConfig` mysql 分支判断扩为含 redis（session_type: 'redis' 写入）
   - `mapSessionKind` 识别 `'redis'`
   - `runTest` 加 redis 分支：`redisTest({host,port,username,password,db:0})` 建连即断，成功 toast
5. **src/views/WorkspaceView.vue**：
   - 分区过滤 [L279](src/views/WorkspaceView.vue#L279)：`isMysqlSession` 判定扩为 `stype === 'mysql' || stype === 'redis'`（数据库区同时容纳两者；保留字段名 isMysql 加注释，避免牵动树图标/路由多处）
   - `treeIconColor`/树图标：redis 复用 mdi-database（lucide 映射已存在，避免回退 CircleHelp）；颜色 primary 与 MySQL 一致或加区分（提示「按用户反馈可作为分类色，听后」——默认 primary）
   - 右键连接路由加 `if (stype === 'redis') { void connectRedisSession(target); return }`；`connectRedisSession`（解出密码 → `redisStore.connect`）+ `openRedisTab()`（单例 type:'redis'，对齐 openMysqlTab）
   - MenuBar 若有 mysql 菜单项则对齐加 `'redis'` action → openRedisTab（若无则跳过）

### Agent C — 工作台 UI（依赖 bindings + store，与 B 并行）

1. **src/components/redis/RedisDbWorkspace.vue**（新，Tab 主体）：对齐 MysqlDbWorkspace——
   - 未连接占位区：标题卡片 + `v-alert connError` + 已存连接 `v-list-item`（连接/删除按钮）+ vaultLocked 解锁条目 + 「新建连接」→ RedisConnectionForm 对话框 + VaultUnlockDialog 复用
   - 已连接主区：`db index` 选择器（0-15 下拉，selectDb）+ 刷新
   - Keys 面板（内联或独立 RedisKeysPanel.vue）：pattern 输入 + 刷新；key 列表（点击选中 → 详情：TYPE/TTL/值）；删除 key（ui.confirm）
   - 值展示：string 直接展示；hash/list/set/zset 用 redis_exec 组装命令（HGETALL/LRANGE/SMEMBERS/ZRANGE WITHSCORES）后按结构表渲染
   - 命令执行条：单命令输入 + 执行（client 解析：空格分词 + 引号包裹处理 → redisExec）；结果显示按 kind 渲染（单值/表格/JSON 树）
2. **src/components/redis/RedisConnectionForm.vue**（新）：对话框，字段 host(默认127.0.0.1)/port(6379)/username(可空)/password(可空,明文切换)/db(默认0)；submit 走 `store.connect`，成功后 emit('connected', connLabel) 关窗
3. **src/views/WorkspaceView.vue Tab 挂载**：Redis 工作台 Tab 的动态组件渲染（对齐 mysql Tab 的渲染分支）

## 约定（所有智能体遵守）

- IPC 模型字段 snake_case；新增命令 lib.rs **两处**登记
- 前端组件禁止直接 invoke，一律走 `@/api/redis.ts`
- Vuetify 弹层样式需 `.v-application` 与 `.v-overlay-container` 双层选择器
- 图标只复用 lucide 映射集现有名（绝不新增未登记 mdi 名）
- 危险操作（删 key/断连）经 `ui.confirm`

## 派发顺序

1. Agent A（后端，无依赖）→ 2. 集成者触发 bindings 重生成 → 3. Agent B ∥ Agent C（前端两块，互不重叠文件）

## 验证

1. `cargo check` 零警告（A 完成时）
2. `npm run build` = vue-tsc + vite build 双绿（B/C 完成时）
3. **真实 Redis 端到端**（本机 redis-server 或用户提供地址）：
   - SessionForm 新建「数据库 (Redis)」会话 → 测试连接成功、保存 → 会话树数据库区出现、双击/右键连接打开 Redis Tab
   - 连接表单录入 → 已存连接列表（含设置主密码后 vault 加密/解锁流程）→ 连接成功
   - db 选择器切换 0-15、`DBSIZE` 随动
   - Keys 面板：pattern 过滤、查看 string/hash/list/set/zset 值、TTL、删除 key
   - 命令执行条：`SET k v`、`GET k`、`HGETALL h`、`PING`、非法命令报错提示
4. 深色主题下工作台 UI 无样式破裂
