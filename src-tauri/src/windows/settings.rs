//! settings 窗（label=settings）：有边框、可调尺寸、单例、关闭=隐藏（T13 实现；spec 5.5/6.1/6.3）。
//!
//! 职责：
//! - **窗壳**：首次 `open_settings` 按需创建（有边框装饰、可调、默认 760×540 逻辑、最小 680×480），
//!   常驻隐藏复用（单例）；`CloseRequested` → `prevent_close` + `hide`（关闭=隐藏，FR-1.3 口径）。
//! - **`open_settings`**（6.1）：单例前置——已存在则 show+focus，否则创建后 show+focus（EX-2 入口）。
//! - **`get_settings`**（6.1）：返回 `SettingsView`（INV-1：`ai` 仅 {baseUrl, model, hasKey}）。
//! - **`set_settings`**（6.1）：通用补丁即时保存生效（FR-13）——应用补丁→原子落盘→
//!   `settings://changed` 广播（**剥离 `ai.api_key`**，INV-1）；
//!   `ai.api_key` 空值＝清除密文（允许未配置，design-spec 7.3）；
//!   **热键与剪贴板监听不走本通道**（分别归 `rebind_hotkey` / `set_clipboard_monitor`，6.1）——
//!   收到即拒（防绕过专用命令导致状态机/注册表不同步）。
//!
//! 偏离 tasks.md 的说明（记录于 implementation-report T13 节；T15 已收口第一项）：
//! - 开机自启（FR-1.5）：T13 仅落 config 值；**T15 已在 set_settings 接线 HKCU Run 写入/移除**
//!   （autostart.rs：注册表先行、失败即中止、config 仅随成功更新——见本文件 set_settings 注释）。
//! - 自动保存反馈（「已保存」1.5s 淡出）为前端 UI 语义，本窗壳不感知。
//!
//! 锁序：先 config 后 sessions——本模块仅经 config 全局访问器；无自有锁。

use std::path::PathBuf;

