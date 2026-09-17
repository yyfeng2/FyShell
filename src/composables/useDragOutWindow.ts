/**
 * 标签拖出新窗口组合式函数（契约 5.3 P1 前端增强）
 *
 * 用 @tauri-apps/api/webviewWindow 的 WebviewWindow 创建新窗口展示会话终端：
 * - label 唯一：`session-<会话 id>`（与 capabilities 中 "session-*" 窗口匹配模式一致）
 * - URL 带会话 id 参数：`index.html?session=<id>`，新窗口主入口读取参数后自动打开对应终端
 * - 尺寸 800x600，深色主题（架构红线：深色主题全覆盖）
 *
 * 注意：创建 WebviewWindow 需要 `core:webview:allow-create-webview-window` 权限。
 */
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'

export interface DragOutWindowOptions {
  /** 拖出的会话 ID（新窗口据此自动打开对应终端） */
  sessionId: string
  /** 新窗口标题（默认 "FyShell"） */
  title?: string
  /** 窗口宽度，默认 800 */
  width?: number
  /** 窗口高度，默认 600 */
  height?: number
  /** 新窗口主题，默认深色（架构红线：深色主题全覆盖） */
  theme?: 'dark' | 'light'
}

export function useDragOutWindow() {
  /**
   * 创建展示该会话终端的新窗口。
   * 同 label 窗口已存在时聚焦该窗口并跳过创建（避免重复创建报错）；
   * 创建失败返回 null，不阻塞调用方。
   */
  async function openSessionWindow(opts: DragOutWindowOptions): Promise<WebviewWindow | null> {
    // label 唯一：以会话 id 作后缀，重复拖出/多会话互不冲突
    const label = `session-${opts.sessionId}`
    const existing = await WebviewWindow.getByLabel(label)
    if (existing) {
      try {
        await existing.setFocus()
      } catch {
        /* 无 set-focus 权限时忽略，仅跳过创建 */
      }
      return null
    }
    const win = new WebviewWindow(label, {
      url: `index.html?session=${encodeURIComponent(opts.sessionId)}`,
      title: opts.title ?? 'FyShell',
      width: opts.width ?? 800,
      height: opts.height ?? 600,
      center: true,
      theme: opts.theme ?? 'dark',
    })
    return new Promise<WebviewWindow | null>((resolve) => {
      win.once('tauri://created', () => resolve(win))
      win.once('tauri://error', () => resolve(null))
    })
  }

  return { openSessionWindow }
}
