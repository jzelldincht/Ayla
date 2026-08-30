//! events：6.1 命令参数/返回类型与 6.2 事件 payload 的唯一定义处（T3 实现；前端 src/lib/types.ts 手工镜像）。
//!
//! 约定：
//! - 字段序列化键一律 camelCase（`roundId`/`renderMode`/`sessionId` …），与 spec 6.1/6.2 字面一致；
//!   唯 `ex_code` 按 spec 6.2 字面保留下划线。
//! - 枚举值：`RenderMode`/`PanelMode`/`TranslateSource` 为 lowercase（stream|block 等，C1 裁决）；
//!   `HotkeyFunction` 为 camelCase（与 6.3 config.hotkeys 键名同风格）。
//! - INV-1：任何响应 payload 不携带 API Key 明文/密文——`get_settings` 返回 `SettingsView`，
//!   其中 `ai` 仅 `{baseUrl, model, hasKey}`；设置 Key 经 `SettingsPatch.ai.api_key` 明文进入
//!   Rust 内存后立即 DPAPI 加密存储；`settings://changed` 广播构造时必须剥离 `api_key` 字段
//!   （由后端事件发射点负责，本文件在 `SettingsChangedPayload` 上以注释固化该纪律）。
//! - INV-7：命令参数 `key` 属凭据输入，任何日志不得输出其值。

use serde::{Deserialize, Serialize};

use crate::config::{has_key, PresentationMode, SceneMode, Settings, ThemeMode};

// ---------------------------------------------------------------------------
// 事件名常量（唯一事件名源；前端 ipc.ts 引用同名字面量）
// ---------------------------------------------------------------------------

pub const EVENT_PANEL_SHOW: &str = "panel://show";
pub const EVENT_TRANSLATION_BEGIN: &str = "translation://begin";
pub const EVENT_TRANSLATION_CHUNK: &str = "translation://chunk";
pub const EVENT_TRANSLATION_DONE: &str = "translation://done";
pub const EVENT_TRANSLATION_ERROR: &str = "translation://error";
pub const EVENT_OCR_RESULT: &str = "ocr://result";
pub const EVENT_SETTINGS_CHANGED: &str = "settings://changed";
pub const EVENT_SELECTION_CLOSED: &str = "selection://closed";
/// `mask://begin`（T9 追加，主 Agent 批准的"data: URL 拉取"方案配套，见 implementation-report T9 节）：
/// 框选触发后 Rust 向各 `mask-{i}` 窗广播一次"新会话开始"（`MaskBeginPayload`，仅会话标识与屏号——
/// **不含位图**；冻结快照经 `get_mask_snapshot` 命令拉取）。前端收到后重新拉帧，消除池化窗显示旧帧。
pub const EVENT_MASK_BEGIN: &str = "mask://begin";

/// 翻译面板窗口 label（spec 5.5：`translate`；T11 预创建该窗）。
/// 翻译流式事件（begin/chunk/done/error）emit 目标即此 label，前后端共用唯一来源。
pub const WINDOW_TRANSLATE_LABEL: &str = "translate";

/// `onboarding://begin`（T14 追加事件，等价 panel://show 复用窗的状态复位通道）：
/// 引导窗每次显示时由 windows/onboarding 广播 `OnboardingBeginPayload { mode }`——
/// 常驻隐藏复用的引导页据此复位（首启四步 / guide 两步直达），消除复用窗残留旧状态。
pub const EVENT_ONBOARDING_BEGIN: &str = "onboarding://begin";

/// 遮罩窗 label 前缀（spec 5.5：`mask-{i}`，i 为按显示器 (x,y) 排序后的编号 0..n，T9）。
pub const WINDOW_MASK_LABEL_PREFIX: &str = "mask-";

// ---------------------------------------------------------------------------
// 共享枚举（截面/面板/命令通用）
// ---------------------------------------------------------------------------

/// 翻译源入口（6.1 `translate.source`；与 5.6 各链路 trigger source 对齐）。
/// `Hash`：T7 的会话表 `HashMap<Source, TranslationRound>` 键需要（spec 5.7.7 会话模型；
/// 与 HotkeyFunction 的 Hash 同先例——仅能力 derive 补充，序列化契约不变）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranslateSource {
    Selection,
    Input,
    Clipboard,
    Ocr,
}

/// 面板呈现形态（6.2 `panel://show.mode`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelMode {
    Selection,
    Input,
    Clipboard,
    Ocr,
}

