//! onboarding 窗（label=onboarding）：固定尺寸向导 560×430、有边框、居中（T14 实现；
//! spec 5.5/FR-11；design-spec 6）。
//!
//! 模式（`events::OnboardingMode`，窗口 URL `?mode=guide` 与 `onboarding://begin` 载荷同源）：
//! - `First`（首启四步向导，FR-11.1）：启动检测「无任何已存引导且未配置 Key」→ 创建并显示；
//! - `Guide`（FR-11.4 补配引导）：`open_config_guide` 命令复用同一窗体直达隐私要点 + Key 步骤，
//!   完成后经 `GuideFinishedHandler` 回调通知补配结果（T16 接线：挂起翻译自动继续/丢弃）。
//!
//! 生命周期（spec 5.5）：完成/跳过后隐藏保留（补配复用）；已配置有效 Key 永不自动弹出（FR-11.5）；
//! 向导不支持 Esc（前端忽略）且原生关闭按钮＝隐藏（未完成时下次启动重新弹出，不阻断常驻）。
//!
//! `finish_onboarding`：写 `meta.onboardingCompleted=true`（FR-11.3/FR-11.5）→ 落盘 →
//! `settings://changed` 广播（仅 meta 补丁）→ 隐藏保留 → guide 模式触发回调（参数 hasKey）。
//!
//! 锁序：仅 config 全局访问器（先 config 后 sessions）；本模块自有锁零把
//! （`MODE` 为原子量，无互斥需求）。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

use tauri::{
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use crate::config;
use crate::error::AppError;
use crate::events::{
    MetaPatch, OnboardingBeginPayload, OnboardingMode, SettingsPatch, EVENT_ONBOARDING_BEGIN,
    EVENT_SETTINGS_CHANGED,
};

/// 引导窗 label（spec 5.5 窗口清单；translate.rs 失焦隐藏/判定亦引用）。
pub const WINDOW_ONBOARDING_LABEL: &str = "onboarding";

/// 固定尺寸 560×480 逻辑像素、有边框、居中（design-spec 6 窗体规格；真机修复轮 13d
/// 自 430 上调——AI 服务步扩为接口地址/模型名/API Key 三字段）。
pub const ONBOARDING_SIZE_LOGICAL: (f64, f64) = (560.0, 480.0);

// ---------------------------------------------------------------------------
// 首启检测（FR-11.1/11.5 纯判定）
// ---------------------------------------------------------------------------

/// 首启引导是否应显示：`onboardingCompleted==false` **且** `hasKey==false`。
/// 任一成立即已生效/已告知 → 不再自动弹出（FR-11.5：已存在有效配置永不自动弹出；
/// 已告知并跳过（FR-11.3）同样不再弹出）。
pub fn should_show_first_run(settings: &config::Settings) -> bool {
    !settings.meta.onboarding_completed && !config::has_key(settings)
}

// ---------------------------------------------------------------------------
// 补配完成回调（FR-11.4；T16 由 action_router 接线）
// ---------------------------------------------------------------------------

/// 引导 guide 模式完成回调接口：`has_key` 区分「补配成功（继续挂起翻译）」与
/// 「跳过/未补配（丢弃挂起翻译，FR-11.3 路径）」——T16 据此恢复 FR-11.4 挂起队列。
pub trait GuideFinishedHandler: Send + Sync + 'static {
    fn guide_finished(&self, has_key: bool);
}

static GUIDE_HANDLER: OnceLock<Box<dyn GuideFinishedHandler>> = OnceLock::new();

/// 注入回调（仅一次；重复注入拒绝，防多源接线——hotkey::set_callback 同先例）。
pub fn set_guide_finished_handler(handler: Box<dyn GuideFinishedHandler>) -> Result<(), AppError> {
    GUIDE_HANDLER
        .set(handler)
        .map_err(|_| AppError::Config("onboarding: guide 完成回调重复注入".to_string()))
}

fn notify_guide_finished(has_key: bool) {
    match GUIDE_HANDLER.get() {
        Some(handler) => handler.guide_finished(has_key),
        None => log::debug!("onboarding: guide 完成，回调未接线（T16 action_router 接入）"),
    }
}

