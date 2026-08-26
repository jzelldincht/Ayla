//! Alya 应用装配（T1 脚手架骨架）。
//!
//! 本文件仅承载：插件注册顺序（占位注释）、模块声明与启动序列占位；
//! 各模块的业务逻辑由对应任务实现（T2~T16，见各占位文件首行职责注释与 spec 5.4 模块划分）。
//! 架构不变量（INV-1~INV-8）与模块依赖方向规则见 spec 5.4，后续任务不得越过。

pub mod action_router;
pub mod capture;
pub mod clipboard_monitor;
pub mod config;
pub mod error;
pub mod events;
pub mod hotkey;
pub mod ocr;
pub mod selection;
pub mod translate;
pub mod tray;
pub mod windows;

/// 应用入口：装配插件并运行事件循环。
///
/// 插件注册顺序（T1 占位；后续任务只填充各自位置的实现，保持顺序不变）：
/// 1. `tauri_plugin_log` —— 最先注册，后续初始化日志可落文件（spec 7.4）；
/// 2. `tauri_plugin_single_instance` —— 互斥体尽早生效，杜绝双实例竞态（spec 5.7.9）；
/// 3. `tauri_plugin_global_shortcut` —— 全局热键注册（T4，spec 5.7.1）；
/// 4. `tauri_plugin_autostart` —— 开机自启（T15，spec 5.7.9）；
/// 5. `tauri_plugin_notification` —— 系统通知（EX-7/EX-9/EX-10/D3 呈现层，spec 5.3）。
///
/// 启动序列（T4/T5 已接入 1~3 步；其余留待对应任务）：
/// 1. 配置加载（T2 模块接入）→ 2. 热键注册（T4）→ 3. 剪贴板监听启动（T5，FR-5.3 恢复上次状态）
///    → 4. 托盘初始化（T15）→ 5. 单实例 pid 缓存（T15）→ 6. translate 暖窗预创建（T11）。
pub fn run() {
    let app = match tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {
            // T15：D7 前台握手（AllowSetForegroundWindow）+ EX-12 唤起 input_translate。
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            hotkey::rebind_hotkey, // T4：改键专用（回退语义，spec 5.7.1）
            clipboard_monitor::set_clipboard_monitor, // T5：三方开关统一入口（FR-5.1，INV-8）
        ])
        .setup(|app| {
            // 启动序列 1：配置加载（T2 模块；原子写/迁移桩在 config::load_from 内完成）。
            let settings = config::load_from(&config::app_config_dir(app.handle())?)?;
            config::init_global(settings)?;
            // 启动序列 2：五项热键逐一注册 + EX-7 失败聚合通知（T4）。
            hotkey::init(app.handle())?;
            // 启动序列 3：剪贴板监听按持久化开关恢复（T5；失败不阻断启动，仅记录）。
            clipboard_monitor::start_if_enabled(app.handle());

            // TODO(T16)：action_router 接线——hotkey::set_callback +
            //   clipboard_monitor::set_translate_trigger（依赖方向 action_router→能力模块，spec 5.4）。
            // TODO(T11/T15)：托盘初始化（tray.rs）、单实例 pid 缓存、translate 暖窗预创建。
            Ok(())
        })
        .build(tauri::generate_context!())
    {
        Ok(app) => app,
        Err(err) => {
            eprintln!("alya: 应用启动失败：{err}");
            std::process::exit(1);
        }
    };

    app.run(|_app, _event| {
        // T16：事件循环钩子（可选），当前骨架无业务逻辑。
    });
}