use tauri::{
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use crate::config;
use crate::error::AppError;
use crate::events::{AiPatch, SettingsPatch, SettingsView, EVENT_SETTINGS_CHANGED};

/// 设置窗 label（spec 5.5 窗口清单）。
pub const WINDOW_SETTINGS_LABEL: &str = "settings";

/// 默认 760×540 逻辑像素（design-spec 7.1）、最小 680×480、可调。
pub const SETTINGS_DEFAULT_SIZE_LOGICAL: (f64, f64) = (760.0, 540.0);
pub const SETTINGS_MIN_SIZE_LOGICAL: (f64, f64) = (680.0, 480.0);

// ---------------------------------------------------------------------------
// 纯函数：补丁应用与广播剥离（命令内调用；单测覆盖）
// ---------------------------------------------------------------------------

/// 把 `set_settings` 补丁合并到配置（**热键/剪贴板监听补丁拒绝**——6.1 声明两者走专用通道）。
/// `apiKey` 语义（design-spec 7.3）：空白＝未配置（清密文）；非空＝立即 DPAPI 加密（INV-1）。
pub fn apply_settings_patch(
    settings: &mut config::Settings,
    patch: &SettingsPatch,
) -> Result<(), AppError> {
    if patch.hotkeys.is_some() {
        return Err(AppError::Config(
            "热键变更不走 set_settings：请用 rebind_hotkey 专用命令（spec 6.1）".to_string(),
        ));
    }
    if patch.clipboard_monitor.is_some() {
        return Err(AppError::Config(
            "剪贴板监听开关不走 set_settings：请用 set_clipboard_monitor 三方统一入口（spec 6.1）"
                .to_string(),
        ));
    }

    if let Some(ai) = &patch.ai {
        if let Some(base_url) = &ai.base_url {
            settings.ai.base_url = base_url.trim().to_string();
        }
        if let Some(model) = &ai.model {
            settings.ai.model = model.trim().to_string();
        }
        if let Some(api_key) = &ai.api_key {
            // 空值＝未配置（密文清除）；非空经 DPAPI 加密，明文仅存于入参内存（INV-1）。
            let key = api_key.trim();
            if key.is_empty() {
                settings.ai.api_key_cipher = None;
            } else {
                settings.ai.api_key_cipher =
                    Some(config::crypto::protect(key.as_bytes()).map_err(|err| {
                        AppError::Config(format!(
                            "API Key 加密失败（DPAPI {} 类别，无任何值）",
                            err.class()
                        ))
                    })?);
            }
        }
    }
    if let Some(translation) = &patch.translation {
        if let Some(lang) = &translation.default_target_lang {
            settings.translation.default_target_lang = lang.clone();
        }
        if let Some(presentation) = translation.presentation {
            settings.translation.presentation = presentation;
        }
        if let Some(chars) = translation.long_text_threshold_chars {
            settings.translation.long_text_threshold_chars = chars;
        }
        if let Some(auto_translate) = &translation.auto_translate {
            if let Some(enabled) = auto_translate.enabled {
                settings.translation.auto_translate.enabled = enabled;
            }
            if let Some(delay_ms) = auto_translate.delay_ms {
                // 越界值按出厂默认处理（防误输入过短造成请求风暴/过长像失效）。
                settings.translation.auto_translate.delay_ms =
                    if (300..=5000).contains(&delay_ms) { delay_ms } else { 700 };
            }
        }
    }
    if let Some(panel) = &patch.panel {
        if let Some(hide) = panel.hide_on_blur {
            settings.panel.hide_on_blur = hide;
        }
        if let Some(width) = panel.width_logical {
            settings.panel.width_logical = width;
        }
        if let Some(height) = panel.height_logical {
            settings.panel.height_logical = height;
        }
        // 原文输入框高度记忆（真机修复轮 13i）：合法范围 56~5000 逻辑像素，
        // 越界值忽略（保留现值）——防误写入异常值把编辑器压没或撑爆。
        if let Some(editor_height) = panel.editor_height {
            if (56..=5000).contains(&editor_height) {
                settings.panel.editor_height_logical = Some(editor_height);
            }
        }
    }
    if let Some(scene) = &patch.scene {
        if let Some(active) = scene.active {
            settings.scene.active = active;
        }
    }
    if let Some(ocr) = &patch.ocr {
        if let Some(language) = &ocr.language {
            settings.ocr.language = language.clone();
        }
    }
    if let Some(general) = &patch.general {
        if let Some(autostart) = general.autostart {
            settings.general.autostart = autostart;
        }
        if let Some(theme) = general.theme {
            settings.general.theme = theme;
        }
        // 界面语言（i18n-1）。
        if let Some(language) = general.language {
            settings.general.language = language;
        }
    }
    if let Some(meta) = &patch.meta {
        if let Some(completed) = meta.onboarding_completed {
            settings.meta.onboarding_completed = completed;
        }
    }
    Ok(())
}

/// 构造 `settings://changed` 广播补丁：**剥离 `ai.api_key`**（INV-1：不得发往任何 WebView；
/// 密文永不广播）；`ai` 子补丁剥离后仅剩空字段则整体省略；热键/剪贴板监听本通道恒拒绝故不携带。
pub fn broadcast_patch(patch: &SettingsPatch) -> SettingsPatch {
    let ai = patch
        .ai
        .as_ref()
        .map(|ai| AiPatch {
            base_url: ai.base_url.clone(),
            model: ai.model.clone(),
            api_key: None,
        })
        .filter(|ai| ai.base_url.is_some() || ai.model.is_some());
    SettingsPatch {
        ai,
        translation: patch.translation.clone(),
        panel: patch.panel.clone(),
        clipboard_monitor: None,
        scene: patch.scene.clone(),
        hotkeys: None,
        ocr: patch.ocr.clone(),
        general: patch.general.clone(),
        meta: patch.meta.clone(),
    }
}

// ---------------------------------------------------------------------------
// 6.1 命令：get_settings / set_settings / open_settings
// ---------------------------------------------------------------------------

/// `get_settings`（6.1；INV-1：API Key 仅返回 `hasKey`）。
#[tauri::command]
pub fn get_settings() -> Result<SettingsView, AppError> {
    let settings = config::read_global()?;
    Ok(SettingsView::from_settings(&settings))
}

/// `set_settings`（6.1）：通用设置即时保存生效（FR-13）——
/// 应用补丁 → 原子落盘 → `settings://changed` 广播（剥离 apiKey，INV-1）。
/// 广播失败仅记录（三处 UI 镜像滞后至下次 get_settings，INV-8 收敛口径，先例见 T5）。
/// 开机自启（FR-1.5）例外接线：**先**执行 HKCU Run 真实写入/移除（`autostart::apply`），
/// 失败即中止（config 值不动、命令返回错误由设置页行内呈现）——「启用才写、关闭即移除」
/// 语义由 autostart.rs 保证（含已禁用态的幂等判定）。
#[tauri::command]
pub fn set_settings(app: AppHandle, patch: SettingsPatch) -> Result<(), AppError> {
    if let Some(target_on) = patch.general.as_ref().and_then(|general| general.autostart) {
        crate::autostart::apply(&app, target_on)?;
    }
    let dir = config::app_config_dir(&app)?;
    {
        let mut guard = config::write_global()?;
        apply_settings_patch(&mut guard, &patch)?;
    }
    let settings = config::read_global()?;
    config::save_to(&dir, &settings)?;

    // 真机修复轮 14b：主题变更 → 原生标题栏（设置窗/引导窗）即时跟随。
    if patch.general.as_ref().and_then(|g| g.theme).is_some() {
        let theme = config::read_global().map(|s| s.general.theme).unwrap_or(config::ThemeMode::System);
        for label in [WINDOW_SETTINGS_LABEL, crate::windows::onboarding::WINDOW_ONBOARDING_LABEL] {
            if let Some(w) = app.get_webview_window(label) {
                crate::ui_text::apply_titlebar_theme(&w, theme);
            }
        }
    }

    let broadcast = broadcast_patch(&patch);
    if let Err(err) = app.emit(EVENT_SETTINGS_CHANGED, broadcast) {
        log::error!("settings: settings://changed 广播失败: {err}");
    }
    Ok(())
}

/// `open_settings`（6.1；EX-2 入口）：单例前置——已有窗则 show+focus，否则创建后显示。
#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<(), AppError> {
    let window = ensure_window(&app)?;
    show_and_focus(&app, &window)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 窗壳：按需创建 / 隐藏复用 / 关闭=隐藏
// ---------------------------------------------------------------------------

/// 按需创建（首次打开）或返回既有单例（隐藏复用）。
pub fn ensure_window(app: &AppHandle) -> Result<WebviewWindow, AppError> {
    if let Some(window) = app.get_webview_window(WINDOW_SETTINGS_LABEL) {
        return Ok(window);
    }
    let app_clone = app.clone();
    // 尺寸上限＝主屏工作区（真机修复轮 13c：贴边/手动拉伸不得超过屏幕高度——
    // 此前无可调上限，用户拖拽/贴靠后窗口底边越出屏幕）。宽高不减边距：Win 贴边
    // 恰为工作区整幅，需允许贴满。取不到显示器时不设上限（保守不阻断建窗）。
    let mut builder = WebviewWindowBuilder::new(
        app,
        WINDOW_SETTINGS_LABEL,
        WebviewUrl::App(PathBuf::from("src/settings/index.html")),
    )
    .title(crate::ui_text::resolve_global().settings_window_title())
    .inner_size(
        SETTINGS_DEFAULT_SIZE_LOGICAL.0,
        SETTINGS_DEFAULT_SIZE_LOGICAL.1,
    )
    .min_inner_size(SETTINGS_MIN_SIZE_LOGICAL.0, SETTINGS_MIN_SIZE_LOGICAL.1)
    .resizable(true)
    .visible(false);
    if let Some(monitor) = app.primary_monitor().ok().flatten() {
        let wa = monitor.work_area();
        let scale = monitor.scale_factor();
        builder = builder.max_inner_size(
            (wa.size.width as f64) / scale,
            (wa.size.height as f64) / scale,
        );
    }
    builder
        .build()
        .map_err(|e| AppError::Window(format!("settings 窗创建失败: {e}")))
        .inspect(|window| {
        // 居中失败不阻断（仅位置差异；默认 760×540 有边框常规窗体）。
        let _ = window.center();
        // 关闭=隐藏（FR-1.3：设置窗关闭仅隐藏，应用驻留托盘）。
        window.on_window_event(move |event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                if let Some(single) = app_clone.get_webview_window(WINDOW_SETTINGS_LABEL) {
                    let _ = single.hide();
                }
            }
            // 兜底钳制（真机修复轮 13e）：builder max_inner_size 之外的保险——松手后若内尺寸
            // 超过所在显示器工作区（贴边/拖拽异常态），压回工作区，绝不越出屏幕。
            WindowEvent::Resized(size) => clamp_settings_window(&app_clone, *size),
            _ => {}
        });
        // 真机修复轮 13f：强制 DWM 圆角——Win11 对贴边磁吸窗口会把贴边侧取直（底边贴任务栏
        // → 底部直角），用户实测被误读为"窗口被截断"。设 DWMWCP_ROUND 覆盖该自动行为。
        force_rounded_corners(window);
        // 真机修复轮 14b：原生标题栏随主题（深/浅）。
        crate::ui_text::apply_titlebar_theme(
            window,
            config::read_global().map(|s| s.general.theme).unwrap_or(config::ThemeMode::System),
        );
    })
}

