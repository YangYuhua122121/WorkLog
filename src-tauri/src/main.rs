// 发布版在 Windows 上不弹控制台
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    worklog_lib::run()
}