// ---------------------------------------------------------------------------
// 窗口模式态（原子；URL 查询参数不可变，模式切换＝销毁重建）
// ---------------------------------------------------------------------------

const MODE_FIRST: u8 = 0;
const MODE_GUIDE: u8 = 1;

static CURRENT_MODE: AtomicU8 = AtomicU8::new(MODE_FIRST);

fn mode_of() -> OnboardingMode {
    if CURRENT_MODE.load(Ordering::SeqCst) == MODE_GUIDE {
        OnboardingMode::Guide
    } else {
        OnboardingMode::First
    }
}

fn store_mode(mode: OnboardingMode) {
    CURRENT_MODE.store(
        if mode == OnboardingMode::Guide {
            MODE_GUIDE
        } else {
            MODE_FIRST
        },
        Ordering::SeqCst,
    );
}

fn query_of(mode: OnboardingMode) -> &'static str {
    match mode {
        OnboardingMode::Guide => "?mode=guide",
        OnboardingMode::First => "",
    }
}

// ---------------------------------------------------------------------------
// 显示入口
// ---------------------------------------------------------------------------

/// 首启显示（lib.rs 启动序列调用；调用方先 `should_show_first_run` 判定）。
pub fn show_first_run(app: &AppHandle) -> Result<(), AppError> {
    let window = ensure_window(app, OnboardingMode::First)?;
    show_and_begin(app, window, OnboardingMode::First)
}

/// `open_config_guide` 命令（T14 新增；FR-11.4 由 T16 action_router 调用）：
/// guide 模式复用同一窗体直达隐私要点 + Key 步骤。
#[tauri::command]
pub fn open_config_guide(app: AppHandle) -> Result<(), AppError> {
    let window = ensure_window(&app, OnboardingMode::Guide)?;
    show_and_begin(&app, window, OnboardingMode::Guide)
}

fn show_and_begin(
    app: &AppHandle,
    window: WebviewWindow,
    mode: OnboardingMode,
) -> Result<(), AppError> {
    window
        .show()
        .map_err(|e| AppError::Window(format!("onboarding 窗显示失败: {e}")))?;
    log::info!("onboarding: 引导窗已显示（mode={mode:?}，FR-11.4 挂起判定依赖可见性）");
    if let Err(err) = window.set_focus() {
        log::warn!("onboarding: 引导窗焦点失败: {err}");
    }
    // 每次显示复位页面（复用窗残留状态规避）：onboarding://begin {mode}
    let payload = OnboardingBeginPayload { mode };
    if let Err(err) = app.emit(EVENT_ONBOARDING_BEGIN, payload) {
        log::error!("onboarding: onboarding://begin 广播失败: {err}");
    }
    Ok(())
}

/// 按需创建（模式不符则销毁重建——URL 查询参数不可变）或返回既有单例。
pub fn ensure_window(app: &AppHandle, mode: OnboardingMode) -> Result<WebviewWindow, AppError> {
    if let Some(window) = app.get_webview_window(WINDOW_ONBOARDING_LABEL) {
        if mode_of() == mode {
            return Ok(window);
        }
        // 模式变化：销毁重建（销毁失败不阻断——build 会覆盖标签；旧窗仅记日志）。
        if let Err(err) = window.destroy() {
            log::warn!("onboarding: 旧引导窗销毁失败（重建覆盖）: {err}");
        }
    }
    store_mode(mode);
    let app_clone = app.clone();
    WebviewWindowBuilder::new(
        app,
        WINDOW_ONBOARDING_LABEL,
        WebviewUrl::App(PathBuf::from(format!(
            "src/onboarding/index.html{}",
            query_of(mode)
        ))),
    )
    .title(crate::ui_text::resolve_global().onboarding_window_title())
    .inner_size(ONBOARDING_SIZE_LOGICAL.0, ONBOARDING_SIZE_LOGICAL.1)
    .min_inner_size(ONBOARDING_SIZE_LOGICAL.0, ONBOARDING_SIZE_LOGICAL.1)
    .resizable(false)
    .visible(false)
    .build()
    .map_err(|e| AppError::Window(format!("onboarding 窗创建失败: {e}")))
    .inspect(|window| {
        // 居中失败不阻断（仅位置差异）。固定尺寸向导：不支持 Esc（前端忽略）；
        // 原生关闭按钮＝隐藏（完成/跳过由命令写 meta 后隐藏；未完成时下次启动重新弹出）。
        let _ = window.center();
        // 真机修复轮 14b：原生标题栏随主题（深/浅）。
        crate::ui_text::apply_titlebar_theme(
            window,
            config::read_global().map(|s| s.general.theme).unwrap_or(config::ThemeMode::System),
        );
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Some(single) = app_clone.get_webview_window(WINDOW_ONBOARDING_LABEL) {
                    let _ = single.hide();
                }
            }
        });
    })
}

