//! SFTP 传输命令层：契约第 2 节的 10 个命令，薄层转发（参数校验 + 调用 services）。

use tauri::ipc::Channel;
use tauri::{Manager, State};

#[cfg(windows)]
use windows::Win32::Foundation::HWND;

use crate::error::AppError;
use crate::models::transfer::{FileEntry, TransferKind, TransferStatus, TransferTask};
use crate::services::{sftp as sftp_service, sftp_store, transfer as transfer_service};
use crate::state::AppState;

/// 目录列表：`Vec<FileEntry>`
#[tauri::command]
#[specta::specta]
pub async fn sftp_list(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> Result<Vec<FileEntry>, AppError> {
    sftp_service::list(&state, &id, &path).await
}

/// 创建目录
#[tauri::command]
#[specta::specta]
pub async fn sftp_mkdir(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> Result<(), AppError> {
    sftp_service::mkdir(&state, &id, &path).await
}

/// 删除（前端二次确认；目录递归删除）
#[tauri::command]
#[specta::specta]
pub async fn sftp_delete(
    state: State<'_, AppState>,
    id: String,
    path: String,
    is_dir: bool,
) -> Result<(), AppError> {
    sftp_service::delete(&state, &id, &path, is_dir).await
}

/// 重命名
#[tauri::command]
#[specta::specta]
pub async fn sftp_rename(
    state: State<'_, AppState>,
    id: String,
    old_path: String,
    new_path: String,
) -> Result<(), AppError> {
    sftp_service::rename(&state, &id, &old_path, &new_path).await
}

/// 设置文件/目录权限（chmod）：`mode` 为八进制语义数值（如 0o644 = 420）
#[tauri::command]
#[specta::specta]
pub async fn sftp_chmod(
    state: State<'_, AppState>,
    id: String,
    path: String,
    mode: u32,
) -> Result<(), AppError> {
    sftp_service::chmod(&state, &id, &path, mode).await
}

/// 读取远程文件内容为 UTF-8 文本（文本编辑用；≤2MB，非 UTF-8/二进制报错）
#[tauri::command]
#[specta::specta]
pub async fn sftp_read_text(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> Result<String, AppError> {
    sftp_service::read_text(&state, &id, &path).await
}

/// 写回远程文件内容（全量 TRUNCATE 覆盖，UTF-8）——文本编辑保存
#[tauri::command]
#[specta::specta]
pub async fn sftp_write_text(
    state: State<'_, AppState>,
    id: String,
    path: String,
    content: String,
) -> Result<(), AppError> {
    sftp_service::write_text(&state, &id, &path, &content).await
}

/// SFTP 收藏路径列表（按收藏时间倒序）
#[tauri::command]
#[specta::specta]
pub fn sftp_favorite_list() -> Result<Vec<sftp_store::SftpFavorite>, AppError> {
    sftp_store::list()
}

/// 收藏路径：按 (side, path) 幂等，返回含 id 的收藏记录
#[tauri::command]
#[specta::specta]
pub fn sftp_favorite_add(side: String, path: String) -> Result<sftp_store::SftpFavorite, AppError> {
    sftp_store::add(&side, &path)
}

/// 取消收藏：按侧 + 路径删除（未收藏时幂等）
#[tauri::command]
#[specta::specta]
pub fn sftp_favorite_remove(side: String, path: String) -> Result<(), AppError> {
    sftp_store::remove(&side, &path)
}

/// 入队上传，进度走 Channel（返回含 id 的任务快照，前端可凭其取消）
#[tauri::command]
#[specta::specta]
pub async fn sftp_upload(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    local_path: String,
    remote_path: String,
    on_progress: Channel<TransferTask>,
) -> Result<TransferTask, AppError> {
    transfer_service::enqueue(
        &app,
        &state,
        TransferKind::Upload,
        &id,
        local_path,
        remote_path,
        on_progress,
    )
}

/// 入队下载，进度走 Channel
#[tauri::command]
#[specta::specta]
pub async fn sftp_download(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    remote_path: String,
    local_path: String,
    on_progress: Channel<TransferTask>,
) -> Result<TransferTask, AppError> {
    transfer_service::enqueue(
        &app,
        &state,
        TransferKind::Download,
        &id,
        local_path,
        remote_path,
        on_progress,
    )
}

/// 传输队列快照：`Vec<TransferTask>`
#[tauri::command]
#[specta::specta]
pub fn transfer_list(state: State<'_, AppState>) -> Vec<TransferTask> {
    state
        .transfer_tasks
        .lock()
        .expect("transfer_tasks 锁被污染")
        .clone()
}

/// 取消任务：写入取消集合，传输循环每批检查命中即中止
#[tauri::command]
#[specta::specta]
pub fn transfer_cancel(state: State<'_, AppState>, task_id: String) -> Result<(), AppError> {
    state
        .cancelled_transfers
        .lock()
        .expect("cancelled_transfers 锁被污染")
        .insert(task_id);
    Ok(())
}

/// 清除已完成 / 失败 / 已取消记录（Queued / Running 保留）
#[tauri::command]
#[specta::specta]
pub fn transfer_clear(state: State<'_, AppState>) -> Result<(), AppError> {
    let removed: Vec<String> = {
        let tasks = state.transfer_tasks.lock().expect("transfer_tasks 锁被污染");
        let removed = tasks
            .iter()
            .filter(|t| {
                !matches!(
                    t.status,
                    TransferStatus::Queued | TransferStatus::Running
                )
            })
            .map(|t| t.id.clone())
            .collect();
        removed
    };
    {
        let mut tasks = state.transfer_tasks.lock().expect("transfer_tasks 锁被污染");
        tasks.retain(|t| matches!(t.status, TransferStatus::Queued | TransferStatus::Running));
    }
    transfer_service::remove_channels(&removed);
    Ok(())
}

/// 本地目录列表（供双栏左侧使用）；路径为 ".." 时返回上级目录列表
#[tauri::command]
#[specta::specta]
pub fn local_list(path: String) -> Result<Vec<FileEntry>, AppError> {
    // ".." 返回上级：基于进程当前目录解析
    let target = if path == ".." {
        std::env::current_dir()?
            .parent()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| AppError::general("已位于根目录"))?
    } else {
        std::path::PathBuf::from(&path)
    };

    let mut entries = Vec::new();

    // 非根目录时提供 ".." 条目（is_dir: true，供双栏左侧返回上级）
    if target.parent().is_some() {
        entries.push(FileEntry {
            name: "..".to_string(),
            is_dir: true,
            size: 0,
            modified_at: 0,
            permissions: String::new(),
        });
    }

    let read_dir = std::fs::read_dir(&target)?;
    for entry in read_dir {
        let entry = entry?;
        let meta = entry.metadata()?;
        let name = entry.file_name();
        // 防御性处理：read_dir 不产生 "." 与 ".."
        if name == "." || name == ".." {
            continue;
        }
        entries.push(FileEntry {
            is_dir: meta.is_dir(),
            size: meta.len(),
            modified_at: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            // Windows 本地无 POSIX 权限位，返回空串
            permissions: String::new(),
            name: name.to_string_lossy().into_owned(),
        });
    }

    // 目录优先 + 名称不区分大小写排序
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

/// 本地文件系统起点列表（供内置文件选择器作为初始视图）：
/// Windows 返回各盘符（name 形如 "C:"，前端拼 "C:/" 后调 local_list）；
/// 其他平台返回根 "/"。name 均以 is_dir=true 标记，语义与 local_list 一致。
#[tauri::command]
#[specta::specta]
pub fn local_list_roots() -> Result<Vec<FileEntry>, AppError> {
    #[cfg(windows)]
    {
        let mut roots = Vec::new();
        for c in b'A'..=b'Z' {
            let ch = c as char;
            let drive = format!("{ch}:\\");
            if std::path::Path::new(&drive).exists() {
                roots.push(FileEntry {
                    name: format!("{ch}:"),
                    is_dir: true,
                    size: 0,
                    modified_at: 0,
                    permissions: String::new(),
                });
            }
        }
        if roots.is_empty() {
            return Err(AppError::general("未检测到任何本地磁盘"));
        }
        Ok(roots)
    }
    #[cfg(not(windows))]
    {
        Ok(vec![FileEntry {
            name: "/".to_string(),
            is_dir: true,
            size: 0,
            modified_at: 0,
            permissions: String::new(),
        }])
    }
}

/// 自研调用 Windows 原生文件对话框（IFileOpenDialog，即资源管理器选择器）：
/// send=选单个文件 / recv=选目录；用户取消返回 None。
/// 可选 title（对话框标题）与 filter_name/filter_exts（文件扩展名过滤，如 json）。
///
/// 不走任何第三方插件（替代 tauri-plugin-dialog）：
/// 对话框经 run_on_main_thread 在主线程（有消息泵）模态展示，
/// 父窗口为主窗口，保证置顶与输入焦点正确。
#[tauri::command]
#[specta::specta]
pub fn local_pick_dialog(
    app: tauri::AppHandle,
    mode: String,
    title: Option<String>,
    filter_name: Option<String>,
    filter_exts: Option<Vec<String>>,
) -> Result<Option<String>, AppError> {
    let (tx, rx) = std::sync::mpsc::channel::<Result<Option<String>, AppError>>();
    let app2 = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(pick_dialog_native(&app2, mode.as_str(), title, filter_name, filter_exts));
    })
    .map_err(|e| AppError::general(format!("无法在主线程打开文件对话框: {e}")))?;
    rx.recv()
        .map_err(|_| AppError::general("文件对话框线程异常"))?
}

/// 自研调用 Windows 原生保存对话框（IFileSaveDialog，带覆盖确认）：
/// default_path 为默认文件名；用户取消返回 None。参数语义同 local_pick_dialog。
#[tauri::command]
#[specta::specta]
pub fn local_save_dialog(
    app: tauri::AppHandle,
    default_path: String,
    title: Option<String>,
    filter_name: Option<String>,
    filter_exts: Option<Vec<String>>,
) -> Result<Option<String>, AppError> {
    let (tx, rx) = std::sync::mpsc::channel::<Result<Option<String>, AppError>>();
    let app2 = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(save_dialog_native(
            &app2,
            &default_path,
            title,
            filter_name,
            filter_exts,
        ));
    })
    .map_err(|e| AppError::general(format!("无法在主线程打开保存对话框: {e}")))?;
    rx.recv()
        .map_err(|_| AppError::general("文件对话框线程异常"))?
}