/// 设 DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND（失败仅记录；Win10 无此属性时为 no-op）。
fn force_rounded_corners(window: &WebviewWindow) {
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let preference: i32 = DWMWCP_ROUND.0;
    // SAFETY：hwnd 取自 Tauri 窗口句柄（本应用自有窗口）；调用仅设置角落偏好，无内存写入。
    // 依赖图存在多个 windows crate 版本（tauri 内联一份），经裸指针转换到本 crate 的 HWND。
    let hr = unsafe {
        DwmSetWindowAttribute(
            windows::Win32::Foundation::HWND(hwnd.0),
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        )
    };
    if hr.is_err() {
        log::debug!("settings: 强制圆角失败（Win10 等无此属性，忽略）: {hr:?}");
    }
}

/// 设置窗尺寸防抖代次（与 translate 窗 RESIZE_GENERATION 同式：松手判定）。
static SETTINGS_RESIZE_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// 松手（300ms 静默）后复核内尺寸：超过所在屏工作区（逻辑）→ set_size 压回。失败仅记录。
pub fn clamp_settings_window(app: &AppHandle, _size: tauri::PhysicalSize<u32>) {
    use std::sync::atomic::Ordering;

    let generation = SETTINGS_RESIZE_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        if SETTINGS_RESIZE_GENERATION.load(Ordering::Relaxed) != generation {
            return; // 仍在拖拽
        }
        let Some(window) = app.get_webview_window(WINDOW_SETTINGS_LABEL) else {
            return;
        };
        if !window.is_visible().unwrap_or(false) {
            return;
        }
        let (Ok(scale), Ok(inner)) = (window.scale_factor(), window.inner_size()) else {
            return;
        };
        let Ok(Some(monitor)) = window.current_monitor() else {
            return;
        };
        let wa = monitor.work_area();
        let max_w = (wa.size.width as f64) / scale;
        let max_h = (wa.size.height as f64) / scale;
        let w = (inner.width as f64) / scale;
        let h = (inner.height as f64) / scale;
        if w <= max_w && h <= max_h {
            return; // 在界内：无需动作
        }
        let clamped_w = w.min(max_w);
        let clamped_h = h.min(max_h);
        log::warn!(
            "settings: 尺寸越界已压回（{w}x{h} → {clamped_w}x{clamped_h} 逻辑，工作区 {max_w}x{max_h}）"
        );
        let _ = window.set_size(tauri::LogicalSize::new(clamped_w, clamped_h));
    });
}

