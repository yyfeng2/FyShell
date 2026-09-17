/**
 * Tauri Channel 创建辅助 + 事件监听管理辅助
 *
 * - Channel：用于 SSH 终端输出流、传输进度等有序流式数据（契约 §1.4）
 * - 监听管理：收集多个 unlisten Promise，组件卸载时一次性清理（架构红线）
 */
import { Channel, invoke } from '@tauri-apps/api/core';
import type { UnlistenFn } from '@tauri-apps/api/event';

/** 临时诊断：前端关键路径日志写到 Rust stdout（dev 模式进 dev.log），便于远端定位链路问题 */
export function debugLog(message: string): void {
  void invoke('debug_log', { message }).catch(() => {
    /* 诊断日志失败不影响业务 */
  });
}

/**
 * 创建一个 Tauri Channel，把消息转发给 onMessage 回调。
 *
 * @param onMessage 收到每条消息时的回调（数据为 Rust 侧序列化后的值）
 */
export function createChannel<T>(onMessage: (data: T) => void): Channel<T> {
  const channel = new Channel<T>();
  channel.onmessage = onMessage;
  return channel;
}

/**
 * 监听管理器：集中收集 unlisten 函数，组件卸载时一次性调用 cleanup()。
 *
 * 用法（Vue 组件内）：
 * ```ts
 * const group = createUnlistenGroup();
 * onMounted(() => {
 *   group.track(listenSessionStatus(handler));
 *   group.track(listenHostkeyPrompt(handler2));
 * });
 * onBeforeUnmount(() => group.cleanup());
 * ```
 *
 * cleanup() 是幂等的：重复调用不会重复 unlisten。
 */
export interface UnlistenGroup {
  /** 登记一个 listen() 返回的 Promise<UnlistenFn> */
  track: (unlistenPromise: Promise<UnlistenFn>) => void;
  /** 执行全部已登记的 unlisten（幂等） */
  cleanup: () => void;
}

export function createUnlistenGroup(): UnlistenGroup {
  const pending: Array<Promise<UnlistenFn>> = [];
  let disposed = false;

  return {
    track(unlistenPromise: Promise<UnlistenFn>): void {
      if (disposed) {
        // 已清理后再登记的监听，立即自行解除，避免泄漏
        unlistenPromise
          .then((unlisten) => unlisten())
          .catch(() => {
            // 忽略解除失败
          });
        return;
      }
      pending.push(unlistenPromise);
    },
    cleanup(): void {
      if (disposed) return;
      disposed = true;
      for (const p of pending.splice(0)) {
        p.then((unlisten) => unlisten()).catch(() => {
          // 忽略解除失败
        });
      }
    },
  };
}

/**
 * 便捷函数：同时登记多个 unlisten Promise（等价于逐个 track）。
 */
export function trackAll(
  group: UnlistenGroup,
  unlistenPromises: Array<Promise<UnlistenFn>>,
): void {
  for (const p of unlistenPromises) {
    group.track(p);
  }
}