// ---------------------------------------------------------------------------
// finish_onboarding（完成/跳过；FR-11.1(d)/FR-11.3/FR-11.5）
// ---------------------------------------------------------------------------

/// `finish_onboarding` 命令（T14 前端调用）：写 `meta.onboardingCompleted=true` → 落盘 →
/// 广播（仅 meta 补丁，INV-8）→ 隐藏保留（spec 5.5 完成/跳过后隐藏保留）→
/// guide 模式触发补配回调（FR-11.4，含 hasKey 判定；T16 接线）。
#[tauri::command]
pub fn finish_onboarding(app: AppHandle) -> Result<(), AppError> {
    let dir = config::app_config_dir(&app)?;
    {
        let mut guard = config::write_global()?;
        guard.meta.onboarding_completed = true;
    }
    {
        let settings = config::read_global()?;
        config::save_to(&dir, &settings)?;
    }

    let patch = SettingsPatch {
        meta: Some(MetaPatch {
            onboarding_completed: Some(true),
        }),
        ..Default::default()
    };
    if let Err(err) = app.emit(EVENT_SETTINGS_CHANGED, patch) {
        log::error!("onboarding: settings://changed 广播失败: {err}");
    }
    if let Some(window) = app.get_webview_window(WINDOW_ONBOARDING_LABEL) {
        let _ = window.hide();
    }
    if mode_of() == OnboardingMode::Guide {
        let has_key = config::read_global()
            .map(|s| config::has_key(&s))
            .unwrap_or(false);
        notify_guide_finished(has_key);
    }
    log::info!("onboarding: 完成/跳过已落盘（onboardingCompleted=true）");
    Ok(())
}

// ---------------------------------------------------------------------------
// 单元测试：首启判定矩阵（FR-11.5）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_show_first_run_matrix_fr115() {
        // 出厂新配置（未完成 + 无 Key）→ 显示
        assert!(should_show_first_run(&config::Settings::default()));

        // 已完成引导（即使 Key 跳过，FR-11.3）→ 不再显示
        let mut completed = config::Settings::default();
        completed.meta.onboarding_completed = true;
        assert!(!should_show_first_run(&completed));

        // 已配置有效 Key（meta 未写，旧版配置等）→ 永不自动弹出
        let mut with_key = config::Settings::default();
        let cipher = crate::config::crypto::protect(b"sk-fake-test-key-002")
            .unwrap_or_else(|err| panic!("DPAPI 保护失败: {err}"));
        with_key.ai.api_key_cipher = Some(cipher);
        assert!(!should_show_first_run(&with_key));

        // 恒真组合防御
        assert!(should_show_first_run(&config::Settings::default()));
    }

    #[test]
    fn mode_roundtrip_and_query() {
        // 模式原子态与 URL 查询参数一致（guide 带 ?mode=guide，first 无参）
        store_mode(OnboardingMode::Guide);
        assert_eq!(mode_of(), OnboardingMode::Guide);
        assert_eq!(query_of(OnboardingMode::Guide), "?mode=guide");
        store_mode(OnboardingMode::First);
        assert_eq!(mode_of(), OnboardingMode::First);
        assert_eq!(query_of(OnboardingMode::First), "");
    }
}
