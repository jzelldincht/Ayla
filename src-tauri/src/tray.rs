//! tray：托盘图标与原生菜单（T15 实现，spec 5.7.9 + design-spec 第 8 章 + FR-12）。
//!
//! 职责：
//! - **七项原生菜单**（design-spec 8 顺序与文案；原生菜单不自绘）：
//!   `当前场景 ▶（互译/定向 单选勾选）｜输入翻译｜截图识别｜截图翻译｜☑剪贴板监听｜设置｜退出`；
//!   无划词项（FR-12.1）。菜单为 config 的镜像（INV-8）：场景勾选与监听勾选实时反映。
//! - **左键单击＝输入翻译**（FR-12.2/Q8；`show_menu_on_left_click(false)` 使左键发射
//!   click 事件而不是弹菜单；右键保持原生）。
//! - **菜单镜像重建**：`settings://changed` 收到 场景/监听 相关补丁时重建菜单
//!   （`should_rebuild_menu` 纯决策；重建只 set_menu 不发射任何事件——无递归）。
//! - **「退出」= FR-1.3 唯一退出路径**：注销热键 → 停剪贴板监听 → 删 instance.pid →
//!   `app.exit(0)`（`shutdown`；pid/热键/监听清理失败仅记日志，不阻断退出）。
//! - 动作入口统一：菜单动作经 `action_router::dispatch`（输入翻译/截图识别/截图翻译/场景切换）
//!   与 `action_router::set_scene`（子菜单单选，INV-8 与 Alt+W 同语义路径）；
//!   剪贴板监听开关经 `clipboard_monitor::set_clipboard_monitor`（6.1 三方统一入口）。
//!
//! 依赖方向（spec 5.4）：tray → action_router/config/hotkey/clipboard_monitor/windows → 能力模块；
//! 禁止反向。本模块无常驻自有状态：托盘句柄与菜单均由 tauri 托管（`tray_by_id` 取回）。

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Listener, Wry};

use crate::action_router::{self, Action};
use crate::config;
use crate::error::AppError;
use crate::events::{SettingsPatch, EVENT_SETTINGS_CHANGED};

/// 托盘图标 ID（多实例防护：仅一个托盘；`tray_by_id` 取回重建菜单）。
pub const TRAY_ID: &str = "alya-tray";

/// 菜单项 ID（muda 原生菜单 id；事件回调按此分发，文案与 ID 解耦）。
pub const ID_SCENE_MUTUAL: &str = "scene-mutual";
pub const ID_SCENE_DIRECTED: &str = "scene-directed";
pub const ID_INPUT_TRANSLATE: &str = "input-translate";
pub const ID_OCR_RECOGNIZE: &str = "ocr-recognize";
pub const ID_OCR_TRANSLATE: &str = "ocr-translate";
pub const ID_CLIPBOARD_TOGGLE: &str = "clipboard-toggle";
pub const ID_SETTINGS: &str = "settings-open";
pub const ID_QUIT: &str = "quit";

// ---------------------------------------------------------------------------
// 纯逻辑：菜单镜像快照 + 重建决策（单测覆盖；INV-8 config 为事实源）
// ---------------------------------------------------------------------------

/// 菜单镜像快照（从 config 单点提取；构建菜单仅消费此结构）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuSnapshot {
    /// 活动场景（互译）。
    pub scene_mutual: bool,
    /// 活动场景（定向）。
    pub scene_directed: bool,
    /// 剪贴板监听勾选态（FR-5.1 三方同步）。
    pub clipboard_checked: bool,
}

/// config → 菜单镜像（INV-8：托盘是 config 的订阅镜像，不做独立状态）。
pub fn menu_snapshot_of(settings: &config::Settings) -> MenuSnapshot {
    MenuSnapshot {
        scene_mutual: settings.scene.active == config::SceneMode::Mutual,
        scene_directed: settings.scene.active == config::SceneMode::Directed,
        clipboard_checked: settings.clipboard_monitor,
    }
}

/// `settings://changed` 镜像重建决策：补丁涉及菜单可见状态（场景/监听）才重建。
/// 其余补丁（主题/OCR/Key/面板尺寸…）与菜单无关，跳过重建（避免无谓的菜单创建/释放）。
pub fn should_rebuild_menu(patch: &SettingsPatch) -> bool {
    patch
        .scene
        .as_ref()
        .map(|s| s.active.is_some())
        .unwrap_or(false)
        || patch.clipboard_monitor.is_some()
}

