# FyShell updater 签名密钥

此目录存放 Tauri updater 签名密钥的**本地备份**，密钥本身不入库（`.gitignore` 已排除 `*.key`）。

## 密钥文件

| 文件 | 说明 |
|---|---|
| `fyshell-signing.key` | 签名私钥（**绝密**，丢失后无法签发更新包，更新体系失效） |
| `fyshell-signing.key.pub` | 对应公钥（已配置进 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`） |

- 生成日期：2026-09-29（用户选择重新生成，替换了 v0.1.0 发布时的旧密钥对）
- 密码：**无密码**（`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 留空）
- 原始位置：`C:\Users\yyfeng14\.tauri\fyshell-signing.key`（用户目录备份，与本项目目录双份互备）

## 打包时使用

```bash
export TAURI_SIGNING_PRIVATE_KEY=$(cat signing/fyshell-signing.key)
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
npx tauri build
```

产物：`src-tauri/target/release/bundle/nsis/FyShell_x.y.z_x64-setup.exe` + 同名 `.sig` 签名文件（随 Release 上传，供 updater 验签）。

## 红线

- ⚠️ **私钥绝不能提交进 git 或推送到 GitHub**（仓库公开，推送即泄露）——`.gitignore` 已排除，勿移动密钥到别处。
- 公钥随安装包分发：已安装旧版（v0.1.0 旧 pubkey）用户自动更新会验签失败，需随新包重新分发新 pubkey 并更新 latest.json。
