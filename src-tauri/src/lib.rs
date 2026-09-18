// FyShell 主库：builder 配置、插件注册、全局状态、命令注册

mod commands;
mod error;
mod models;
mod services;
mod state;
mod tray;

use tauri::Manager;

// IPC 类型绑定自动生成（tauri-specta）：debug 构建时导出 TS 绑定到 ../src/bindings.ts，
// bindings.ts 即 IPC 类型的权威来源；src/api/*.ts 手写封装保持可用。
// 下方 generate_handler 保持全部命令注册与功能不变，Builder 仅负责收集命令类型元数据并导出。
#[cfg(debug_assertions)]
fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
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
            commands::ssh::ssh_hostkey_accept,
            // 前端诊断日志（临时调试用）
            commands::debug::debug_log,
            // SFTP 传输（commands/sftp.rs）
            commands::sftp::sftp_list,
            commands::sftp::sftp_mkdir,
            commands::sftp::sftp_delete,
            commands::sftp::sftp_rename,
            commands::sftp::sftp_chmod,
            commands::sftp::sftp_upload,
            commands::sftp::sftp_download,
            commands::sftp::transfer_list,
            commands::sftp::transfer_cancel,
            commands::sftp::transfer_clear,
            commands::sftp::local_list,
            commands::sftp::local_mkdir,
            commands::sftp::local_rename,
            commands::sftp::local_delete,
            // SFTP 收藏路径（commands/sftp.rs，SQLite sftp_favorites 表）
            commands::sftp::sftp_favorite_list,
            commands::sftp::sftp_favorite_add,
            commands::sftp::sftp_favorite_remove,
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
            commands::tunnel::tunnel_start_all,
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
            commands::mysql_console::mysql_saved_query_list,
            commands::mysql_console::mysql_saved_query_save,
            commands::mysql_console::mysql_saved_query_rename,
            commands::mysql_console::mysql_saved_query_delete,
            // 数据编辑增强（commands/mysql_edit.rs，P2）
            commands::mysql_edit::mysql_edit_preview,
            commands::mysql_edit::mysql_update_rows,
            commands::mysql_edit::mysql_delete_row,
            commands::mysql_edit::mysql_insert_rows,
            // 导入导出（commands/mysql_io.rs，P2）
            commands::mysql_io::mysql_export,
            commands::mysql_io::mysql_import,
            // 数据库对象（commands/mysql_objects.rs）：视图/函数/过程/触发器/事件
            commands::mysql_objects::mysql_object_list,
            commands::mysql_objects::mysql_object_ddl,
            commands::mysql_objects::mysql_object_save,
            commands::mysql_objects::mysql_object_drop,
            // 数据库级管理（commands/mysql_db.rs）：切换/新建/删除库 + 表快捷操作
            commands::mysql_db::mysql_db_list,
            commands::mysql_db::mysql_db_create,
            commands::mysql_db::mysql_db_drop,
            commands::mysql_db::mysql_db_switch,
            commands::mysql_db::mysql_table_show_create,
            commands::mysql_db::mysql_table_optimize,
            commands::mysql_db::mysql_table_rename,
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
            // 主密码（commands/master_password.rs，P0 预留接口接线）
            commands::master_password::master_password_status,
            commands::master_password::master_password_set,
            commands::master_password::master_password_verify,
            // 凭据保险库（commands/vault.rs，主密码保护已存连接密码）
            commands::vault::vault_status,
            commands::vault::vault_unlock,
            commands::vault::vault_lock,
            commands::vault::vault_encrypt,
            commands::vault::vault_decrypt,
            // 高功能设置（commands/settings.rs）：设置项持久化（SQLite settings 表）
            commands::settings::settings_get_all,
            commands::settings::settings_get,
            commands::settings::settings_set,
            // SSH 选项会话级覆盖（commands/settings.rs，sshopt_session_{session_id}_{key}）
            commands::settings::sshopt_session_list,
            commands::settings::sshopt_session_set,
            commands::settings::sshopt_session_delete,
            // 键位映射（commands/key_mapping.rs，SQLite key_mappings 表）
            commands::key_mapping::key_mapping_list,
            commands::key_mapping::key_mapping_save,
            commands::key_mapping::key_mapping_delete,
            // 本地终端（commands/local_shell.rs）
            commands::local_shell::local_shell_connect,
            commands::local_shell::local_shell_disconnect,
            commands::local_shell::local_shell_write,
            commands::local_shell::local_shell_resize,
            commands::local_shell::local_shell_alive,
            // Telnet 终端（commands/telnet.rs）
            commands::telnet::telnet_connect,
            commands::telnet::telnet_disconnect,
            commands::telnet::telnet_write,
            commands::telnet::telnet_alive,
            // 串口终端（commands/serial.rs）
            commands::serial::serial_list,
            commands::serial::serial_connect,
            commands::serial::serial_disconnect,
            commands::serial::serial_write,
            commands::serial::serial_alive,
        ])
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
        // 模型含 u64/i64 字段（字节数/耗时/时间戳），TS 侧统一为 number
        //（JS number 安全整数上限 2^53，远超实际取值范围）
        .dangerously_cast_bigints_to_number()
}

// debug 构建时生成 bindings.ts（与 tauri dev 启动路径共用，测试用例可单独触发）
#[cfg(debug_assertions)]
fn export_ipc_bindings(builder: &tauri_specta::Builder<tauri::Wry>) {
    builder
        .export(
            specta_typescript::Typescript::default(),
            concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts"),
        )
        .expect("导出 TypeScript IPC 绑定失败");
}