/// 呈现模式（C1 裁决：`translation://begin.renderMode`，分档判定为 Rust 纯函数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RenderMode {
    /// 短文流式（默认呈现也统一走 stream 语义；整段模式内部同样流式消费）。
    Stream,
    /// 长文整段：等待期不渲染增量，完成后整段展示（FR-10.3）。
    Block,
}

/// 可改热键功能（FR-14.2：五功能；剪贴板监听不设热键）。
/// `Hash`：T4 的 `registered: HashMap<Function, Combo>` 需要（spec 5.7.1；仅能力补充，无契约变化）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HotkeyFunction {
    Selection,
    Input,
    OcrRecognize,
    OcrTranslate,
    ToggleScene,
}

/// 引导窗模式（T14 追加，`onboarding://begin.mode` + 窗口 URL 查询参数 `?mode=guide`）：
/// 首启四步向导（FR-11.1）与 FR-11.4 补配引导（guide 直达隐私要点 + Key 步骤）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnboardingMode {
    First,
    Guide,
}

// ---------------------------------------------------------------------------
// 6.1 命令参数 / 返回类型
// ---------------------------------------------------------------------------

/// `get_settings` 返回（INV-1：API Key 仅存在 `hasKey`；无密文无明文）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub schema_version: u32,
    pub ai: AiSettingsView,
    pub translation: crate::config::TranslationSettings,
    pub panel: crate::config::PanelSettings,
    pub clipboard_monitor: bool,
    pub scene: crate::config::SceneSettings,
    pub hotkeys: crate::config::HotkeySettings,
    pub ocr: crate::config::OcrSettings,
    pub general: crate::config::GeneralSettings,
    pub meta: crate::config::MetaSettings,
}

/// `SettingsView.ai` 子结构——对前端暴露的最小 AI 配置面（无 `apiKeyCipher`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettingsView {
    pub base_url: String,
    pub model: String,
    /// 经 DPAPI 解密判定的 Key 可用性（spec 5.7.8：解不出视同未配置）。
    pub has_key: bool,
}

impl SettingsView {
    /// 从配置仓库构造视图；`has_key` 走 `config::has_key`（解密失败降级为 false，不崩溃）。
    pub fn from_settings(settings: &Settings) -> Self {
        SettingsView {
            schema_version: settings.schema_version,
            ai: AiSettingsView {
                base_url: settings.ai.base_url.clone(),
                model: settings.ai.model.clone(),
                has_key: has_key(settings),
            },
            translation: settings.translation.clone(),
            panel: settings.panel.clone(),
            clipboard_monitor: settings.clipboard_monitor,
            scene: settings.scene.clone(),
            hotkeys: settings.hotkeys.clone(),
            ocr: settings.ocr.clone(),
            general: settings.general.clone(),
            meta: settings.meta.clone(),
        }
    }
}

/// `set_settings` 补丁（6.1）+ `settings://changed` 变更补丁（6.2，INV-8 三方同步镜像）。
/// 全部字段可选：缺省＝不变更；`SettingsPatch::default()`＝空补丁（序列化为 `{}`）。
/// 注意：热键变更不在此通道（走 `rebind_hotkey`，6.1）；广播构造时剥离 `ai.api_key`。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai: Option<AiPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation: Option<TranslationPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<PanelPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clipboard_monitor: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<ScenePatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkeys: Option<HotkeysPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ocr: Option<OcrPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub general: Option<GeneralPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<MetaPatch>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// API Key 明文入口（`set_settings` 通道）：仅作为 IPC 参数进入 Rust 内存即被 DPAPI
    /// 加密成 `ai.apiKeyCipher` 存储（INV-1：磁盘/日志/工件只有密文或 none）。
    /// **`settings://changed` 广播必经剥离此字段**（不发往任何 WebView）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TranslationPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_target_lang: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<PresentationMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_text_threshold_chars: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_translate: Option<AutoTranslatePatch>,
}

/// 原文变化自动翻译补丁（轮 12）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoTranslatePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PanelPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hide_on_blur: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_logical: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_logical: Option<u32>,
    /// 原文输入框高度（真机修复轮 13i：用户拖动后记忆，逻辑像素；缺省＝未拖过 → 黄金分割）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScenePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<SceneMode>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HotkeysPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ocr_recognize: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ocr_translate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggle_scene: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OcrPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GeneralPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autostart: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeMode>,
    /// 界面语言（i18n-1）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<crate::config::Language>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MetaPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onboarding_completed: Option<bool>,
}

