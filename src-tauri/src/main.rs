// 发布构建下隐藏控制台窗口（Tauri 桌面应用约定）。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    alya::run();
}
