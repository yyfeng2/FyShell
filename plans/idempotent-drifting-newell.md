# 报错友好化 + 撰写栏输入历史

## Context

用户反馈两个问题：

1. **所有报错都不友好**：Rust 侧 `AppError` 序列化为字符串（`SSH 错误: Authentication failed`、`SQLite 错误: UNIQUE constraint failed...`），MySQL 服务器错误整段英文原文直出；前端 13 个组件各自复制 `errText`（只做 `String(err)` 原样透传），另有大量 toast/错误横幅直接内插 `String(e)`，无任何翻译归纳。
2. **撰写栏（ComposeBar）不记录输入历史**：[ComposeBar.vue](src/components/ssh/terminal/ComposeBar.vue) 发送后直接清空 `draft`，输入框只绑 `@keydown.enter`，无历史记录与上下键处理。CLI 控制台（MysqlCliConsole.vue）已有完整 history 逻辑可参照。

## 改动一：错误信息友好化

### 1. 新建 `src/utils/errors.ts`

导出 `friendlyError(err: unknown): string`：

- 内部归一化：`typeof err === 'string' ? err : err instanceof Error ? err.message : String(err)`
- 命中规则 → 返回 `友好文案（原文）`，原文超 160 字符截断（保留可调试性）
- 未命中规则 → 原样返回（AppError 已带「SSH 错误:」等中文前缀，不破坏现有可读部分）

归一化规则表（正则，覆盖本应用常见错误族）：

| 原文模式 | 友好文案 |
|---|---|
| `ECONNREFUSED` / `Connection refused` | 连接被拒绝：目标端口未监听或被防火墙拦截 |
| `ETIMEDOUT` / `timed out` / `timeout` | 连接超时：请检查网络与端口 |
| `ENOTFOUND` / `no such host` / `getaddrinfo` | 域名解析失败：请检查主机地址 |
| `ENETUNREACH` | 网络不可达 |
| `Authentication failed` / `Access denied` / `permission denied` | 认证失败：请检查用户名、密码或密钥 |
| `connect error` / `connection not available` / `connect failed` | 连接失败：请检查主机、端口与网络 |
| `Connection reset` / `broken pipe` / `Connection closed` | 连接已断开，请重试 |
| `Unknown database` | 数据库不存在 |
| `doesn't exist` / `ER_NO_SUCH_TABLE` | 表不存在 |
| `Duplicate entry` / `ER_DUP_ENTRY` | 唯一键冲突：记录已存在 |
| `cannot be null` / `ER_BAD_NULL_ERROR` | 字段不能为空（NULL） |
| `syntax error` / `1064` | SQL 语法错误 |
| `UNIQUE constraint failed` | 记录已存在（唯一约束） |
| `FOREIGN KEY constraint failed` | 存在关联数据，无法操作（外键约束） |
| `No such file` / `ENOENT` | 文件或路径不存在 |
| `Error \d+ \(\d{5}\)`（MySQL 服务器错误码格式） | MySQL 报错（保留原文） |

匹配顺序按表中顺序（更具体的在前），命中即返回。

### 2. 替换点

**A. 删除 13 处重复的 `errText` 本地定义，改为 `import { friendlyError as errText } from '@/utils/errors'`**（一行 diff/文件，调用点零改动，立即全部友好化）：

- `src/stores/mysql.ts:451`、`src/stores/redis.ts:336`
- `src/composables/useTargetConnection.ts:16`
- `src/components/mysql/`：MysqlQueryTab、EditDatabaseDialog、ErModelDialog、FindInDbDialog、MysqlDataGrid、MysqlDbWorkspace、NewDatabaseDialog（grep 确认共 13 处，含上列）

**B. 直接内插 `String(e)` 的用户可见展示点改为 `friendlyError(e)`**（机械替换，逐处确认是 toast/错误横幅而非 debugLog）：

- `src/views/WorkspaceView.vue`（约 15 处：加载会话列表/删除/复制连接/打印数据库/连接失败等 toast）
- `src/views/tunnel/TunnelView.vue`（6 处）
- `src/views/mysql/TableDesigner.vue`（loadError 赋值 + 2 处 toast）
- `src/components/sftp/FilePane.vue`（约 7 处 errorMsg/notify）
- `src/components/mysql/HistoryDrawer.vue`（加载历史/已保存查询失败 + `String(err)` toast）
- `src/components/mysql/ExplainPanel.vue:119`（`执行计划获取失败: ${String(err)}`）
- `src/components/mysql/MysqlCliConsole.vue`（`push('error', \`ERROR ${String(e)}\`)` → friendlyError）
- `src/components/mysql/AutoRunPanel.vue`（`const msg = ...` 处）
- `src/stores/terminal.ts` 中 `debugLog` 里的 `String(e)` 为调试日志，**不改**

## 改动二：撰写栏输入历史

修改 `src/components/ssh/terminal/ComposeBar.vue`：

1. **模块级共享历史**（第二个 `<script lang="ts">` 块，所有终端 Tab 的 ComposeBar 实例共用，对齐 Xshell 行为）：
   - `const composeHistory = ref<string[]>([])`，localStorage 持久化（key `fy-compose-history`，惰性加载、try/catch 容错、上限 100 条）
2. **发送时记录**：`send()` 中非空文本发送后，若与末条不同则 push（连续重复折叠）并落盘
3. **上下键回溯**：输入框加 `@keydown.up` / `@keydown.down`（preventDefault），逻辑参照 MysqlCliConsole.vue 的 `onKeydown`（[MysqlCliConsole.vue:122-141]）：
   - 首次上翻到最新一条；已到最早一条则停
   - 下翻越过最新一条 → 清空输入并重置 `historyIndex`
4. **发送后重置** `historyIndex = -1`（`draft` 清空已有）

## 注意事项

- 工作区有约 15 个未提交的修改文件（与本次改动部分重叠），只做 surgically 替换，不动既有修改内容；改动前重新 Read 目标文件
- 有并行会话近期编辑过 MysqlCliConsole.vue / HistoryDrawer.vue，编辑前必须重读最新内容

## 验证

1. 前端构建通过（package.json build 脚本 / `tauri dev`）
2. 连接测试服务器 192.168.31.100（凭据见记忆），触发典型错误验证友好文案：
   - 查询 Tab 执行错误 SQL（语法错误 → 「SQL 语法错误（…）」）
   - 查询不存在的表（→ 「表不存在」）
   - 错误密码连接（→ 「认证失败…」）
3. 撰写栏：输入 2-3 条命令发送 → 上键依次回溯、下键返回；切换到另一终端 Tab 验证历史共享；重启应用验证持久化
