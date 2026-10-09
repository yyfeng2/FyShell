//! 单实例守卫（Windows）：默认仅允许一个实例在跑；设置 `allow_multiple_instances`
//! 为 true（设置对话框「允许多个客户端实例」）时直接放行，N 个实例并存。
//!
//! 实现：命名互斥 + 隐藏事件窗口 WM_COPYDATA 唤起。
//! - 命名互斥 `{identifier}-sim`：第二实例 `CreateMutexW` 得到 ERROR_ALREADY_EXISTS。
//!   互斥为内核对象，进程退出自动释放——崩溃也不留孤儿锁。
//! - 隐藏事件窗口 `{identifier}-sic`：首实例注册，收到 WM_COPYDATA 即唤起主窗口
//!   （show/unminimize/focus，与 tray.rs 唤起逻辑一致）。
//! - 多开开关开启：不建互斥、不注册窗口（last-write-wins 属多开固有行为）。

#[cfg(target_os = "windows")]
use std::sync::OnceLock;

use crate::services::settings_store;

#[cfg(target_os = "windows")]
use tauri::Manager;
#[cfg(target_os = "windows")]
use windows::core::PCWSTR;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{
    GetLastError, ERROR_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
#[cfg(target_os = "windows")]
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::CreateMutexW;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, FindWindowW, RegisterClassExW, SendMessageW, WINDOW_EX_STYLE,
    WM_COPYDATA, WNDCLASSEXW, WS_OVERLAPPED,
};

/// 单实例守卫：setup 中 settings_store 初始化后最早调用。
/// - 多开开关开启 → 直接放行（不建互斥）。
/// - 已有实例在跑 → 唤起其主窗口并退出本进程（code 0）。
/// - 首实例 → 注册唤起窗口后放行（互斥句柄泄漏持有至进程退出，维系命名互斥存活）。
#[cfg(target_os = "windows")]
pub fn guard(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // 多开开关：开启则跳过互斥（多实例并存）
    if settings_store::get("allow_multiple_instances")?.as_deref() == Some("true") {
        return Ok(());
    }

    // 命名互斥/事件窗口类名基于 identifier（如 com.fyshell.app），跨 debug/release 一致。
    // 字符串必须 NUL 结尾（Windows API 要求 C 字符串；缺 NUL 会读越界——
    // 落在零内存上会被当作空名 → 创建无名互斥 → 多实例永远互斥不上！）
    let identifier = app.config().identifier.clone();
    let mutex_name: Vec<u16> = format!("{identifier}-sim\0").encode_utf16().collect();
    let event_class: Vec<u16> = format!("{identifier}-sic\0").encode_utf16().collect();

    unsafe {
        // bInitialOwner=true：首实例立即拥有；第二实例打开已存在互斥并触发 ERROR_ALREADY_EXISTS
        let _mutex = CreateMutexW(None, true, PCWSTR(mutex_name.as_ptr()))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            // 已有一个实例在跑：经其隐藏事件窗口 WM_COPYDATA 唤起主窗口，然后退出本进程
            if let Ok(hwnd) = FindWindowW(PCWSTR(event_class.as_ptr()), PCWSTR::null()) {
                let cds = COPYDATASTRUCT {
                    dwData: 0,
                    cbData: 0,
                    lpData: std::ptr::null_mut(),
                };
                let _ = SendMessageW(
                    hwnd,
                    WM_COPYDATA,
                    None,
                    Some(LPARAM(&cds as *const COPYDATASTRUCT as isize)),
                );
            }
            std::process::exit(0);
        }
        // 首实例：注册隐藏事件窗口。`_mutex` 句柄有意不关闭（HANDLE 无 Drop，值是裸指针）——
        // 内核对象随之留在进程内、互斥命名持续有效；进程退出由 OS 统一回收，不建孤儿锁。
        init_event_window(app, &event_class)?;
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn guard(_app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // 非 Windows 暂不约束多实例（FyShell 目标平台为 Windows）
    Ok(())
}

/// 事件窗口句柄（供窗口过程唤起主窗口）
#[cfg(target_os = "windows")]
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

/// 注册隐藏事件窗口：专收 WM_COPYDATA（第二实例借此唤起本实例主窗口）。窗口不可见。
#[cfg(target_os = "windows")]
unsafe fn init_event_window(
    app: &tauri::AppHandle,
    class_name: &[u16],
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = APP.set(app.clone());
    let module = GetModuleHandleW(PCWSTR::null())?;
    let hinst = HINSTANCE(module.0);
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(event_proc),
        hInstance: hinst,
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };
    RegisterClassExW(&class);
    // 隐藏事件窗口：尺寸 0，不 ShowWindow → 永不可见
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        PCWSTR(class_name.as_ptr()),
        PCWSTR::null(),
        WS_OVERLAPPED,
        0,
        0,
        0,
        0,
        None,
        None,
        Some(hinst),
        None,
    )?;
    // 句柄仅需进程存活期间有效；显式引用防未用告警
    debug_assert!(!hwnd.is_invalid());
    Ok(())
}

/// 窗口过程：WM_COPYDATA = 第二实例请求唤起 → 显示并聚焦主窗口（与 tray.rs 唤起同逻辑）
#[cfg(target_os = "windows")]
unsafe extern "system" fn event_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_COPYDATA {
        if let Some(app) = APP.get() {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
            }
        }
        return LRESULT(1);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
