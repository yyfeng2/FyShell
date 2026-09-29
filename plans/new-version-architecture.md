# 新版 FyShell 架构与功能优先级方案

> 生成日期：2026-09-17
> 定位：Tauri 2 + Vue 3 重写版；SSH 前端参考 Xshell/Xftp，数据库参考 Navicat Premium
> 项目名：**FyShell**，代码目录：e:\fyshell
> 背景：旧版（HexHub-Client 5.1.9，Nuxt 3 + Electron）已放弃

---

## 一、技术架构

### 1.1 技术选型

| 领域 | 选型 | 理由 |
|---|---|---|
| 桌面框架 | **Tauri 2**（Rust） | 安装包 ~5-10MB、内存占用低，适合常驻型管理工具 |
| SSH/SFTP | **russh 0.63 + russh-sftp** | 最活跃（Tabby 作者维护）、纯 Rust、PTY/shell 全支持 |
| MySQL | **mysql_async** | 动态任意 SQL 无法编译期校验，选纯驱动 + Tokio 原生 |
| 终端 | xterm.js + WebGL addon | 官方验证组合；监听 `webglcontextlost` 自动重建 |
| 进度推送 | **Channel API**（不用 event） | 官方专为有序流式设计，类型安全 |
| 持久化 | Rust 侧 SQLite/store（凭据）+ IndexedDB（缓存） | 敏感凭据不进 WebView |
| 前端 | Vue 3 + Vuetify 3 + Pinia + CodeMirror 6 | 沿用熟悉生态；**tauri-specta** 自动生成 TS 绑定 |

### 1.2 目录结构

```
fyshell/
├── src/                      # Vue 3 前端
│   ├── api/                  #   invoke 封装层（组件不直接调 invoke）
│   ├── composables/
│   ├── components/
│   │   ├── common/           #   AdvancedTable / FlexTabs 等按旧版重写
│   │   ├── ssh/  ├── db/  ├── ftp/  └── editor/
│   └── views/
├── src-tauri/
│   ├── src/
│   │   ├── commands/         # 薄层：参数校验 + 转发
│   │   ├── services/         # ssh / sftp / mysql / transfer 业务模块
│   │   └── models/
│   ├── capabilities/         # 细粒度权限声明
│   └── tauri.conf.json
```

### 1.3 关键架构决策（对照旧版教训）

| 旧版 5.1.9 的问题 | 新版方案 |
|---|---|
| 自定义 XOR 混淆二进制 RPC，无认证 | 标准 Tauri IPC（invoke + Channel），无需模拟协议 |
| 凭据存浏览器 IndexedDB（可被触达） | 凭据存 Rust 侧 SQLite + 主密码保护（借鉴 Xshell） |
| 无托盘、无自动更新、无单实例 | single-instance（最先注册）+ tray + updater + autostart 全上 |
| Electron 内存占用大、GPU 加速需禁用 | Tauri 2 轻量；xterm WebGL context lost 用自动重建而非全局禁 GPU |
| 便携版更新无逻辑 | NSIS（currentUser）+ 自签名 + updater JSON endpoint；便携版自建"重命名替换"更新 |
| 英文 i18n 空壳 | 默认 UTF-8/中文，语言包同步维护，编码自动探测（Xshell 乱码是高频抱怨） |

### 1.4 IPC 与长任务设计

- **命令（invoke）**：Rust 侧 `commands/` 保持轻量，重计算用 `async` / `spawn_blocking`；统一错误类型（thiserror + Serialize）。
- **Channel API**：文件传输进度、SSH 终端输出流用 Channel（有序、类型安全、高吞吐）；Rust 侧 4KB 批量读取再 send。
- **event**：仅低频状态推送（连接状态、会话列表变更）；多窗口用 `emit_to` 定向发送。
- **高频坑位**：所有 `listen` 组件卸载时 `unlisten()`；标签关闭时 Rust 侧同步清理 SSH session/PTY；resize 按会话 ID 路由。
- **ANSI 转义序列解析交给 xterm.js**，Rust 只透传字节流。
- **模型命名**：IPC 结构体字段一律 snake_case，禁 `rename_all = "camelCase"`（AuthType 枚举除外）。

---

## 二、功能优先级分期

### P0 — 第一期（跑通全链路）

1. **应用骨架**：窗口/托盘/单实例/主题（dark-light-auto）/全局弹层组件
2. **连接管理**：会话树（文件夹分组 + 搜索过滤）、5 种认证方式（密码/私钥/交互式/不验证/跳板机）、测试连接、会话配置存 Rust 侧
3. **SSH 终端**：xterm + russh shell、多标签、分屏、HostKey 确认、编码设置
4. **SFTP 双栏**（参考 Xftp）：本地/远程双栏、拖拽传输、上传下载、传输队列（Channel 进度）、断点续传、终端路径联动