// ---------------------------------------------------------------------------
// 托盘初始化（lib.rs 启动序列第 4 步）
// ---------------------------------------------------------------------------

/// 创建托盘图标 + 七项菜单 + 事件订阅（左键/菜单） + `settings://changed` 镜像监听。
/// 失败上抛——托盘是 FR-1.3 常驻形态的承载（无托盘即无退出路径），阻断启动。
pub fn init(app: &AppHandle) -> Result<(), AppError> {
    if app.tray_by_id(TRAY_ID).is_some() {
        return Ok(()); // 幂等（异常装配路径防重复创建）
    }
    let settings = config::read_global()?;
    let menu = build_menu(app, &settings)?;

    let app_clone = app.clone();
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().cloned().ok_or_else(|| {
            AppError::Window("托盘图标缺失（default_window_icon 为空）".to_string())
        })?)
        .tooltip("Alya 翻译")
        .menu(&menu)
        // FR-12.2：左键单击＝呼出输入翻译面板（不弹菜单）；右键保持原生菜单。
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu_event(app, event.id().as_ref()))
        .on_tray_icon_event(on_tray_event)
        .build(app)
        .map_err(|e| AppError::Window(format!("托盘图标创建失败: {e}")))?;

    // INV-8 镜像订阅：config 变更广播 → 场景/监听变更则重建菜单（重建不带事件，无递归）。
    app.listen(EVENT_SETTINGS_CHANGED, move |event| {
        let patch: SettingsPatch = match serde_json::from_str(event.payload()) {
            Ok(patch) => patch,
            Err(err) => {
                log::warn!("tray: settings://changed 载荷解析失败（跳过重建）: {err}");
                return;
            }
        };
        if should_rebuild_menu(&patch) {
            if let Err(err) = rebuild_menu(&app_clone) {
                log::error!("tray: 菜单镜像重建失败: {err}");
            }
        }
    });

    log::info!("tray: 托盘已就绪（七项菜单；左键=输入翻译 FR-12.2）");
    Ok(())
}

/// 从 config 构建菜单（原生：Submenu/MenuItem/CheckMenuItem/Separator——design-spec 8 顺序）。
/// i18n-1：文案按 config general.language 解析（System 按系统 UI 语言），语言切换后菜单
/// 镜像重建自动跟随。
fn build_menu(app: &AppHandle, settings: &config::Settings) -> Result<Menu<Wry>, AppError> {
    let lang = crate::ui_text::resolve(settings);
    let snapshot = menu_snapshot_of(settings);
    let scene_menu = Submenu::with_items(
        app,
        lang.tray_scene_menu(),
        true,
        &[
            &CheckMenuItem::with_id(
                app,
                ID_SCENE_MUTUAL,
                lang.tray_scene_mutual(),
                true,
                snapshot.scene_mutual,
                None::<&str>,
            )
            .map_err(map_menu_err)?,
            &CheckMenuItem::with_id(
                app,
                ID_SCENE_DIRECTED,
                lang.tray_scene_directed(),
                true,
                snapshot.scene_directed,
                None::<&str>,
            )
            .map_err(map_menu_err)?,
        ],
    )
    .map_err(map_menu_err)?;
    let separator1 = PredefinedMenuItem::separator(app).map_err(map_menu_err)?;
    let input = MenuItem::with_id(app, ID_INPUT_TRANSLATE, lang.tray_input_translate(), true, None::<&str>)
        .map_err(map_menu_err)?;
    let ocr_recognize = MenuItem::with_id(app, ID_OCR_RECOGNIZE, lang.tray_ocr_recognize(), true, None::<&str>)
        .map_err(map_menu_err)?;
    let ocr_translate = MenuItem::with_id(app, ID_OCR_TRANSLATE, lang.tray_ocr_translate(), true, None::<&str>)
        .map_err(map_menu_err)?;
    let separator2 = PredefinedMenuItem::separator(app).map_err(map_menu_err)?;
    let clipboard = CheckMenuItem::with_id(
        app,
        ID_CLIPBOARD_TOGGLE,
        lang.tray_clipboard(),
        true,
        snapshot.clipboard_checked,
        None::<&str>,
    )
    .map_err(map_menu_err)?;
    let separator3 = PredefinedMenuItem::separator(app).map_err(map_menu_err)?;
    let settings_item =
        MenuItem::with_id(app, ID_SETTINGS, lang.tray_settings(), true, None::<&str>).map_err(map_menu_err)?;
    let quit = MenuItem::with_id(app, ID_QUIT, lang.tray_quit(), true, None::<&str>).map_err(map_menu_err)?;

    Menu::with_items(
        app,
        &[
            &scene_menu,
            &separator1,
            &input,
            &ocr_recognize,
            &ocr_translate,
            &separator2,
            &clipboard,
            &separator3,
            &settings_item,
            &quit,
        ],
    )
    .map_err(map_menu_err)
}