/// 平台实现：Windows 用 IFileOpenDialog（COM），其他平台暂不支持（报错提示）。
#[cfg(windows)]
fn pick_dialog_native(
    app: &tauri::AppHandle,
    mode: &str,
    title: Option<String>,
    filter_name: Option<String>,
    filter_exts: Option<Vec<String>>,
) -> Result<Option<String>, AppError> {
    dialog_native(
        app,
        false,
        mode == "recv",
        title,
        None,
        filter_name,
        filter_exts,
    )
}

/// 平台实现：Windows 用 IFileSaveDialog（COM，带覆盖确认），其他平台报错。
#[cfg(windows)]
fn save_dialog_native(
    app: &tauri::AppHandle,
    default_path: &str,
    title: Option<String>,
    filter_name: Option<String>,
    filter_exts: Option<Vec<String>>,
) -> Result<Option<String>, AppError> {
    dialog_native(
        app,
        true,
        false,
        title,
        Some(default_path),
        filter_name,
        filter_exts,
    )
}

/// 共享原生对话框实现：open（IFileOpenDialog，pick_folders=true 选目录）/
/// save（IFileSaveDialog，覆盖确认）；均带可选标题、默认文件名与扩展名过滤。
#[cfg(windows)]
fn dialog_native(
    app: &tauri::AppHandle,
    save: bool,
    pick_folders: bool,
    title: Option<String>,
    default_path: Option<&str>,
    filter_name: Option<String>,
    filter_exts: Option<Vec<String>>,
) -> Result<Option<String>, AppError> {
    use windows::core::{HSTRING, HRESULT, PCWSTR};
    use windows::Win32::Foundation::ERROR_CANCELLED;
    use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::{
        FileOpenDialog, FileSaveDialog, FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT,
        FOS_PICKFOLDERS, IFileDialog, IFileOpenDialog, IFileSaveDialog, IShellItem,
        SIGDN_FILESYSPATH,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;

    use windows::Win32::UI::WindowsAndMessaging::{
        GetLastActivePopup, IsWindowVisible, LoadCursorW, SetCursor, SetForegroundWindow,
        SetWindowPos, ShowCursor, HWND_TOP, IDC_ARROW, SWP_NOMOVE, SWP_NOSIZE,
        SWP_SHOWWINDOW,
    };

    unsafe {
        // 弹框前恢复系统光标：部分环境（WebView2 宿主）在弹系统对话框时
        // 继承残留的隐藏光标状态导致指针不可见。ShowCursor 计数可能被
        // 压到负值（隐藏），循环递增直至 >= 0（可见）；再显式重置为箭头。
        let mut shown_count = ShowCursor(true);
        while shown_count < 0 && shown_count > -100 {
            shown_count = ShowCursor(true);
        }
        if let Ok(arrow) = LoadCursorW(None, IDC_ARROW) {
            let _ = SetCursor(Some(arrow));
        }

        let dialog: IFileDialog = if save {
            let d: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| AppError::general(format!("创建保存对话框失败: {e}")))?;
            d.into()
        } else {
            let d: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| AppError::general(format!("创建文件对话框失败: {e}")))?;
            d.into()
        };

        let mut opts = dialog.GetOptions().map_err(to_dialog_err)?;
        opts |= FOS_FORCEFILESYSTEM;
        if save {
            opts |= FOS_OVERWRITEPROMPT;
        } else if pick_folders {
            opts |= FOS_PICKFOLDERS;
        }
        dialog.SetOptions(opts).map_err(to_dialog_err)?;

        if let Some(t) = title {
            dialog.SetTitle(&HSTRING::from(t.as_str()))
                .map_err(to_dialog_err)?;
        }
        if let Some(name) = default_path {
            dialog.SetFileName(&HSTRING::from(name))
                .map_err(to_dialog_err)?;
        }
        // 扩展名过滤：多条扩展名拼成 "*.a;*.b" 单组 spec（保持简单，不分组下拉）
        if let (Some(fname), Some(exts)) = (filter_name, filter_exts) {
            if !exts.is_empty() {
                let spec = exts.iter().map(|e| format!("*.{e}")).collect::<Vec<_>>().join(";");
                let hs_name = HSTRING::from(fname.as_str());
                let hs_spec = HSTRING::from(spec.as_str());
                let spec_arr = [COMDLG_FILTERSPEC {
                    pszName: PCWSTR(hs_name.as_ptr()),
                    pszSpec: PCWSTR(hs_spec.as_ptr()),
                }];
                dialog.SetFileTypes(&spec_arr).map_err(to_dialog_err)?;
            }
        }

        // 用户取消：ERROR_CANCELLED -> HRESULT 0x800704C7，视为正常返回
        // Show 前起辅助线程：Show() 是模态循环阻塞至对话框关闭，无法在
        // Show 返回后再置前。根因：run_on_main_thread 的消息处理上下文里
        // 弹出的对话框无法自行激活（Windows 前台激活限制），会开在主窗口
        // 后面——模态挡住输入且不可见（指针「丢失」）。辅助线程等对话框
        // 出现后用 GetLastActivePopup 找到它并强制提到最前。
        let owner = main_hwnd(app);
        let owner_raw = owner.map(|h| h.0 as isize);
        let helper = std::thread::spawn(move || {
            if let Some(raw) = owner_raw {
                let owner = HWND(raw as *mut core::ffi::c_void);
                unsafe {
                    for _ in 0..20 {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        let dlg = GetLastActivePopup(owner);
                        if dlg.0 != owner.0 && IsWindowVisible(dlg).as_bool() {
                            let _ = SetWindowPos(
                                dlg,
                                Some(HWND_TOP),
                                0,
                                0,
                                0,
                                0,
                                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                            );
                            let _ = SetForegroundWindow(dlg);
                            break;
                        }
                    }
                }
            }
        });

        if let Err(e) = dialog.Show(owner) {

            let _ = helper.join();
            if e.code() == HRESULT::from_win32(ERROR_CANCELLED.0) {
                return Ok(None);
            }
            return Err(to_dialog_err(e));
        }
        let _ = helper.join();

        let item: IShellItem = dialog.GetResult().map_err(to_dialog_err)?;
        let path = item.GetDisplayName(SIGDN_FILESYSPATH).map_err(to_dialog_err)?;
        let s = path
            .to_string()
            .map_err(|_| AppError::general("文件路径解码失败"))?;
        // GetDisplayName 返回的缓冲由调用方释放
        CoTaskMemFree(Some(path.0 as *const core::ffi::c_void));
        Ok(Some(s))
    }
}