/// `settings://changed` 事件 payload（INV-8 三方同步镜像；字段纪律见模块头）。
/// 类型语义与 `SettingsPatch` 相同，独立命名以显式区分「广播补丁」与「写入补丁」。
pub type SettingsChangedPayload = SettingsPatch;

/// `translate` 参数（6.1：{text, source} → {roundId}）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateCommand {
    pub text: String,
    pub source: TranslateSource,
}

/// `translate` / `send_ocr_to_translate` 返回（新轮标识，流式结果经事件返回）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateResult {
    pub round_id: String,
}

/// `cancel_translation` 参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelTranslationCommand {
    pub round_id: String,
}

/// `rebind_hotkey` 参数（{function, combo}）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebindHotkeyCommand {
    pub function: HotkeyFunction,
    pub combo: String,
}

/// `set_clipboard_monitor` 参数（三方开关统一入口，FR-5.1）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetClipboardMonitorCommand {
    pub enabled: bool,
}

/// `set_panel_target_lang` 参数（T12 追加命令——最小契约扩展，等价 C1 先例：
/// 6.1 原表无面板临时目标语言通道，而 FR-2.7 要求"面板下拉临时切换"且 `prompt.rs` 已备易失槽位；
/// 参数缺省/`lang` 空 = 清除临时值回落设置默认值，`Some` 须为 Q5 九语言文案之一）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SetPanelTargetLangCommand {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

/// 选区矩形（`complete_selection`；**所在显示器局部物理像素**，INV-4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// `complete_selection` 参数（框选确认；monitorIndex 为各屏编号 0..n）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteSelectionCommand {
    pub session_id: String,
    pub monitor_index: u32,
    pub rect: SelectionRect,
}

/// `cancel_selection` 参数（Esc 取消）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelSelectionCommand {
    pub session_id: String,
}

/// `get_mask_snapshot` 参数（T9 追加命令，主 Agent 批准方案二——遮罩页冻结帧经命令以 data: URL
/// 内存字符串返回；INV-3 说明见模块头/implementation-report T9 节：不落盘、无 HTTP、位图仅存于
/// Rust 会话与本次一次性返回，后续 IPC 仅传文本）。monitorIndex 从窗口 URL 查询参数 `?monitorIndex=`
/// 读得（窗口创建时按 label mask-{i} 写入，非 IPC）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMaskSnapshotCommand {
    pub monitor_index: u32,
}

/// `get_mask_snapshot` 返回：`{sessionId, monitorIndex, dataUrl}`（dataUrl＝`data:image/png;base64,...`
/// 内存字符串，窗口解析后仅绘制一次即弃，不持久化）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskSnapshotResult {
    pub session_id: String,
    pub monitor_index: u32,
    pub data_url: String,
}

/// `reocr` 参数（对保留裁剪块换语种重识别，FR-6.2）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReocrCommand {
    pub session_id: String,
    pub lang: String,
}

/// `reocr` 返回。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReocrResult {
    pub text: String,
}

/// `send_ocr_to_translate` 参数（一键送翻译，FR-6.5；与 `translate` 返回同型）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendOcrToTranslateCommand {
    pub session_id: String,
}

/// `verify_connection` 参数（缺省＝用已存值；D6：先 GET {base}/models，404/405 回退最小 chat）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VerifyConnectionCommand {
    pub base: Option<String>,
    pub model: Option<String>,
    /// 待验证 Key（用户输入，非已存密文）；INV-7：任何日志不得输出。
    pub key: Option<String>,
}

/// `list_ocr_languages` 返回单项（已装语言包，5.7.6；设置下拉源头，防 EX-6）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrLanguage {
    pub tag: String,
    pub label: String,
}

// ---------------------------------------------------------------------------
// 6.2 事件 payload（Rust→窗口）
// ---------------------------------------------------------------------------

/// `panel://show`：`{mode, text?, sessionId?}`（text：划词原文/截图识别形态附加的原文）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelShowPayload {
    pub mode: PanelMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// `translation://begin`：`{roundId, renderMode}`（C1 裁决：分档判定为 Rust 纯函数，前端按此渲染）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationBeginPayload {
    pub round_id: String,
    pub render_mode: RenderMode,
}

/// `translation://chunk`：增量 `delta`（纯文本追加，D9；微批合并 ≤16ms，5.7.7）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationChunkPayload {
    pub round_id: String,
    pub delta: String,
}

/// `translation://done`：本轮终态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationDonePayload {
    pub round_id: String,
}

