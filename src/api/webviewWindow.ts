/**
 * 标签拖出新窗口辅助（契约 5.3：@tauri-apps/api/webviewWindow 的 WebviewWindow）
 *
 * 标签携带 session id，新窗口 URL 携带同名查询参数，窗口内自行连接。
 */
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';

/** 新窗口默认尺寸（契约：800x600） */
const SESSION_WINDOW_WIDTH = 800;
const SESSION_WINDOW_HEIGHT = 600;

/** 生成新窗口的唯一 label；同一会话复用同一 label，避免重复开窗 */
function sessionWindowLabel(sessionId: string): string {
  return `session-${sessionId}`;
}

/**
 * 打开承载指定会话的新窗口（label 唯一、URL index.html?session=<id>、尺寸 800x600）。
 *
 * 若该会话已有对应窗口，则聚焦已有窗口而不是重复创建。
 * 注意：WebviewWindow 构造后创建是异步的，创建失败通过 tauri://error 事件暴露。
 *
 * @param sessionId 要在新窗口中打开的会话 ID
 */
export async function openSessionWindow(sessionId: string): Promise<WebviewWindow> {
  const label = sessionWindowLabel(sessionId);
  // getByLabel 为异步 API，返回 Promise<WebviewWindow | null>
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    void existing.setFocus();
    return existing;
  }

  const webviewWindow = new WebviewWindow(label, {
    url: `index.html?session=${encodeURIComponent(sessionId)}`,
    width: SESSION_WINDOW_WIDTH,
    height: SESSION_WINDOW_HEIGHT,
    title: 'FyShell',
  });
  webviewWindow.once('tauri://error', (e) => {
    console.error('打开会话窗口失败:', e);
  });
  return Promise.resolve(webviewWindow);
}

/**
 * 从当前窗口 URL 中读取 session 参数（新窗口初始化时定位要连接的会话）。
 *
 * @returns session id；URL 无该参数时返回 null
 */
export function getSessionIdFromUrl(): string | null {
  return new URLSearchParams(window.location.search).get('session');
}
