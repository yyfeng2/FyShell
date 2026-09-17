# FyShell Updater 自动更新配置说明

## 当前状态

- Tauri 2 updater 插件已接线：Cargo.toml 依赖、lib.rs 注册、capabilities 权限均已配置。
- 菜单栏「帮助 → 检测更新」已可用：无更新提示「已是最新版本」；有更新弹确认框下载安装
  （安装落盘后重启应用生效）；网络失败 toast 报错。
- **项目尚无发布渠道**，`tauri.conf.json` 中 endpoints 为占位地址，当前检测会因无法访问而报"网络错误"，属预期行为。

## 密钥位置

- 私钥：`src-tauri/keys/updater.key`（无密码，本地开发用，已加入 `.gitignore`，勿入库）
- 公钥：`src-tauri/keys/updater.key.pub`
- 公钥指纹：`58B19ADF726E0BCD`
- 公钥已写入 `tauri.conf.json` 的 `plugins.updater.pubkey`（即 .pub 文件内容的单行 base64）。

## 正式发布时需要做的事

1. **替换端点**：将 `tauri.conf.json` 中 `plugins.updater.endpoints` 的占位地址
   `https://releases.example.com/fyshell/{{target}}/{{current_version}}`
   替换为真实发布渠道 URL（如 GitHub Releases / 自建静态服务器）。`{{target}}`、
   `{{current_version}}` 为 Tauri 内置模板变量，保持不变。
2. **构建签名产物**：`tauri.conf.json` 已开启 `bundle.createUpdaterArtifacts`，
   执行 `npm run tauri build` 时会生成安装器及配套 `.sig` 签名文件（NSIS 下为
   `.exe` + `.exe.sig`）。构建时签名需要私钥，通过环境变量传入：
   - `TAURI_SIGNING_PRIVATE_KEY_PATH=src-tauri/keys/updater.key`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD=`（空密码可省略）
3. **上传发布物**：将 `.exe` 与 `.exe.sig` 上传至端点对应目录，并放置一份
   Tauri updater 静态 JSON 清单（含 `version`、`pubkey`、`platforms`、`url` 等字段）。
4. **版本号**：发布新版本前更新 `tauri.conf.json` 与 `Cargo.toml`/`package.json` 的
   `version` 字段，保持一致。

## 注意事项

- 私钥或密码丢失后无法再签名更新包，更新链路将失效，请妥善备份。
- `plugins.updater` 为 Tauri 2 格式（无 `active` 字段），是否检测/安装由前端调用
  `@tauri-apps/plugin-updater` 的 `check()` / `downloadAndInstall()` 控制。
- 检测更新能力依赖 `capabilities/default.json` 中的 `updater:default` 权限。
- 自动重启需依赖 `@tauri-apps/plugin-process` 的 `relaunch()`，当前未引入；
  安装完成后提示用户手动重启，后续如需自动重启再补 process 插件。