/// `translation://error`：`{roundId, ex_code, message, retriable, partial?}`。
/// `ex_code` 取 `AppError::ex_code()`（"EX-1".."EX-14"）；`partial`＝EX-5 保留的已收增量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationErrorPayload {
    pub round_id: String,
    /// spec 6.2 字面：`ex_code`（保留下划线，与 roundId 的 camelCase 混用即为契约原样）。
    #[serde(rename = "ex_code")]
    pub ex_code: String,
    pub message: String,
    pub retriable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial: Option<String>,
}

impl TranslationErrorPayload {
    /// 由 AppError 与 roundId 装配（ex_code/message/retriable 单一来源，禁止前端另判）。
    pub fn from_error(
        round_id: impl Into<String>,
        err: &crate::error::AppError,
        partial: Option<String>,
    ) -> Self {
        TranslationErrorPayload {
            round_id: round_id.into(),
            ex_code: err.ex_code().to_string(),
            message: err.to_string(),
            retriable: err.retriable(),
            partial,
        }
    }
}

/// `ocr://result`：识别结果（截图识别形态；reocr 亦经此推送）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrResultPayload {
    pub session_id: String,
    pub text: String,
}

/// `selection://closed`：截图会话关闭（面板据此释放语种下拉等会话态）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionClosedPayload {
    pub session_id: String,
}

/// `mask://begin`（T9 内部引导事件，见模块头）：新框选会话开始。
/// 载荷**不含位图**——冻结帧经 `get_mask_snapshot` 命令按屏号拉取。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskBeginPayload {
    pub session_id: String,
    pub monitor_index: u32,
}

/// `onboarding://begin`（T14 追加）：引导窗复位指令——每次显示时广播当前模式
/// （首启四步 / guide 两步直达），复用窗页面据此复位并避免残留旧步骤状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingBeginPayload {
    pub mode: OnboardingMode,
}