fn show_and_focus(_app: &AppHandle, window: &WebviewWindow) -> Result<(), AppError> {
    // i18n-1：标题按当前语言刷新（建窗后语言可能已切换）。
    let _ = window.set_title(crate::ui_text::resolve_global().settings_window_title());
    // 真机修复轮 14b：显示前按当前主题刷新标题栏深浅。
    crate::ui_text::apply_titlebar_theme(
        window,
        config::read_global().map(|s| s.general.theme).unwrap_or(config::ThemeMode::System),
    );
    window
        .show()
        .map_err(|e| AppError::Window(format!("settings 窗显示失败: {e}")))?;
    if let Err(err) = window.set_focus() {
        // 焦点失败不阻断显示（D7 前台握手兜底在 T15）。
        log::warn!("settings: 设置窗焦点失败（T15 前台握手兜底）: {err}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 单元测试：补丁应用全语义 / Key 空值与加密 / 专通道拒绝 / 广播剥离（INV-1）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use crate::events::{
        AutoTranslatePatch, GeneralPatch, MetaPatch, OcrPatch, PanelPatch, ScenePatch,
        TranslationPatch,
    };

    /// 无 unwrap/expect 的取值辅助（INV-6：clippy deny 覆盖测试；panic 仅用于测试断言失败）。
    fn ok_of<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(err) => panic!("期望成功值，实际错误: {err}"),
        }
    }

    #[test]
    fn apply_patch_updates_all_section_fields_preserving_others() {
        let mut s = config::Settings::default();
        ok_of(apply_settings_patch(
            &mut s,
            &SettingsPatch {
                translation: Some(TranslationPatch {
                    default_target_lang: Some("日文".to_string()),
                    presentation: Some(config::PresentationMode::Staged),
                    long_text_threshold_chars: Some(2000),
                    auto_translate: None,
                }),
                panel: Some(PanelPatch {
                    hide_on_blur: Some(false),
                    width_logical: Some(500),
                    height_logical: Some(400),
                    editor_height: Some(200),
                }),
                scene: Some(ScenePatch {
                    active: Some(config::SceneMode::Directed),
                }),
                ocr: Some(OcrPatch {
                    language: Some("ja".to_string()),
                }),
                general: Some(GeneralPatch {
                    autostart: Some(true),
                    theme: Some(config::ThemeMode::Dark),
                    language: Some(config::Language::En),
                }),
                ..Default::default()
            },
        ));
        assert_eq!(s.translation.default_target_lang, "日文");
        assert_eq!(s.translation.presentation, config::PresentationMode::Staged);
        assert_eq!(s.translation.long_text_threshold_chars, 2000);
        assert!(!s.panel.hide_on_blur);
        assert_eq!(s.panel.width_logical, 500);
        assert_eq!(s.panel.height_logical, 400);
        assert_eq!(s.panel.editor_height_logical, Some(200), "13i 输入框高度记忆");
        assert_eq!(s.scene.active, config::SceneMode::Directed);
        assert_eq!(s.ocr.language, "ja");
        assert!(s.general.autostart);
        assert_eq!(s.general.theme, config::ThemeMode::Dark);
        // 未触及字段保持出厂默认
        assert_eq!(s.ai.base_url, config::DEFAULT_BASE_URL);
        assert_eq!(s.hotkeys.selection, "Alt+Q");
    }

    #[test]
    fn apply_patch_auto_translate_enabled_and_delay_clamped() {
        let mut s = config::Settings::default();
        // 出厂默认：关 + 700ms（轮 12 用户需求：默认关，按回车翻译）。
        assert!(!s.translation.auto_translate.enabled);
        assert_eq!(s.translation.auto_translate.delay_ms, 700);
        ok_of(apply_settings_patch(
            &mut s,
            &SettingsPatch {
                translation: Some(TranslationPatch {
                    auto_translate: Some(AutoTranslatePatch {
                        enabled: Some(true),
                        delay_ms: Some(1000),
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
        ));
        assert!(s.translation.auto_translate.enabled);
        assert_eq!(s.translation.auto_translate.delay_ms, 1000);
        // 越界延迟按出厂默认 700 处理。
        ok_of(apply_settings_patch(
            &mut s,
            &SettingsPatch {
                translation: Some(TranslationPatch {
                    auto_translate: Some(AutoTranslatePatch {
                        enabled: None,
                        delay_ms: Some(1),
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
        ));
        assert_eq!(s.translation.auto_translate.delay_ms, 700);
    }

    #[test]
    fn apply_patch_editor_height_out_of_range_ignored() {
        // 真机修复轮 13i：编辑器高度越界（<56 / >5000）忽略，保留现值。
        let mut s = config::Settings::default();
        ok_of(apply_settings_patch(
            &mut s,
            &SettingsPatch {
                panel: Some(PanelPatch {
                    editor_height: Some(10),
                    ..Default::default()
                }),
                ..Default::default()
            },
        ));
        assert!(s.panel.editor_height_logical.is_none(), "过小值应被忽略");
        ok_of(apply_settings_patch(
            &mut s,
            &SettingsPatch {
                panel: Some(PanelPatch {
                    editor_height: Some(9999),
                    ..Default::default()
                }),
                ..Default::default()
            },
        ));
        assert!(s.panel.editor_height_logical.is_none(), "过大值应被忽略");
    }

    #[test]
    fn apply_patch_ai_fields_trimmed_and_meta_applied() {
        let mut s = config::Settings::default();
        ok_of(apply_settings_patch(
            &mut s,
            &SettingsPatch {
                ai: Some(AiPatch {
                    base_url: Some("  https://example.com/v1  ".to_string()),
                    model: Some("  custom-model  ".to_string()),
                    api_key: None,
                }),
                meta: Some(MetaPatch {
                    onboarding_completed: Some(true),
                }),
                ..Default::default()
            },
        ));
        assert_eq!(s.ai.base_url, "https://example.com/v1", "前后空白应裁剪");
        assert_eq!(s.ai.model, "custom-model");
        assert!(s.meta.onboarding_completed);
    }

    #[test]
    fn api_key_empty_clears_cipher_but_key_sets_encrypted_one() {
        // 先配置一个 Key（DPAPI 加密，哑值）
        let mut s = config::Settings::default();
        ok_of(apply_settings_patch(
            &mut s,
            &SettingsPatch {
                ai: Some(AiPatch {
                    api_key: Some("sk-fake-test-key-001".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        ));
        assert!(s.ai.api_key_cipher.is_some(), "非空 Key 应加密为密文");
        assert!(
            config::has_key(&s),
            "解密应成功（真机 DPAPI，等价 config 测试先例）"
        );

        // 空白（空字符串=未配置 → 清密文，design-spec 7.3）
        ok_of(apply_settings_patch(
            &mut s,
            &SettingsPatch {
                ai: Some(AiPatch {
                    api_key: Some("   ".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        ));
        assert!(s.ai.api_key_cipher.is_none(), "空白应清除密文");
        assert!(!config::has_key(&s));
    }

    #[test]
    fn apply_patch_rejects_hotkeys_and_clipboard_monitor_channels() {
        let mut s = config::Settings::default();
        let err = match apply_settings_patch(
            &mut s,
            &SettingsPatch {
                hotkeys: Some(Default::default()),
                ..Default::default()
            },
        ) {
            Ok(_) => panic!("热键补丁应被拒绝"),
            Err(err) => err,
        };
        assert_eq!(err.ex_code(), "config");
        let err = match apply_settings_patch(
            &mut s,
            &SettingsPatch {
                clipboard_monitor: Some(true),
                ..Default::default()
            },
        ) {
            Ok(_) => panic!("剪贴板监听补丁应被拒绝"),
            Err(err) => err,
        };
        assert_eq!(err.ex_code(), "config");
        assert!(!s.clipboard_monitor, "拒绝路径不得写入任何值");
    }

    #[test]
    fn broadcast_patch_strips_api_key_and_empty_ai() {
        // apiKey + 具名 ai 字段：广播保留 baseUrl/model，剥离 apiKey（INV-1）
        let with_both = broadcast_patch(&SettingsPatch {
            ai: Some(AiPatch {
                base_url: Some("https://a.invalid".to_string()),
                api_key: Some("sk-fake-secret".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        });
        let ai = with_both
            .ai
            .as_ref()
            .unwrap_or_else(|| panic!("ai 应保留（有 baseUrl）"));
        assert_eq!(ai.base_url.as_deref(), Some("https://a.invalid"));
        assert!(ai.api_key.is_none(), "广播必须剥离 apiKey（INV-1）");

        // 仅 apiKey：剥离后 ai 为空 → 整体省略
        let key_only = broadcast_patch(&SettingsPatch {
            ai: Some(AiPatch {
                api_key: Some("sk-fake-secret".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        });
        assert!(key_only.ai.is_none(), "仅 Key 补丁的广播应为空 ai");

        // 其他字段原样透传（hotkeys/clipboardMonitor 本通道恒 None）
        let rest = broadcast_patch(&SettingsPatch {
            ocr: Some(OcrPatch {
                language: Some("auto".to_string()),
            }),
            general: Some(GeneralPatch {
                theme: Some(config::ThemeMode::Light),
                ..Default::default()
            }),
            ..Default::default()
        });
        assert_eq!(
            rest.ocr
                .as_ref()
                .unwrap_or_else(|| panic!("ocr 应透传"))
                .language
                .as_deref(),
            Some("auto")
        );
        assert_eq!(
            rest.general
                .as_ref()
                .unwrap_or_else(|| panic!("general 应透传"))
                .theme,
            Some(config::ThemeMode::Light)
        );
        assert!(rest.clipboard_monitor.is_none());
        assert!(rest.hotkeys.is_none());
    }

    #[test]
    fn broadcast_patch_matches_settings_changed_type_semantics() {
        // 广播载荷类型与 SettingsPatch 相同（events.rs：SettingsChangedPayload = SettingsPatch）
        let patch = broadcast_patch(&SettingsPatch::default());
        let json = serde_json::to_string(&patch).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json, "{}", "空广播＝空对象（事件契约）");
    }
}
