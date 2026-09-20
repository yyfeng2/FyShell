//! 系统托盘：图标 + 菜单（显示主窗口 / 设置 / 托盘 / 退出）+ 关闭窗口按设置隐藏到托盘

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

/// 关闭到托盘运行时状态（默认 false = 关闭窗口即退出；托盘菜单/设置对话框双入口修改）
static CLOSE_TO_TRAY: AtomicBool = AtomicBool::new(false);

/// 托盘菜单「托盘」勾选项引用（设置对话框修改时同步勾选态）
static TRAY_TOGGLE_ITEM: Mutex<Option<CheckMenuItem<tauri::Wry>>> = Mutex::new(None);

/// 创建托盘图标与菜单。
/// 图标取应用默认窗口图标（`app.default_window_icon()`），缺省时优雅降级（托盘仍可用但无图标）。
pub fn init(app: &AppHandle) -> tauri::Result<()> {
    // 菜单项：显示主窗口 / 设置 / 托盘（勾选=关闭时隐藏到托盘）/ 退出
    let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let tray_toggle_item = CheckMenuItem::with_id(
        app,
        "tray-toggle",
        "托盘",
        true,
        CLOSE_TO_TRAY.load(Ordering::Relaxed),
        None::<&str>,
    )?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &settings_item, &tray_toggle_item, &quit_item])?;
    // 保存勾选项引用（设置对话框修改时同步勾选态）
    *TRAY_TOGGLE_ITEM.lock().expect("tray toggle 锁中毒") = Some(tray_toggle_item);

    let mut builder = TrayIconBuilder::with_id("fyshell-tray")
        .tooltip("FyShell")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "settings" => {
                show_main_window(app);
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.emit("tray-settings-requested", ());
                }
            }
            "tray-toggle" => {
                // 托盘勾选：切换关闭到托盘行为，并通知前端同步设置 store（持久化）
                let enabled = !CLOSE_TO_TRAY.load(Ordering::Relaxed);
                set_close_to_tray(enabled);
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.emit("tray-close-to-tray-changed", enabled);
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // 左键单击托盘图标：唤起主窗口
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    // 图标可能为空（图标资源缺失 / 未打包），优雅降级：不设置图标，托盘功能不受影响
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

/// 设置关闭到托盘行为（运行时状态 + 托盘菜单勾选态同步）；设置对话框经命令调用
pub fn set_close_to_tray(enabled: bool) {
    CLOSE_TO_TRAY.store(enabled, Ordering::Relaxed);
    if let Some(item) = TRAY_TOGGLE_ITEM.lock().expect("tray toggle 锁中毒").as_ref() {
        let _ = item.set_checked(enabled);
    }
}

/// 关闭窗口行为：托盘开启时隐藏到托盘（拦截 CloseRequested）；默认关闭即退出
pub fn setup_close_to_tray(window: &WebviewWindow) {
    let win = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            if !CLOSE_TO_TRAY.load(Ordering::Relaxed) {
                return; // 默认：关闭即退出（不拦截）
            }
            api.prevent_close();
            let _ = win.hide();
        }
    });
}

/// 显示并聚焦主窗口
fn show_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}
