# FyShell 设计审查快照·第三轮（impeccable critique）

- 日期：2026-09-19
- Method: dual-agent (A: 静态源码审查 · B: a58ef0917226927fc CDP 浏览器取证)
- 范围：全部菜单（MenuBar 七菜单 + 树右键）、功能页面（SSH 终端/SFTP/MySQL/Redis/传输队列/快捷命令/隧道）、窗口（新建会话/设置/快捷键列表/会话列表），19 张截图 + 全量源码核对
- Surface 模式：Operate

## Design Health Score

Nielsen 10 heuristics：**31/40**（静态审查更严格，扣分点集中在前两轮截图未覆盖的 MySQL/Redis/传输队列页面）

| # | 启发式 | 二轮 | 本轮 | Δ |
|---|---|---|---|---|
| 1 | 系统状态可见性 | 4 | 4 | — |
| 2 | 匹配真实世界 | 4 | 3 | -1 树右键三项同名不同义 + "SQL 编辑区"假反馈 |
| 3 | 用户控制 | 4 | 4 | — |
| 4 | 一致性 | 4 | 3 | -1 divider/字段间距/选中态灰度/图标尺寸跨页偏差 |
| 5 | 错误预防 | 3 | 3 | — |
| 6 | 识别而非回忆 | 3 | 3 | — |
| 7 | 灵活高效 | 3 | 3 | — |
| 8 | 美学极简 | 4 | 4 | — |
| 9 | 错误恢复 | 3 | 3 | — |
| 10 | 帮助文档 | 2 | 2 | 遗留：无文档入口 |

## 二轮修复落地核对（CDP 截图确认全部存活）

- 菜单快捷键同行右对齐（文件/编辑/窗口）✅；工具菜单 divider 分组 ✅；快捷键列表对话框（全局/终端内两组）✅；标签颜色虚线空态 chip ✅；1920/1280 两档分辨率下无溢出裁剪 ✅

## Priority Issues（综合 A 静态 + B 视觉）

| 严重度 | 问题 | Fix 方向 |
|---|---|---|
| 中(B) | SSH 隧道页筛选下拉宽度塌缩：placeholder"全部会话"不可见（v-field 52px、input 22px），无法识别为过滤器（TunnelView.vue:10-23） | 加 min-width 或默认值"全部会话" |
| P2 | 树分区虚线硬编码 `#c6c9ce` 不随主题切换，深色下刺眼（WorkspaceView.vue:2612） | 改 `rgba(var(--v-theme-on-surface), 0.12)` |
| P2 | DDL 展示区三处注释写"等宽"实际宋体（.mysql-ws__ddl-text/ddl-input/grant-line），SQL 数据区应走 --fy-mono | 改 var(--fy-mono) |
| P2 | 树右键三项同一动作误导：db-leaf"打开数据库/新建查询/命令列界面..."全指向 menuNewQuery；"命令列界面..."暗示弹对话框实际只开 Tab | 合并为"新建查询..."一项 |
| P2 | 假反馈 toast『已发出新建查询请求，可在 SQL 编辑区输入并执行』——未发请求且无 SqlEditor（MysqlDbWorkspace.vue:842） | 改真实引导或移除 |
| P2 | size="small"+compact 高度 16px 疑点未推广：MySQL 工具条 ~15 个按钮/Redis 工具条/TransferQueue 刷新等同根因 | CDP 先取证实际高度再统一 height: 26px |
| 低(B) | 快捷命令面板标题硬编码英文"Compose Pane"（ComposePane.vue:19），全应用唯一英文标题 | 改中文 |
| P3 | 文案一致性：文件菜单"打开"无省略号；toggle-theme 两处文案（"主题"vs"切换主题"） | 统一 |
| P3 | 对话框标题行 divider 不一致（4 个有/3 个无） | 统一模式 |
| P3 | 选中态灰度三处三个值：SSH 选项 0.15/MySQL tab 0.16/树 0.18 | 统一 0.15 |
| P3 | 帮助按钮弹内部术语 toast『FyShell P0 — Tauri 2…』，与"关于"入口不一致（WorkspaceView.vue:2026） | 同样打开设置 about 分区 |
| P3 | LogViewer 无标题行无 ×，依赖遮罩关闭（WorkspaceView.vue:2309） | 补标题栏 |
| P3 | FilePane/TransferQueueView 内联 SVG 硬编码 mdi path，脱离 Lucide 集合 | mdiMap 登记统一通道 |

## Minor Observations

- 图标显式数字 size 七档混用（10/12/13/14/16/18/20），chrome 图标钮建议收敛 18/20 两档
- ComposeBar 目标文案不平行："到当前会话/全部会话/到可见标签"
- 旧 fallback 值冗余（FlexTabs outline 46 111 219、--fy-row-height-tree 24px fallback）
- WorkspaceView 死样式 .workspace__node-host（hostLabel 恒空串）
- TransferQueueView statusColor 返回 'grey' 非语义色约定成员
- MySQL/Redis 未连接占位区标题样式不同（Redis 有 tonal 卡片，MySQL 无）

## Persona Red Flags

- 新手运维：隧道筛选框不可识别 + "SQL 编辑区"假反馈引用不存在区域——引导双缺位
- 键盘流：无红旗（快捷键速查齐备）
- Xshell 老用户：无红旗