// ---------------------------------------------------------------------------
// 单元测试：契约序列化形状（camelCase/ex_code/renderMode/无 Key 泄漏）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_show_payload_serde_shape() {
        let p = PanelShowPayload {
            mode: PanelMode::Input,
            text: None,
            session_id: None,
        };
        let json = serde_json::to_string(&p).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            json, r#"{"mode":"input"}"#,
            "可选字段缺省不出现（text?/sessionId?）"
        );
    }

    #[test]
    fn translation_begin_render_mode_c1_shape() {
        // C1 裁决：renderMode "stream"|"block"，roundId camelCase
        let p = TranslationBeginPayload {
            round_id: "r-1".into(),
            render_mode: RenderMode::Block,
        };
        let json = serde_json::to_string(&p).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json, r#"{"roundId":"r-1","renderMode":"block"}"#);
        let stream = RenderMode::Stream;
        assert_eq!(
            serde_json::to_string(&stream).unwrap_or_else(|e| panic!("{e}")),
            r#""stream""#
        );
    }

    #[test]
    fn translation_error_payload_keeps_ex_code_underscore() {
        // spec 6.2 字面：ex_code 下划线；roundId/retriable/partial camelCase
        let err = crate::error::AppError::ServerResponse("500 Internal Server Error".into());
        let p = TranslationErrorPayload::from_error("r-2", &err, Some("已收部分".into()));
        let json = serde_json::to_string(&p).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            json,
            r#"{"roundId":"r-2","ex_code":"EX-14","message":"翻译服务返回异常: 500 Internal Server Error","retriable":true,"partial":"已收部分"}"#
        );
        assert!(p.retriable);
        assert_eq!(p.ex_code, "EX-14");
    }

    #[test]
    fn settings_view_never_exposes_key_plaintext_or_cipher() {
        // INV-1：get_settings 产物不得含 apiKeyCipher / api_key 键
        let settings = Settings::default();
        let view = SettingsView::from_settings(&settings);
        let json = serde_json::to_string(&view).unwrap_or_else(|e| panic!("{e}"));
        assert!(!json.contains("apiKeyCipher"), "视图不得含密文键");
        assert!(!json.contains("api_key"), "视图不得含明文键");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap_or_else(|e| panic!("{e}"));
        let has_key_field = v.get("ai").and_then(|a| a.get("hasKey"));
        assert_eq!(
            has_key_field,
            Some(&serde_json::Value::Bool(false)),
            "ai.hasKey 必须存在（INV-1 前端唯一 Key 状态）"
        );
    }

    #[test]
    fn settings_patch_default_and_serde() {
        // 空补丁 = {}
        let patch = SettingsPatch::default();
        let json = serde_json::to_string(&patch).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json, "{}");
        // 局部补丁：仅改主题与监听
        let patch = SettingsPatch {
            general: Some(GeneralPatch {
                theme: Some(ThemeMode::Dark),
                ..Default::default()
            }),
            clipboard_monitor: Some(true),
            ..Default::default()
        };
        let json = serde_json::to_string(&patch).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            json, r#"{"clipboardMonitor":true,"general":{"theme":"dark"}}"#,
            "未改动字段不得出现"
        );
    }

    #[test]
    fn settings_patch_carries_api_key_in_but_must_be_stripped_on_broadcast() {
        // 写入通道允许 api_key 明文（进 Rust 内存即加密）；广播通道纪律由发射点剥离——
        // 此处验证 AiPatch 语义，并验证「构造无 api_key 的广播补丁」可行（类型层面不强制）。
        let wrote = SettingsPatch {
            ai: Some(AiPatch {
                api_key: Some("sk-fake-test".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&wrote).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            json.contains("apiKey"),
            "写入通道携带 apiKey（仅内存，不含密文键）"
        );

        let broadcast = SettingsPatch {
            ai: Some(AiPatch {
                model: Some("deepseek-chat".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&broadcast).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            !json.contains("api_key"),
            "广播补丁不得出现 api_key（INV-1）"
        );
    }

    #[test]
    fn command_types_serde_shape() {
        // 6.1 参数键名全貌（camelCase 抽查 + 枚举值抽查）
        let c = TranslateCommand {
            text: "你好".into(),
            source: TranslateSource::Input,
        };
        let json = serde_json::to_string(&c).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json, r#"{"text":"你好","source":"input"}"#);

        let r = RebindHotkeyCommand {
            function: HotkeyFunction::OcrRecognize,
            combo: "Alt+S".into(),
        };
        let json = serde_json::to_string(&r).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json, r#"{"function":"ocrRecognize","combo":"Alt+S"}"#);

        let sel = CompleteSelectionCommand {
            session_id: "s1".into(),
            monitor_index: 1,
            rect: SelectionRect {
                x: 10,
                y: 20,
                width: 100,
                height: 50,
            },
        };
        let json = serde_json::to_string(&sel).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            json,
            r#"{"sessionId":"s1","monitorIndex":1,"rect":{"x":10,"y":20,"width":100,"height":50}}"#
        );
    }

    #[test]
    fn mask_contract_types_serde_shape() {
        // mask://begin 载荷（camelCase；不含任何位图字段）
        let begin = MaskBeginPayload {
            session_id: "s7".into(),
            monitor_index: 0,
        };
        let json = serde_json::to_string(&begin).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json, r#"{"sessionId":"s7","monitorIndex":0}"#);

        // get_mask_snapshot 命令参数（仅 monitorIndex，无需 sessionId——后端取活动会话）
        let cmd = GetMaskSnapshotCommand { monitor_index: 1 };
        let json = serde_json::to_string(&cmd).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json, r#"{"monitorIndex":1}"#);

        // 返回（dataUrl 为内存 data: URL 字符串）
        let res = MaskSnapshotResult {
            session_id: "s7".into(),
            monitor_index: 1,
            data_url: "data:image/png;base64,QQ==".into(),
        };
        let json = serde_json::to_string(&res).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            json,
            r#"{"sessionId":"s7","monitorIndex":1,"dataUrl":"data:image/png;base64,QQ=="}"#
        );

        // 事件名常量与 TS 镜像保持一致（EVENTS.maskBegin = "mask://begin"；label 前缀）
        assert_eq!(EVENT_MASK_BEGIN, "mask://begin");
        assert_eq!(WINDOW_MASK_LABEL_PREFIX, "mask-");
    }

    #[test]
    fn onboarding_begin_payload_serde_shape() {
        // T14：onboarding://begin 载荷（camelCase {mode}；"first"|"guide"）与事件名
        let first = OnboardingBeginPayload {
            mode: OnboardingMode::First,
        };
        assert_eq!(
            serde_json::to_string(&first).unwrap_or_else(|e| panic!("{e}")),
            r#"{"mode":"first"}"#
        );
        let guide = OnboardingBeginPayload {
            mode: OnboardingMode::Guide,
        };
        assert_eq!(
            serde_json::to_string(&guide).unwrap_or_else(|e| panic!("{e}")),
            r#"{"mode":"guide"}"#
        );
        assert_eq!(EVENT_ONBOARDING_BEGIN, "onboarding://begin");
    }
}