#[cfg(windows)]
fn to_dialog_err(e: windows::core::Error) -> AppError {
    AppError::general(format!("文件对话框操作失败: {e}"))
}

/// 主窗口 HWND（模态对话框的父窗口）
#[cfg(windows)]
fn main_hwnd(app: &tauri::AppHandle) -> Option<HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::HWND;
    let win = app.get_webview_window("main")?;
    let handle = win.window_handle().ok()?;
    let raw = handle.as_raw();
    if let RawWindowHandle::Win32(w) = raw {
        Some(HWND(w.hwnd.get() as *mut core::ffi::c_void))
    } else {
        None
    }
}

/// 非 Windows 平台：原生对话框不可用（FyShell 目标平台为 Windows）
#[cfg(not(windows))]
fn pick_dialog_native(
    _app: &tauri::AppHandle,
    _mode: &str,
    _title: Option<String>,
    _filter_name: Option<String>,
    _filter_exts: Option<Vec<String>>,
) -> Result<Option<String>, AppError> {
    Err(AppError::general("原生文件对话框仅在 Windows 平台可用"))
}

/// 非 Windows 平台：原生保存对话框不可用
#[cfg(not(windows))]
fn save_dialog_native(
    _app: &tauri::AppHandle,
    _default_path: &str,
    _title: Option<String>,
    _filter_name: Option<String>,
    _filter_exts: Option<Vec<String>>,
) -> Result<Option<String>, AppError> {
    Err(AppError::general("原生保存对话框仅在 Windows 平台可用"))
}

/// 本地新建文件夹（fe-sftp 双栏本地侧）
#[tauri::command]
#[specta::specta]
pub fn local_mkdir(path: String) -> Result<(), AppError> {
    std::fs::create_dir_all(&path)?;
    Ok(())
}

/// 本地重命名
#[tauri::command]
#[specta::specta]
pub fn local_rename(old_path: String, new_path: String) -> Result<(), AppError> {
    std::fs::rename(&old_path, &new_path)?;
    Ok(())
}

/// 本地删除：is_dir 为 true 时递归删除目录，否则删除文件
#[tauri::command]
#[specta::specta]
pub fn local_delete(path: String, is_dir: bool) -> Result<(), AppError> {
    if is_dir {
        std::fs::remove_dir_all(&path)?;
    } else {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}
