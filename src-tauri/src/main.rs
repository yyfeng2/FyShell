// FyShell 入口：极简，仅转发到 lib.rs 的 run()
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    fyshell_lib::run()
}
