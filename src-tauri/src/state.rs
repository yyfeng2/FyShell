//! 全局应用状态：经 `app.manage(AppState)` 注册，
//! commands / services 层通过 `tauri::State<AppState>` 访问。

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

pub struct AppState {
    /// 已建立的 SSH 会话句柄，key = 会话 id（uuid）
    pub ssh_sessions: Mutex<HashMap<String, crate::services::ssh::SshSessionHandle>>,

    /// 传输队列快照（含已完成 / 失败记录，供 transfer_list 返回）
    pub transfer_tasks: Mutex<Vec<crate::models::transfer::TransferTask>>,

    /// 已取消的传输任务 id 集合：传输循环每写一批检查一次，命中即中止
    pub cancelled_transfers: Mutex<HashSet<String>>,

    /// HostKey 首次确认挂起点，key = 会话 id。
    /// ssh_connect 遇到未信任主机时把 oneshot::Sender 存入此处，
    /// ssh_hostkey_accept 取出后 send(bool)，实现前端确认前的异步挂起。
    pub pending_hostkey: Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>,

    /// 本地终端会话句柄（portable-pty），key = 连接路由键（每标签唯一）
    pub local_sessions: Mutex<HashMap<String, crate::services::local_shell::LocalShellHandle>>,

    /// Telnet 会话句柄，key = 连接路由键（每标签唯一）
    pub telnet_sessions: Mutex<HashMap<String, crate::services::telnet::TelnetSessionHandle>>,

    /// 串口会话句柄，key = 连接路由键（每标签唯一）
    pub serial_sessions: Mutex<HashMap<String, crate::services::serial::SerialSessionHandle>>,

    /// 会话配置持久化存储（SQLite，内部自带 Mutex<Connection>，方法为同步签名）
    pub config_store: crate::services::config_store::ConfigStore,

    /// 进行中的终端 rz/sz 文件传输，key = 连接路由键（每标签唯一）。
    /// read_loop 检测到 ZRQINIT 哨兵时写入（输出改道到条目里的管道），
    /// 传输任务结束时移除（读循环恢复常规转发）。
    pub rzsz_sessions: Mutex<HashMap<String, crate::services::rzsz::RzszEntry>>,
}

impl AppState {
    /// config_store 由 lib.rs setup 中经 `ConfigStore::open(app_data_dir)` 创建后注入
    ///（数据库路径依赖 Tauri API，只能在 app 启动后获取）
    pub fn new(config_store: crate::services::config_store::ConfigStore) -> Self {
        Self {
            ssh_sessions: Mutex::new(HashMap::new()),
            transfer_tasks: Mutex::new(Vec::new()),
            cancelled_transfers: Mutex::new(HashSet::new()),
            pending_hostkey: Mutex::new(HashMap::new()),
            local_sessions: Mutex::new(HashMap::new()),
            telnet_sessions: Mutex::new(HashMap::new()),
            serial_sessions: Mutex::new(HashMap::new()),
            config_store,
            rzsz_sessions: Mutex::new(HashMap::new()),
        }
    }
}

// AppState 不实现 Default：构造必须显式传入 config_store（数据库路径经 setup 注入）
