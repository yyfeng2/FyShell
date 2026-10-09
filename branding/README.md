# FyShell 品牌图标

此目录存放应用图标的**源资产**（设计稿原样入库），实际生效的图标集位于 `src-tauri/icons/`。

## 源资产文件

| 文件 | 说明 |
|---|---|
| `icon-source.jpg` | 设计稿去水印原图（微信传输件的清洗版，上游源头） |
| `icon-1024.png` | 1024×1024 RGBA 源图，`npx tauri icon` 的输入 |
| `icon.ico` | 设计工具逐尺寸导出的多尺寸 ICO（16/32/64/128/256 五尺寸） |

- 图标定稿日期：2026-10-09（橙色圆角方框 + 蓝色 "fy" 字样）
- 图标更换时仅取 `微信图片_20261009102557_4_8` 变体，同目录 `3_8` 变体为备选未采用

## 实际生效的图标集

`src-tauri/icons/` 由 `npx tauri icon` 从 `icon-1024.png` 生成全套标准集：

```bash
npx tauri icon branding/icon-1024.png
```

| 用途 | 文件 |
|---|---|
| 桌面/窗口 | `32x32.png`、`64x64.png`、`128x128.png`、`128x128@2x.png`、`icon.png` |
| Windows exe 资源 | `icon.ico`（**保留设计稿逐尺寸导出原版**，小尺寸比缩放重采样更锐利） |
| macOS | `icon.icns` |
| Windows 商店（MSIX） | `Square*.png` 共 9 个 + `StoreLogo.png` |

注意：

- 生成后 android/ios 移动端目录会一并产出，纯桌面项目可剔除（无引用）
- `tauri.conf.json` 的 `bundle.icon` 引用 `icons/icon.ico` 与 `icons/icon.png`，路径不变无需改动
- 托盘图标取 `app.default_window_icon()`（编译期从 bundle 图标嵌入），自动继承
- 图标在编译期嵌入 exe 资源，更换后需重新构建才生效
