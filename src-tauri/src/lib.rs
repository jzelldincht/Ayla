//! Alya 应用装配（T1 脚手架骨架）。
//!
//! 本文件仅承载：插件注册顺序（占位注释）、模块声明与启动序列占位；
//! 各模块的业务逻辑由对应任务实现（T2~T16，见各占位文件首行职责注释与 spec 5.4 模块划分）。
//! 架构不变量（INV-1~INV-8）与模块依赖方向规则见 spec 5.4，后续任务不得越过。

pub mod action_router;
pub mod autostart;
pub mod capture;
pub mod clipboard_monitor;
pub mod config;
pub mod error;
pub mod events;
pub mod hotkey;
pub mod instance;
pub mod ocr;
pub mod selection;
pub mod translate;
pub mod tray;
pub mod ui_text;
pub mod windows;

/// 应用入口：装配插件并运行事件循环。
///
/// 插件注册顺序（T1 既定；后续任务只填充各自位置的实现，保持顺序不变）：
/// 1. `tauri_plugin_log` —— 最先注册，后续初始化日志可落文件（spec 7.4）；
/// 2. `tauri_plugin_single_instance` —— 互斥体尽早生效，杜绝双实例竞态（spec 5.7.9）；
/// 3. `tauri_plugin_global_shortcut` —— 全局热键注册（T4，spec 5.7.1）；
/// 4. `tauri_plugin_autostart` —— 开机自启（T15，spec 5.7.9；HKCU Run 真实接线见 autostart.rs）；
/// 5. `tauri_plugin_notification` —— 系统通知（EX-7/EX-9/EX-10/D3 呈现层，spec 5.3）。
///
/// 启动序列（T15 起完整装配；各步失败处理见步骤内注释）：
/// 0.（run 最早段，build 之前）D7 前台握手（instance::handshake——二次实例在插件 setup 内即
///    `std::process::exit(0)`，授权必须发生在 build 之前）→
/// 1. 配置加载（T2）→ 1.5. instance.pid 写入（T15，D7——取最早可用落点，最小化「二次启动早于
///    pid 落盘」竞态窗口；失败记日志不阻断）→ 2. 热键注册（T4）→
/// 3. 剪贴板监听启动（T5，FR-5.3 恢复上次状态）→ 4. 托盘初始化（T15，失败阻断——
///    托盘是 FR-1.3 常驻形态与唯一退出路径的承载）→ 5. translate 暖窗预创建（T11，D5）→
/// 6. 首启引导检测并显示（T14，FR-11.1/11.5）。
pub fn run() {
    // === 启动 0：D7 前台握手（新实例侧；须在任何 Builder 装配之前——见 instance.rs 模块头） ===
    match instance::default_pid_dir() {
        Some(dir) => {
            instance::handshake(&dir, std::process::id());
        }
        None => log::debug!(
            "instance: APPDATA 环境变量缺失（异常环境），跳过 D7 前台握手（双实例唤起将走无焦点降级）"
        ),
    }

    let app = match tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                // spec 7.4：Debug 级含性能打点（hotkey_received/selection_done/panel_visible），
                // 发布默认关（Info）；dev 构建（debug_assertions）开启 Debug 支撑 J1 工程实测。
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // EX-12（spec 5.7.9）：互斥体已存在——不创建第二进程（二次实例已在插件 setup 内退场），
            // 唤起既有实例呼出输入翻译面板（Q8：与托盘左键同语义）。
            // argv 仅作来源打点（二次启动的命令行参数，不经任何凭据通道；INV-7 不记内容）。
            // Info 级：二次启动唤起属用户可见事件（spec 7.4 日志 Info 级口径）。
            log::info!(
                "single_instance: 二次启动唤起（argv_len={}）→ input_translate（FR-1.4/EX-12）",
                argv.len()
            );
            action_router::dispatch(app, action_router::Action::InputTranslate);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            hotkey::rebind_hotkey,                // T4：改键专用（回退语义，spec 5.7.1）
            hotkey::hotkey_registration_failures, // T13 新增：设置页快捷键行尾 warning 图标数据源
            clipboard_monitor::set_clipboard_monitor, // T5：三方开关统一入口（FR-5.1，INV-8）
            windows::mask::complete_selection,    // T9：框选确认（6.1；无效选区按 Q11 视同取消）
            windows::mask::cancel_selection,      // T9：Esc 取消（6.1；无残留 FR-8.3/8.6）
            windows::mask::get_mask_snapshot, // T9 追加（主 Agent 批准）：遮罩页拉取冻结帧（data: URL）
            ocr::list_ocr_languages,          // T10：已装 OCR 语言包清单（6.1；设置下拉数据源）
            translate::set_panel_target_lang, // T12：面板临时目标语言（FR-2.7 易失槽，最小契约扩展）
            translate::verify::verify_connection_cmd, // T14：连接验证（6.1；rename="verify_connection"，D6）
            windows::settings::get_settings,          // T13：设置读取（6.1；INV-1 仅 hasKey）
            windows::settings::set_settings, // T13：通用设置即时保存（6.1；热键/监听走专通道）
            windows::settings::open_settings, // T13：设置窗单例前置（6.1；EX-2 入口）
            windows::onboarding::open_config_guide, // T14 新增：FR-11.4 配置引导（guide 模式复用窗体）
            windows::onboarding::finish_onboarding, // T14：完成/跳过写 meta.onboardingCompleted
            action_router::translate, // T16：面板提交（6.1；FR-11.4 门控+EX-13 单路径）
            action_router::cancel_translation, // T16：显式取消（6.1；Esc 中止，design-spec 3.5）
            action_router::reocr,     // T16：截图换语种重识别（6.1/FR-6.2）
            action_router::send_ocr_to_translate, // T16：截图一键送翻（6.1/FR-6.5）
            windows::translate::panel_hide, // 真机修复轮 2：关闭=隐藏（窗口插件 API 被 ACL 拒绝）
            windows::translate::panel_start_drag, // 真机修复轮 2：顶栏拖拽
            windows::translate::panel_resize_for_content, // 真机修复轮 13：译文内容自适应高度（不持久化）
        ])
        .setup(|app| {
            // 启动序列 1：配置加载（T2 模块；原子写/迁移桩在 config::load_from 内完成）。
            let settings = config::load_from(&config::app_config_dir(app.handle())?)?;
            config::init_global(settings)?;
            // 启动序列 1.5：instance.pid 写入（T15，D7——取启动序列最早可用落点；
            // 失败仅记录：握手属尽力语义，双实例唤起仍可经 EX-12 回调完成，仅焦点保障降级）。
            if let Err(err) =
                instance::write_pid(&config::app_config_dir(app.handle())?, std::process::id())
            {
                log::error!(
                    "instance: instance.pid 写入失败（二次启动 D7 握手将无 pid 可用）: {err}"
                );
            }
            // 启动序列 2：五项热键逐一注册 + EX-7 失败聚合通知（T4）。
            hotkey::init(app.handle())?;
            // 启动序列 2.5：T16 全链路汇聚接线——action_router handler 注入 +
            // 热键/剪贴板/引导完成/框选延续四回调接线（依赖方向 action_router→能力模块，spec 5.4）。
            // 失败阻断启动：热键回调可能于 setup 期间触发，未接线即丢事件属集成不完整。
            action_router::wire(app.handle())?;
            // 启动序列 3：剪贴板监听按持久化开关恢复（T5；失败不阻断启动，仅记录）。
            clipboard_monitor::start_if_enabled(app.handle());
            // 启动序列 4：托盘初始化（T15，spec 5.7.9——七项原生菜单 + 左键=输入翻译 FR-12.2）。
            // 失败阻断启动：托盘是 FR-1.3 常驻形态与「唯一退出路径」的承载，无托盘即无法退出。
            tray::init(app.handle())?;
            // 启动序列 5：translate 暖窗预创建并隐藏（D5/J1；常驻复用，隐藏≠销毁——T11）。
            // 失败阻断启动：无面板窗口应用不可用（spec 5.5 生命周期行）。
            windows::translate::init(app.handle())?;
            // 启动序列 6：首启引导检测（FR-11.1/11.5——onboardingCompleted 与 hasKey 均无效
            // 才显示四步向导；已配置有效配置或已告知跳过后永不自动弹出）。
            // 失败不阻断启动（应用照常常驻；用户可经设置/托盘入口补配，截图识别不受影响）。
            let startup_settings = config::read_global()?;
            if windows::onboarding::should_show_first_run(&startup_settings) {
                if let Err(err) = windows::onboarding::show_first_run(app.handle()) {
                    log::error!("onboarding: 首启引导显示失败（不阻断启动）: {err}");
                }
            }

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

    app.run(|app, event| {
        // 退出钩子（T15）：`RunEvent::Exit` 尽力清理 instance.pid（D7）。
        // 托盘「退出」路径已在 shutdown 中显式删 pid（删后再 exit(0)，此处幂等兜底——
        // 覆盖 taskkill 之外的任何正常退出/异常退出事件序列）。热键/监听清理由托盘路径执行。
        if let tauri::RunEvent::Exit = event {
            match config::app_config_dir(app).and_then(|dir| instance::remove_pid(&dir)) {
                Ok(()) => log::debug!("instance: 退出清理完成（instance.pid 已移除）"),
                Err(err) => log::warn!("instance: 退出清理 instance.pid 失败: {err}"),
            }
        }
    });
}
