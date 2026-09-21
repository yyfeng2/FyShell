# 第三方开源组件说明

FyShell 基于 MIT License 开源发布。本项目使用了以下第三方开源组件，在此向各开源项目及作者致谢。

## 前端

| 组件 | 版本 | 许可证 | 用途 |
|---|---|---|---|
| [Vue](https://github.com/vuejs/core) | ^3.5 | MIT | 前端 UI 框架 |
| [Vuetify](https://github.com/vuetifyjs/vuetify) | ^3.7 | MIT | Material 组件库 |
| [Pinia](https://github.com/vuejs/pinia) | ^2.2 | MIT | 状态管理 |
| [xterm.js](https://github.com/xtermjs/xterm.js)（@xterm/xterm 及 fit/search/webgl 插件） | ^5.5 | MIT | 终端模拟与 WebGL 渲染 |
| [CodeMirror 6](https://github.com/codemirror/dev)（@codemirror/state/view/autocomplete/lang-sql） | ^6 | MIT | SQL 编辑器（高亮/补全） |
| [Mermaid](https://github.com/mermaid-js/mermaid) | ^12 | MIT | ER 模型图渲染 |
| [Lucide Icons](https://github.com/lucide-icons/lucide)（lucide-vue-next） | ^1.0 | ISC | 图标库 |
| [sql-formatter](https://github.com/sql-formatter-org/sql-formatter) | ^15.8 | MIT | SQL 美化格式化 |
| [@tauri-apps/api](https://github.com/tauri-apps/tauri) | ^2 | MIT OR Apache-2.0 | Tauri 前端 API |
| @tauri-apps/plugin-dialog / plugin-opener / plugin-updater | ^2 | MIT OR Apache-2.0 | 系统对话框/打开器/更新插件 |

## 后端（Rust）

| 组件 | 许可证 | 用途 |
|---|---|---|
| [Tauri](https://github.com/tauri-apps/tauri)（tauri 及 dialog/opener/single-instance/updater/autostart 插件） | MIT OR Apache-2.0 | 应用框架 |
| [tokio](https://github.com/tokio-rs/tokio) | MIT | 异步运行时 |
| [russh](https://github.com/warp-tech/russh) | Apache-2.0 | SSH 协议实现（ring 加密后端） |
| [russh-sftp](https://github.com/warp-tech/russh-sftp) | Apache-2.0 | SFTP 子系统 |
| [rusqlite](https://github.com/rusqlite/rusqlite) | MIT | SQLite 持久化（bundled） |
| [mysql_async](https://github.com/blackbeam/mysql_async) | MIT OR Apache-2.0 | MySQL 客户端 |
| [redis-rs](https://github.com/redis-rs/redis-rs) | BSD-3-Clause | Redis 客户端 |
| [portable-pty](https://github.com/wezterm/wezterm) | MIT | 本地终端 PTY（Windows ConPTY / Unix PTY） |
| [serialport-rs](https://github.com/serialport/serialport-rs) | MPL-2.0 | 串口通信 |
| RustCrypto（[chacha20poly1305](https://github.com/RustCrypto/AEADs) / [pbkdf2](https://github.com/RustCrypto/password-hashes) / [sha2](https://github.com/RustCrypto/hashes)） | MIT OR Apache-2.0 | 凭据保险库加密（AEAD + 密钥派生） |
| [serde](https://github.com/serde-rs/serde) / serde_json | MIT OR Apache-2.0 | 序列化 |
| [specta](https://github.com/specta-rs/specta) / tauri-specta | MIT | IPC 类型绑定自动生成 |
| [thiserror](https://github.com/dtolnay/thiserror) | MIT OR Apache-2.0 | 错误定义 |
| [uuid](https://github.com/uuid-rs/uuid) / [chrono](https://github.com/chronotope/chrono) / [rand](https://github.com/rust-random/rand) | MIT OR Apache-2.0 | 通用工具 |

## 说明

- 以上组件均通过 crates.io / npm 正式渠道引入，各自许可证原文可在对应项目仓库查阅。
- 本项目未修改任何第三方组件源码；随安装包分发的 Rust 依赖以 cargo `cargo license` 输出为准。
- 如认为本清单有遗漏或错误，欢迎提 Issue 指出。
