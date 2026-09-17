/**
 * updater —— 自动更新检测（封装 @tauri-apps/plugin-updater）
 *
 * 当前项目尚无发布渠道，tauri.conf.json 中 endpoints 为占位地址；
 * 检测网络失败 / 无端点时优雅返回错误信息，由调用方决定 UI 提示方式。
 */
import { check, type Update } from '@tauri-apps/plugin-updater'

/** 检测更新结果：ok=false 时 error 为错误描述；无更新时 update 为 null */
export interface UpdateCheckResult {
  ok: boolean
  update: Update | null
  /** 网络失败 / 无端点 / 签名校验失败等错误描述 */
  error?: string
}

/** 检查更新：调用插件 check，网络失败或无端点时优雅返回错误信息 */
export async function check_update(): Promise<UpdateCheckResult> {
  try {
    const update = await check()
    return { ok: true, update }
  } catch (e) {
    return {
      ok: false,
      update: null,
      error: e instanceof Error ? e.message : String(e),
    }
  }
}

/**
 * 下载并安装更新（插件内部校验签名后落盘）。
 * 安装成功不会立即生效，需调用 relaunch() 重启应用；失败时抛出由调用方提示。
 */
export async function install_update(update: Update): Promise<void> {
  await update.downloadAndInstall()
}
