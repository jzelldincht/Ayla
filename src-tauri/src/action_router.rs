//! action_router：五动作统一入口（T15 占位实现；T16 汇聚前置检查链与全链路）。
//!
//! spec 5.4：热键/托盘/单实例回调 → `action_router` → 各能力模块（禁止反向、禁止绕过本模块
//! 直接触发翻译——保证 EX-13 顶替只有一条路径）。本任务（T15）交付：
//! - `Action` 五动作枚举（与 T4 `HotkeyAction` 同名语义，作为跨入口的规范动作类型）；
//! - `dispatch` 唯一入口：托盘/单实例回调自此进入；已注入 `ActionHandler`（T16 实现）则转发，
//!   未注入走 **minimal 直通**（本节注释明示该直通不承载 T16 的前置检查链/挂起队列/压制语义）；
//! - `set_action_handler` 注入接口（同 hotkey::set_callback / clipboard::set_translate_trigger
//!   先例：OnceLock 一次性注入，重复注入拒绝）。
//!
//! T15 minimal 直通的行为口径（与服务模块的真实能力一致，但**不含** FR-11.4 引导挂起、
//! 引导/框选期压制——这些是 T16 的交付）：
//! - `InputTranslate` → `windows::translate::show_panel{mode: input}`（Q8/FR-12.2 面板入口）；
//! - `SelectionTranslate` → `selection::run_selection`（EX-9/10 通知在模块内完成）＋
//!   `show_panel{mode: selection, text}`；
//! - `OcrRecognize` / `OcrTranslate` → `windows::mask::start_capture_session`（截图翻译的
//!   「识别后自动送翻」分路属 T16，本任务先保框选入口一致，见 implementation-report T15 节）；
//! - `ToggleScene` → 翻转 config.scene.active（原子落盘）＋ `settings://changed` 广播（INV-8）。
//!
//! 依赖方向：action_router → selection/clipboard/translate/windows → config/events（spec 5.4）；
//! 锁序遵守「先 config 后 sessions」：本模块仅持 config 锁，无嵌套。

use tauri::AppHandle;
use tauri::Emitter;

use crate::config;
use crate::error::AppError;
use crate::events::{
    PanelMode, PanelShowPayload, ScenePatch, SettingsPatch, EVENT_SETTINGS_CHANGED,
};

/// 五动作（与 spec 5.4 / T4 `HotkeyAction` 一一对应；名称即入口语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    SelectionTranslate,
    InputTranslate,
    OcrRecognize,
    OcrTranslate,
    ToggleScene,
}

/// 动作分发回调注入接口：T16 用完整 action_router 实现（前置检查链：onboarding 挂起队列容量 1、
/// hasKey==false 弹配置引导、框选/引导期压制、EX-13 单路径）并 `set_action_handler` 注入；
/// 未注入时经 minimal 直通（见模块头——显式行为，非静默失败）。
pub trait ActionHandler: Send + Sync + 'static {
    fn dispatch(&self, app: &AppHandle, action: Action);
}

static HANDLER: std::sync::OnceLock<Box<dyn ActionHandler>> = std::sync::OnceLock::new();

/// 注入完整 handler（仅一次；重复注入拒绝，防多源接线）。
pub fn set_action_handler(handler: Box<dyn ActionHandler>) -> Result<(), AppError> {
    HANDLER
        .set(handler)
        .map_err(|_| AppError::Config("action_router: handler 重复注入".to_string()))
}

/// 五动作唯一入口（托盘菜单/左键、single-instance 回调、T16 热键/剪贴板回调均经此）。
pub fn dispatch(app: &AppHandle, action: Action) {
    log::debug!("action_router: {action:?}");
    match HANDLER.get() {
        Some(handler) => handler.dispatch(app, action),
        None => minimal_dispatch(app, action),
    }
}

// ---------------------------------------------------------------------------
// T15 minimal 直通（T16 以注入 handler 取代——接口已就位，语义升级不散落调用点）
// ---------------------------------------------------------------------------

fn minimal_dispatch(app: &AppHandle, action: Action) {
    match action {
        Action::InputTranslate => {
            fail_log("输入翻译", show_panel(app, PanelMode::Input, None, None));
        }
        Action::SelectionTranslate => {
            // 取词失败已由 selection 模块按 EX-9/EX-10 发送系统通知（不弹面板）。
            match crate::selection::run_selection(app) {
                Ok(text) => fail_log(
                    "划词翻译",
                    show_panel(app, PanelMode::Selection, Some(text), None),
                ),
                Err(err) => log::debug!(
                    "action_router: 划词翻译取消（{}，通知已由 selection 模块发出）",
                    err.ex_code()
                ),
            }
        }
        Action::OcrRecognize | Action::OcrTranslate => {
            // 截图识别/截图翻译共用框选入口；「识别后自动送翻译」分路属 T16（模块头说明）。
            if let Err(err) = crate::windows::mask::start_capture_session(app) {
                log::error!("action_router: 截图框选触发失败: {err}");
            }
        }
        Action::ToggleScene => {
            let next = match config::read_global().map(|guard| guard.scene.active) {
                Ok(config::SceneMode::Mutual) => config::SceneMode::Directed,
                Ok(config::SceneMode::Directed) => config::SceneMode::Mutual,
                Err(err) => {
                    log::error!("action_router: 场景翻转读取配置失败: {err}");
                    return;
                }
            };
            fail_log("场景切换", set_scene(app, next));
        }
    }
}

fn fail_log(action_name: &str, result: Result<(), AppError>) {
    if let Err(err) = result {
        log::error!("action_router: {action_name} 执行失败: {err}");
    }
}

fn show_panel(
    app: &AppHandle,
    mode: PanelMode,
    text: Option<String>,
    session_id: Option<String>,
) -> Result<(), AppError> {
    crate::windows::translate::show_panel(
        app,
        PanelShowPayload {
            mode,
            text,
            session_id,
        },
    )
}

/// 场景变更（托盘子菜单切换 / ToggleScene 共用）：写 config → 原子落盘 → `settings://changed`
/// 广播（INV-8 三方镜像：面板分段控件/托盘勾选/设置页；托盘经监听重建菜单——无递归，重建不带事件）。
pub fn set_scene(app: &AppHandle, scene: config::SceneMode) -> Result<(), AppError> {
    let dir = config::app_config_dir(app)?;
    {
        let mut guard = config::write_global()?;
        guard.scene.active = scene;
    }
    let settings = config::read_global()?;
    config::save_to(&dir, &settings)?;
    let patch = SettingsPatch {
        scene: Some(ScenePatch {
            active: Some(scene),
        }),
        ..Default::default()
    };
    app.emit(EVENT_SETTINGS_CHANGED, patch)
        .map_err(|err| AppError::Config(format!("settings://changed 广播失败: {err}")))?;
    log::info!("action_router: 活动场景已切换为 {scene:?}");
    Ok(())
}