fn map_menu_err(err: impl std::fmt::Display) -> AppError {
    AppError::Window(format!("托盘菜单构建失败: {err}"))
}

/// 镜像重建（INV-8）：从 config 重建菜单整体替换（muda 菜单可整体替换；旧菜单随 drop 释放）。
/// **不发射任何事件**——本函数只做菜单设置，杜绝「重建→广播→重建」递归。
pub fn rebuild_menu(app: &AppHandle) -> Result<(), AppError> {
    let settings = config::read_global()?;
    let menu = build_menu(app, &settings)?;
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| AppError::Window("托盘未初始化（tray_by_id 未命中）".to_string()))?;
    tray.set_menu(Some(menu))
        .map_err(|e| AppError::Window(format!("托盘菜单替换失败: {e}")))?;
    log::debug!("tray: 菜单已按 config 镜像重建");
    Ok(())
}

// ---------------------------------------------------------------------------
// 事件分发（菜单 id → 动作；右键菜单原生弹出，无需处理）
// ---------------------------------------------------------------------------

fn on_menu_event(app: &AppHandle, id: &str) {
    match id {
        ID_SCENE_MUTUAL => set_scene_via_menu(app, config::SceneMode::Mutual),
        ID_SCENE_DIRECTED => set_scene_via_menu(app, config::SceneMode::Directed),
        ID_INPUT_TRANSLATE => action_router::dispatch(app, Action::InputTranslate),
        ID_OCR_RECOGNIZE => action_router::dispatch(app, Action::OcrRecognize),
        ID_OCR_TRANSLATE => action_router::dispatch(app, Action::OcrTranslate),
        ID_CLIPBOARD_TOGGLE => toggle_clipboard_via_menu(app),
        ID_SETTINGS => {
            if let Err(err) = crate::windows::settings::open_settings(app.clone()) {
                log::error!("tray: 打开设置失败: {err}");
            }
        }
        ID_QUIT => shutdown(app),
        other => log::warn!("tray: 未知菜单项 id={other}"),
    }
}

/// 子菜单单选 → 场景切换（与 Alt+W/面板分段同一 config 写入与广播路径，INV-8）。
fn set_scene_via_menu(app: &AppHandle, scene: config::SceneMode) {
    if let Err(err) = action_router::set_scene(app, scene) {
        log::error!("tray: 场景切换失败 {scene:?}: {err}");
    }
}

/// 剪贴板监听勾选 → 6.1 三方统一入口（写 config + 广播；失败仅日志——菜单镜像保持旧态，
/// 与设置页失败行内 error 的呈现层差异由「托盘点击无行内反馈」决定，状态以 config 为准）。
fn toggle_clipboard_via_menu(app: &AppHandle) {
    let current = config::read_global()
        .map(|guard| guard.clipboard_monitor)
        .unwrap_or(false);
    let result = crate::clipboard_monitor::set_clipboard_monitor(app.clone(), !current);
    if let Err(err) = result {
        log::error!("tray: 剪贴板监听切换失败: {err}");
    }
}

/// 左键单击（FR-12.2/Q8）：`MouseButtonState::Up` 才动作（Down 先行；防重复/防与拖拽混淆）。
fn on_tray_event(tray: &TrayIcon, event: TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        let app = tray.app_handle();
        log::debug!("tray: 左键单击 → 输入翻译（FR-12.2/Q8）");
        action_router::dispatch(app, Action::InputTranslate);
    }
}

