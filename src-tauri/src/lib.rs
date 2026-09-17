// FyShell 主库：builder 配置、插件注册、全局状态、命令注册

mod commands;
mod error;
mod models;
mod services;
mod state;
mod tray;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // single-instance 必须最先注册：重复启动时唤起已有主窗口
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            // 会话管理（commands/session.rs）
            commands::session::session_list,
            commands::session::session_save,
            commands::session::session_delete,
            commands::session::session_test,
            commands::session::folder_save,
            // SSH 终端（commands/ssh.rs）
            commands::ssh::ssh_connect,
            commands::ssh::ssh_disconnect,
            commands::ssh::ssh_write,
            commands::ssh::ssh_resize,
            commands::ssh::ssh_alive,
            commands::ssh::ssh_hostkey_accept,
            // 前端诊断日志（临时调试用）
            commands::debug::debug_log,
            // SFTP 传输（commands/sftp.rs）
            commands::sftp::sftp_list,
            commands::sftp::sftp_mkdir,
            commands::sftp::sftp_delete,
            commands::sftp::sftp_rename,
            commands::sftp::sftp_upload,
            commands::sftp::sftp_download,
            commands::sftp::transfer_list,
            commands::sftp::transfer_cancel,
            commands::sftp::transfer_clear,
            commands::sftp::local_list,
            commands::sftp::local_mkdir,
            commands::sftp::local_rename,
            commands::sftp::local_delete,
            // 服务器监控（commands/monitor.rs，P1）
            commands::monitor::monitor_start,
            commands::monitor::monitor_stop,
            commands::monitor::docker_list,
            commands::monitor::docker_operate,
            // 快捷命令（commands/quick_command.rs，P1）
            commands::quick_command::qc_list,
            commands::quick_command::qc_save_command,
            commands::quick_command::qc_save_folder,
            commands::quick_command::qc_delete,
            // SSH 隧道（commands/tunnel.rs，P1）
            commands::tunnel::tunnel_list,
            commands::tunnel::tunnel_save,
            commands::tunnel::tunnel_delete,
            commands::tunnel::tunnel_start,
            commands::tunnel::tunnel_stop,
            // 会话增强（commands/session_log.rs + commands/auth_profile.rs，P1）
            commands::session::session_clone,
            commands::session_log::session_log_toggle,
            commands::session_log::session_log_list,
            commands::session_log::session_log_read,
            commands::auth_profile::auth_profile_list,
            commands::auth_profile::auth_profile_save,
            commands::auth_profile::auth_profile_delete,
            // MySQL 基础（commands/mysql.rs，P1）
            commands::mysql::mysql_connect,
            commands::mysql::mysql_disconnect,
            commands::mysql::mysql_list_tables,
            commands::mysql::mysql_query,
            commands::mysql::mysql_execute,
            commands::mysql::mysql_begin,
            commands::mysql::mysql_commit,
            commands::mysql::mysql_rollback,
            // 表设计器（commands/mysql_design.rs，P2）
            commands::mysql_design::mysql_table_design_get,
            commands::mysql_design::mysql_table_design_save,
            // SQL 控制台（commands/mysql_console.rs，P2）
            commands::mysql_console::mysql_history_list,
            commands::mysql_console::mysql_history_search,
            commands::mysql_console::mysql_history_clear,
            commands::mysql_console::mysql_explain,
            commands::mysql_console::mysql_query_multi,
            // 数据编辑增强（commands/mysql_edit.rs，P2）
            commands::mysql_edit::mysql_edit_preview,
            commands::mysql_edit::mysql_update_row,
            commands::mysql_edit::mysql_update_rows,
            commands::mysql_edit::mysql_delete_row,
            // 导入导出（commands/mysql_io.rs，P2）
            commands::mysql_io::mysql_export,
            commands::mysql_io::mysql_import,
            // 数据库对象（commands/mysql_objects.rs）：视图/函数/过程/触发器/事件
            commands::mysql_objects::mysql_object_list,
            commands::mysql_objects::mysql_object_ddl,
            commands::mysql_objects::mysql_object_save,
            commands::mysql_objects::mysql_object_drop,
            // 用户管理（commands/mysql_user.rs）
            commands::mysql_user::mysql_user_list,
            commands::mysql_user::mysql_user_create,
            commands::mysql_user::mysql_user_drop,
            commands::mysql_user::mysql_user_grants,
            // 备份/自动运行（commands/mysql_backup.rs）
            commands::mysql_backup::mysql_backup,
            commands::mysql_backup::mysql_restore,
            commands::mysql_backup::mysql_backup_profile_list,
            commands::mysql_backup::mysql_backup_profile_save,
            commands::mysql_backup::mysql_backup_profile_delete,
            commands::mysql_backup::mysql_backup_run_list,
        ])
        .setup(|app| {
            // 会话配置存储：数据库路径经 Tauri API 获取（setup 在任何命令之前运行）
            let config_store = crate::services::config_store::ConfigStore::open(
                &app.path().app_data_dir()?,
            )?;
            app.manage(state::AppState::new(config_store));

            // P1 各服务的模块级注册表初始化（数据库路径注入）
            let data_dir = app.path().app_data_dir()?;
            crate::services::auth_profile::init(&data_dir)?;
            crate::services::session_log::init(&data_dir)?;
            crate::services::quick_command_store::init(&data_dir)?;
            crate::services::tunnel::init(&data_dir)?;
            crate::services::monitor::init(&data_dir)?;
            crate::services::mysql::init();
            // P2：SQL 控制台查询历史（SQLite，同 fyshell.db 独立 Connection）
            crate::services::mysql_console::init(&data_dir)?;
            // 备份档案 + 运行历史（SQLite，同 fyshell.db 独立 Connection）
            crate::services::mysql_backup::init(&data_dir)?;

            // 托盘：图标 + 菜单
            tray::init(app.handle())?;

            if let Some(window) = app.get_webview_window("main") {
                // 关闭窗口时隐藏到托盘而非退出
                tray::setup_close_to_tray(&window);

                // 主题 dark-light-auto：启动时读取系统主题并显式应用到窗口，
                // 之后保持该主题（深色主题全覆盖）
                if let Ok(theme) = window.theme() {
                    let _ = window.set_theme(Some(theme));
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("FyShell 启动失败");
}