### P1 — 第二期（对齐 Xshell 核心体验）

1. **服务器监控**：CPU/内存/网络迷你条 + 抽屉面板（Docker 容器管理）
2. **快捷命令**：树状分类管理（Xshell 8 模式）+ Compose Pane（多行草稿、发送到单/多会话）
3. **SSH 隧道**：本地/远程/SOCKS 规则管理界面
4. **会话增强**：认证配置文件（一处改全局生效）、会话克隆、会话保活、标签拖出新窗口、会话日志记录 + 日志查看器
5. **MySQL 基础**：连接、表列表、数据网格编辑（分页/筛选/排序/事务）

### P2 — 第三期（对齐 Navicat 核心）

1. **表设计器**：字段/索引/外键/触发器/高级 + 实时 DDL 预览默认可见（旧版 LCS 对齐生成 ALTER 的方案可复用）
2. **SQL 控制台**：补全、格式化、执行计划树、多结果集、查询历史快召回
3. **数据编辑增强**：表单视图、显式 NULL 标识、批量编辑、写操作"预览→确认→执行"管道（建立信任）
4. **导入导出**：CSV/Excel/JSON/XML/SQL 向导式流程

### P3 — 第四期（差异化与加分项）

> **2026-09-17 决定：P3 任务剔除，不做。** 项目范围止于 P2（后端 + 前端均已完成）。本节仅作历史记录保留。

1. **国产数据库支持**：达梦、OceanBase、GaussDB（Navicat 明确不支持，差异化机会）
2. **结构同步 / 数据传输 / 备份还原**（Navicat 口碑最高的三项）
3. **触发器/关键词高亮集**（终端输出匹配自动执行）
4. **命令面板**（Cmd-P 式）、数据字典、AI SQL 助手

---

## 三、设计参考要点速查

> 本节为设计细节交叉索引，每条括号内标注对应分期。

### 3.1 布局

- 左导航树（可停靠/自动隐藏）+ 右多 Tab 工作区 + 底部状态栏（P0.1）
- Tab 按连接着色（生产/测试一眼区分）；紧凑行高保证信息密度（P0.2）
- 终端区支持垂直/水平分屏，标签与分屏可叠加（P0.3）

### 3.2 借 Xshell/Xftp

- **Compose Pane**：多行命令先草稿再发送，可选发送到当前/所有/指定会话（P1.2）
- **会话日志**：输出自动落盘 + 集成式日志查看器（彩色、可搜索）（P1.4）
- **认证配置文件**：认证变量集独立管理，一处改全局生效（P1.4）
- **树状快捷命令**：多组命令集分类管理，未注册也可调用（P1.2）
- **主密码保护**：凭据库整体加密，落地于连接管理（P0.2）；断点续传落地于 SFTP 双栏（P0.4）
- **未排期**：Rzsz（rz/sz）、同步浏览

### 3.3 借 Navicat Premium

- **连接组织**：连接着色 + 虚拟分组 + 星标置顶 + 导航过滤栏（P0.2 起步，虚拟分组/星标可后置）
- **表设计器**：字段/索引/外键/触发器 + DDL 实时预览（保存前可复制、可审查）（P2.1）
- **数据编辑**：网格行内编辑 + 表单视图双模式、显式 NULL 标识（P2.3）
- **杀手锏**：数据传输/结构同步/备份还原（P3.2）

### 3.4 技术红线

- 全键盘可达（Xshell 惯例：Ctrl+T 新标签、Ctrl+Tab 切换、Alt+1~9 直达）
- 深色主题全覆盖（所有窗口、视图、菜单与控件）
- 流式 + 虚拟滚动保证大数据不卡
- 危险操作（DROP/TRUNCATE/删除）二次确认 + 操作审计日志

---

## 四、主要参考来源

- [Tauri 官方文档（v2）](https://v2.tauri.app/) | [Updater 插件](https://v2.tauri.app/plugin/updater/)
- [russh (crates.io)](https://crates.io/crates/russh) | [russh-sftp](https://github.com/AspectUnk/russh-sftp) | [Tabby](https://github.com/Eugeny/tabby)
- [xterm.js #4128 WebGL context lost](https://github.com/xtermjs/xterm.js/issues/4128)
- [Xshell 官方产品页](https://www.xshell.com/zh/xshell/) | [Xftp 官方产品页](https://www.xshell.com/zh/xftp/)
- [Navicat Premium 官方产品页](https://www.navicat.com/en/products/navicat-premium) | [Navicat UI 专页](https://www.navicat.com/en/products/navicat-ui)
- 旧版 HexHub-Client 5.1.9 源码分析（2026-09-17）