// ---------------------------------------------------------------------------
// 「退出」路径（FR-1.3：唯一彻底退出入口；面板关闭只是隐藏）
// ---------------------------------------------------------------------------

/// 退出清理序（spec 5.7.9）：注销热键 → 停监听 → 删 pid → `app.exit(0)`。
/// 各步失败仅记日志不阻断（进程即将退出；pid 清理另有 RunEvent::Exit 兜底）。
pub fn shutdown(app: &AppHandle) {
    log::info!("tray: 「退出」→ 注销热键 → 停监听 → 删 pid → exit(0)");
    crate::hotkey::shutdown(app);
    if let Err(err) = crate::clipboard_monitor::global().disable() {
        log::warn!("tray: 剪贴板监听停止失败（进程即将退出）: {err}");
    }
    match config::app_config_dir(app).and_then(|dir| crate::instance::remove_pid(&dir)) {
        Ok(()) => log::debug!("tray: instance.pid 已清理"),
        Err(err) => log::warn!("tray: instance.pid 清理失败（RunEvent::Exit 兜底）: {err}"),
    }
    app.exit(0);
}

// ---------------------------------------------------------------------------
// 单元测试：菜单镜像快照 + 重建决策（纯逻辑，不触 win32/tauri）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ScenePatch;

    #[test]
    fn menu_snapshot_mirrors_config_exactly() {
        // 出厂（互译 + 监听关）
        let settings = config::Settings::default();
        let snapshot = menu_snapshot_of(&settings);
        assert!(snapshot.scene_mutual);
        assert!(!snapshot.scene_directed);
        assert!(!snapshot.clipboard_checked);

        // 定向 + 监听开
        let directed = config::Settings {
            clipboard_monitor: true,
            scene: config::SceneSettings {
                active: config::SceneMode::Directed,
                ..config::SceneSettings::default()
            },
            ..config::Settings::default()
        };
        let snapshot = menu_snapshot_of(&directed);
        assert!(!snapshot.scene_mutual, "单选语义：互译不勾");
        assert!(snapshot.scene_directed);
        assert!(snapshot.clipboard_checked);

        // 快照必须严格二选一（场景唯一性 FR-2.3）
        assert_ne!(
            snapshot.scene_mutual, snapshot.scene_directed,
            "互译/定向恰一为真（全局唯一活动场景）"
        );
    }

    #[test]
    fn rebuild_decision_only_for_scene_and_clipboard_patches() {
        // 场景补丁 → 重建
        assert!(should_rebuild_menu(&SettingsPatch {
            scene: Some(ScenePatch {
                active: Some(config::SceneMode::Directed),
            }),
            ..Default::default()
        }));
        // 监听补丁 → 重建
        assert!(should_rebuild_menu(&SettingsPatch {
            clipboard_monitor: Some(true),
            ..Default::default()
        }));
        // 无关补丁（主题/OCR/面板/翻译/Key）→ 不重建
        assert!(!should_rebuild_menu(&SettingsPatch::default()));
        assert!(!should_rebuild_menu(&SettingsPatch {
            scene: Some(ScenePatch { active: None }),
            ..Default::default()
        }));
        assert!(!should_rebuild_menu(&crate::events::SettingsPatch {
            ocr: Some(crate::events::OcrPatch {
                language: Some("ja".to_string()),
            }),
            ..Default::default()
        }));
        assert!(!should_rebuild_menu(&crate::events::SettingsPatch {
            general: Some(crate::events::GeneralPatch {
                autostart: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        }));
    }

    #[test]
    fn menu_ids_are_stable_and_distinct() {
        let ids = [
            ID_SCENE_MUTUAL,
            ID_SCENE_DIRECTED,
            ID_INPUT_TRANSLATE,
            ID_OCR_RECOGNIZE,
            ID_OCR_TRANSLATE,
            ID_CLIPBOARD_TOGGLE,
            ID_SETTINGS,
            ID_QUIT,
        ];
        let mut sorted: Vec<&str> = ids.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            ids.len(),
            "菜单 id 不得重复（muda 事件分发唯一性）"
        );
        assert_eq!(TRAY_ID, "alya-tray");
    }
}
