// FyShell 主库：builder 配置、插件注册、全局状态、命令注册

mod commands;
mod error;
mod models;
mod services;
mod single_instance;
mod state;
mod tray;

use tauri::Manager;

/// 数据目录解析优先级：
/// 1. `FYSHELL_DATA_DIR` 环境变量（非空时生效，高级用户指定数据目录）
/// 2. 便携模式（`--features portable` 编译期启用）：数据存 exe 同级 `data/` 目录（单文件版数据跟 exe 走）
/// 3. 默认：系统应用数据目录（安装包模式，Windows 为 %APPDATA%\\com.fyshell.app）
///
/// exe 同级存在 `data_dir.conf`（内容为目录路径）时优先于 2/3 生效（两种模式均可指定数据目录）。
fn resolve_data_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    if let Ok(dir) = std::env::var("FYSHELL_DATA_DIR") {
        if !dir.trim().is_empty() {
            return Ok(std::path::PathBuf::from(dir.trim().to_string()));
        }
    }
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    if let Some(exe_dir) = exe_dir {
        // data_dir.conf 指定数据目录（首行为目录路径，安装/便携两模式均可用）
        let conf = exe_dir.join("data_dir.conf");
        if conf.exists() {
            if let Ok(content) = std::fs::read_to_string(&conf) {
                let dir = content.trim();
                if !dir.is_empty() {
                    return Ok(std::path::PathBuf::from(dir.to_string()));
                }
            }
        }
        // 便携模式：数据存 exe 同级 data/ 目录（启动时自动创建）
        #[cfg(feature = "portable")]
        return Ok(exe_dir.join("data"));
    }
    // 默认：系统应用数据目录（安装包模式）
    Ok(app.path().app_data_dir()?)
}

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
            commands::session::session_reorder,
            // SSH 终端（commands/ssh.rs）
            commands::ssh::ssh_connect,
            commands::ssh::ssh_disconnect,
            commands::ssh::ssh_write,
            commands::ssh::ssh_resize,
            commands::ssh::ssh_hostkey_accept,
            commands::ssh::rzsz_respond,
            // 前端诊断日志（临时调试用）
            commands::debug::debug_log,
            // SFTP 传输（commands/sftp.rs）
            commands::sftp::sftp_list,
            commands::sftp::sftp_mkdir,
            commands::sftp::sftp_delete,
            commands::sftp::sftp_rename,
            commands::sftp::sftp_chmod,
            commands::sftp::sftp_read_text,
            commands::sftp::sftp_write_text,
            commands::sftp::sftp_upload,
            commands::sftp::sftp_download,
            commands::sftp::transfer_list,
            commands::sftp::transfer_cancel,
            commands::sftp::transfer_clear,
            commands::sftp::local_list,
            commands::sftp::local_list_roots,
            commands::sftp::local_pick_dialog,
            commands::sftp::local_save_dialog,
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
            commands::mysql::mysql_cli_exec,
            commands::mysql::mysql_begin,
            commands::mysql::mysql_commit,
            commands::mysql::mysql_rollback,
            // Redis 连接管理（commands/redis.rs）
            commands::redis::redis_connect,
            commands::redis::redis_disconnect,
            commands::redis::redis_test,
            commands::redis::redis_info,
            commands::redis::redis_select_db,
            commands::redis::redis_keys,
            commands::redis::redis_exec,
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
            // 工具级功能（commands/mysql_tools.rs）：数据传输/数据生成/数据同步/结构同步
            commands::mysql_tools::mysql_data_transfer,
            commands::mysql_tools::mysql_data_generate,
            commands::mysql_tools::mysql_data_sync,
            commands::mysql_tools::mysql_structure_sync,
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
            commands::mysql_db::mysql_db_edit,
            commands::mysql_db::mysql_db_find,
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
            // 删除全部用户数据（恢复到首次运行状态：用户数据表 + 会话日志）
            commands::settings::user_data_clear,
            // 托盘（commands/tray.rs）：关闭到托盘行为（托盘菜单/设置对话框双入口）
            commands::tray::tray_set_close_to_tray,
            // 配色方案文件导入导出（commands/color_scheme.rs）
            commands::color_scheme::scheme_read_file,
            commands::color_scheme::scheme_write_file,
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
            commands::session::session_reorder,
            // SSH 终端（commands/ssh.rs）
            commands::ssh::ssh_connect,
            commands::ssh::ssh_disconnect,
            commands::ssh::ssh_write,
            commands::ssh::ssh_resize,
            commands::ssh::ssh_hostkey_accept,
            commands::ssh::rzsz_respond,
            // 前端诊断日志（临时调试用）
            commands::debug::debug_log,
            // SFTP 传输（commands/sftp.rs）
            commands::sftp::sftp_list,
            commands::sftp::sftp_mkdir,
            commands::sftp::sftp_delete,
            commands::sftp::sftp_rename,
            commands::sftp::sftp_chmod,
            commands::sftp::sftp_read_text,
            commands::sftp::sftp_write_text,
            commands::sftp::sftp_upload,
            commands::sftp::sftp_download,
            commands::sftp::transfer_list,
            commands::sftp::transfer_cancel,
            commands::sftp::transfer_clear,
            commands::sftp::local_list,
            commands::sftp::local_list_roots,
            commands::sftp::local_pick_dialog,
            commands::sftp::local_save_dialog,
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
            commands::mysql::mysql_cli_exec,
            commands::mysql::mysql_begin,
            commands::mysql::mysql_commit,
            commands::mysql::mysql_rollback,
            // Redis 连接管理（commands/redis.rs）
            commands::redis::redis_connect,
            commands::redis::redis_disconnect,
            commands::redis::redis_test,
            commands::redis::redis_info,
            commands::redis::redis_select_db,
            commands::redis::redis_keys,
            commands::redis::redis_exec,
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
            // 工具级功能（commands/mysql_tools.rs）：数据传输/数据生成/数据同步/结构同步
            commands::mysql_tools::mysql_data_transfer,
            commands::mysql_tools::mysql_data_generate,
            commands::mysql_tools::mysql_data_sync,
            commands::mysql_tools::mysql_structure_sync,
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
            commands::mysql_db::mysql_db_edit,
            commands::mysql_db::mysql_db_find,
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
            // 删除全部用户数据（恢复到首次运行状态：用户数据表 + 会话日志）
            commands::settings::user_data_clear,
            // 托盘（commands/tray.rs）：关闭到托盘行为（托盘菜单/设置对话框双入口）
            commands::tray::tray_set_close_to_tray,
            // 配色方案文件导入导出（commands/color_scheme.rs）
            commands::color_scheme::scheme_read_file,
            commands::color_scheme::scheme_write_file,
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
            // 数据目录解析（环境变量/便携模式/data_dir.conf/系统默认，见 resolve_data_dir）
            let data_dir = resolve_data_dir(app.handle())?;
            std::fs::create_dir_all(&data_dir)?;

            // 会话配置存储：数据库路径经数据目录解析（setup 在任何命令之前运行）
            let config_store = crate::services::config_store::ConfigStore::open(&data_dir)?;
            app.manage(state::AppState::new(config_store));

            // P1 各服务的模块级注册表初始化（数据库路径注入）
            crate::services::auth_profile::init(&data_dir)?;
            crate::services::session_log::init(&data_dir)?;
            crate::services::quick_command_store::init(&data_dir)?;
            crate::services::tunnel::init(&data_dir)?;
            crate::services::mysql::init();
            // Redis 连接注册表（模块级 OnceLock，无需参数）
            crate::services::redis::init();
            // P2：SQL 控制台查询历史（SQLite，同 fyshell.db 独立 Connection）
            crate::services::mysql_console::init(&data_dir)?;
            // 备份档案 + 运行历史（SQLite，同 fyshell.db 独立 Connection）
            crate::services::mysql_backup::init(&data_dir)?;
            // 应用设置（SQLite，同 fyshell.db 独立 Connection，settings 表）
            crate::services::settings_store::init(&data_dir)?;
            // 单实例守卫：已读过设置（含多开开关）后尽早退出重复实例（唤起既有主窗口）；
            // 未开多开时后续 SFTP/键位/保险库等初始化可少碰 SQLite
            single_instance::guard(app.handle())?;
            // SFTP 收藏路径（SQLite，同 fyshell.db 独立 Connection，sftp_favorites 表）
            crate::services::sftp_store::init(&data_dir)?;
            // 键位映射（SQLite，同 fyshell.db 独立 Connection，key_mappings 表）
            crate::services::key_mapping_store::init(&data_dir)?;
            // 凭据保险库（SQLite meta 表独立 Connection：解密 DEK 信封）
            crate::services::vault::init(&data_dir)?;

            // 托盘：图标 + 菜单——仅「关闭到托盘」开启时创建（未开启不显示托盘图标，运行时随开关增减）
            if crate::services::settings_store::get("tray_close_to_tray")?.as_deref() == Some("true") {
                tray::ensure(app.handle())?;
            }

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