#[cfg(debug_assertions)]
#[test]
fn export_ipc_bindings_snapshot() {
    export_ipc_bindings(&specta_builder());
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // bindings.ts 编译期导出：任何命令之前完成，保证前后端类型同步
    #[cfg(debug_assertions)]
    export_ipc_bindings(&specta_builder());

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
            commands::ssh::ssh_hostkey_accept,
            // 前端诊断日志（临时调试用）
            commands::debug::debug_log,
            // SFTP 传输（commands/sftp.rs）
            commands::sftp::sftp_list,
            commands::sftp::sftp_mkdir,
            commands::sftp::sftp_delete,
            commands::sftp::sftp_rename,
            commands::sftp::sftp_chmod,
            commands::sftp::sftp_upload,
            commands::sftp::sftp_download,
            commands::sftp::transfer_list,
            commands::sftp::transfer_cancel,
            commands::sftp::transfer_clear,
            commands::sftp::local_list,
            commands::sftp::local_mkdir,
            commands::sftp::local_rename,
            commands::sftp::local_delete,
            // SFTP 收藏路径（commands/sftp.rs，SQLite sftp_favorites 表）
            commands::sftp::sftp_favorite_list,
            commands::sftp::sftp_favorite_add,
            commands::sftp::sftp_favorite_remove,
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
            commands::tunnel::tunnel_start_all,
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
            commands::mysql_console::mysql_saved_query_list,
            commands::mysql_console::mysql_saved_query_save,
            commands::mysql_console::mysql_saved_query_rename,
            commands::mysql_console::mysql_saved_query_delete,
            // 数据编辑增强（commands/mysql_edit.rs，P2）
            commands::mysql_edit::mysql_edit_preview,
            commands::mysql_edit::mysql_update_rows,
            commands::mysql_edit::mysql_delete_row,
            commands::mysql_edit::mysql_insert_rows,
            // 导入导出（commands/mysql_io.rs，P2）
            commands::mysql_io::mysql_export,
            commands::mysql_io::mysql_import,
            // 数据库对象（commands/mysql_objects.rs）：视图/函数/过程/触发器/事件
            commands::mysql_objects::mysql_object_list,
            commands::mysql_objects::mysql_object_ddl,
            commands::mysql_objects::mysql_object_save,
            commands::mysql_objects::mysql_object_drop,
            // 数据库级管理（commands/mysql_db.rs）：切换/新建/删除库 + 表快捷操作
            commands::mysql_db::mysql_db_list,
            commands::mysql_db::mysql_db_create,
            commands::mysql_db::mysql_db_drop,
            commands::mysql_db::mysql_db_switch,
            commands::mysql_db::mysql_table_show_create,
            commands::mysql_db::mysql_table_optimize,
            commands::mysql_db::mysql_table_rename,
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
            // 主密码（commands/master_password.rs，P0 预留接口接线）
            commands::master_password::master_password_status,
            commands::master_password::master_password_set,
            commands::master_password::master_password_verify,
            // 凭据保险库（commands/vault.rs，主密码保护已存连接密码）
            commands::vault::vault_status,
            commands::vault::vault_unlock,
            commands::vault::vault_lock,
            commands::vault::vault_encrypt,
            commands::vault::vault_decrypt,
            // 高功能设置（commands/settings.rs）：设置项持久化（SQLite settings 表）
            commands::settings::settings_get_all,
            commands::settings::settings_get,
            commands::settings::settings_set,
            // SSH 选项会话级覆盖（commands/settings.rs，sshopt_session_{session_id}_{key}）
            commands::settings::sshopt_session_list,
            commands::settings::sshopt_session_set,
            commands::settings::sshopt_session_delete,
            // 键位映射（commands/key_mapping.rs，SQLite key_mappings 表）
            commands::key_mapping::key_mapping_list,
            commands::key_mapping::key_mapping_save,
            commands::key_mapping::key_mapping_delete,
            // 本地终端（commands/local_shell.rs）
            commands::local_shell::local_shell_connect,
            commands::local_shell::local_shell_disconnect,
            commands::local_shell::local_shell_write,
            commands::local_shell::local_shell_resize,
            commands::local_shell::local_shell_alive,
            // Telnet 终端（commands/telnet.rs）
            commands::telnet::telnet_connect,
            commands::telnet::telnet_disconnect,
            commands::telnet::telnet_write,
            commands::telnet::telnet_alive,
            // 串口终端（commands/serial.rs）
            commands::serial::serial_list,
            commands::serial::serial_connect,
            commands::serial::serial_disconnect,
            commands::serial::serial_write,
            commands::serial::serial_alive,
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
            crate::services::mysql::init();
            // P2：SQL 控制台查询历史（SQLite，同 fyshell.db 独立 Connection）
            crate::services::mysql_console::init(&data_dir)?;
            // 备份档案 + 运行历史（SQLite，同 fyshell.db 独立 Connection）
            crate::services::mysql_backup::init(&data_dir)?;
            // 应用设置（SQLite，同 fyshell.db 独立 Connection，settings 表）
            crate::services::settings_store::init(&data_dir)?;
            // SFTP 收藏路径（SQLite，同 fyshell.db 独立 Connection，sftp_favorites 表）
            crate::services::sftp_store::init(&data_dir)?;
            // 键位映射（SQLite，同 fyshell.db 独立 Connection，key_mappings 表）
            crate::services::key_mapping_store::init(&data_dir)?;
            // 凭据保险库（SQLite meta 表独立 Connection：解密 DEK 信封）
            crate::services::vault::init(&data_dir)?;

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
