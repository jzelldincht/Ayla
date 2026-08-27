//! action_router：五动作统一入口与全链路汇聚（T16 完整实现；spec 5.4/5.6/7.2、FR-11.4、5.7.3）。
//!
//! 依赖方向：action_router → 各能力模块（selection/clipboard_monitor/ocr/capture/translate/windows）
//! → config/events；禁止反向、禁止模块间绕过本模块直接触发翻译（EX-13 顶替只有 service 一条路径）。
//!
//! 汇聚入口（全部经 `dispatch` 或本模块命令）：
//! - 热键（hotkey 回调注入）、托盘菜单/左键（tray）、单实例回调（lib.rs）→ `dispatch(Action)`；
//! - 剪贴板监听（ClipboardTranslateTrigger 注入）→ `clipboard_translate`（挂起/门控同径）；
//! - 框选确认延续（mask `SelectionCompletedHandler` 注入）→ `continue_ocr_flow`（识别/送翻分路）；
//! - 面板提交（`translate` / `send_ocr_to_translate` 命令）→ 本模块门控后进 service（EX-13 单路径）。
//!
//! 前置检查链（spec 5.6.1②a / FR-11.4）：
//! 1. onboarding 可见 → 动作入**挂起队列**（容量 1、易失——进程内 Mutex，重启即丢），流程终止；
//! 2. `hasKey==false`（翻译类入口）→ 不发起网络请求、打开引导窗 guide 模式、动作挂起（FR-3.4/4.4/7.3）；
//! 3. 框选期（截图触发）→ 面板强制隐藏（spec 5.7.3）；引导可见期间面板压制（挂起不弹）。
//!
//! 挂起恢复（FR-11.4）：`GuideFinishedHandler`（onboarding guide 完成/跳过）→ 取队列：
//! - `hasKey==true` → 自动继续执行该动作（按动作语义恢复：已取词直接弹板送翻 / 未取词重新取词 /
//!   输入直接弹板 / OCR 已识别直接展示+送翻 / 未框选重新框选）；
//! - `hasKey==false` → 丢弃并系统通知（FR-11.3 跳过路径）。
//!
//! 7.2 性能打点：三处打点（hotkey_received / selection_done / panel_visible）位于各自模块；
//! 本模块在 dispatch 首尾输出分段汇总日志（Debug 级，dev 构建开启——spec 7.4 发布默认关）。
//!
//! 锁序：本模块仅有队列/OCR 上下文本模块自锁（短临界区），不嵌套 config/sessions 锁；
//! 持有本模块锁期间不得调用任何可能重入本模块或长时间阻塞的接口（enqueue 仅变更内存态）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::capture;
use crate::clipboard_monitor;
use crate::config;
use crate::error::AppError;
use crate::events::{
    OcrResultPayload, PanelMode, PanelShowPayload, ReocrResult, SelectionClosedPayload,
    TranslateResult, TranslateSource, EVENT_OCR_RESULT, EVENT_SELECTION_CLOSED,
    WINDOW_TRANSLATE_LABEL,
};
use crate::hotkey::{self, HotkeyAction};
use crate::ocr::{self, OcrLang};
use crate::translate::sse::SseTranslateExecutor;
use crate::windows;

/// 五动作（与 spec 5.4 / T4 `HotkeyAction` 一一对应；名称即入口语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    SelectionTranslate,
    InputTranslate,
    OcrRecognize,
    OcrTranslate,
    ToggleScene,
}

impl From<HotkeyAction> for Action {
    fn from(value: HotkeyAction) -> Self {
        match value {
            HotkeyAction::SelectionTranslate => Action::SelectionTranslate,
            HotkeyAction::InputTranslate => Action::InputTranslate,
            HotkeyAction::OcrRecognize => Action::OcrRecognize,
            HotkeyAction::OcrTranslate => Action::OcrTranslate,
            HotkeyAction::ToggleScene => Action::ToggleScene,
        }
    }
}

/// 截图链路类型（框选会话从属：识别 / 识别后自动送翻）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrKind {
    Recognize,
    Translate,
}

/// 动作 → 链路计划（纯函数；`dispatch` 按此路由——单测覆盖五入口分发权威表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// 取词 → 面板（划词；剪贴板文本直接走面板，不重复取词）。
    Selection,
    /// 面板输入形态。
    Input,
    /// 截图框选（含识别/自动送翻分路）。
    Capture(OcrKind),
    /// 场景切换（写 config + 广播，INV-8）。
    ToggleScene,
}

pub fn flow_of(action: Action) -> Flow {
    match action {
        Action::SelectionTranslate => Flow::Selection,
        Action::InputTranslate => Flow::Input,
        Action::OcrRecognize => Flow::Capture(OcrKind::Recognize),
        Action::OcrTranslate => Flow::Capture(OcrKind::Translate),
        Action::ToggleScene => Flow::ToggleScene,
    }
}

// ---------------------------------------------------------------------------
// 挂起队列（FR-11.4：容量 1、易失、重启丢弃——进程内 Mutex<Option<..>>）
// ---------------------------------------------------------------------------

/// 挂起动作语义（恢复时的执行方式；与 `Action` 解耦——同一动作可能挂起于不同阶段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingKind {
    /// 划词翻译：`Some(text)`＝取词已完成（恢复直接弹板送翻）；`None`＝挂起于取词前（恢复重新取词）。
    Selection { text: Option<String> },
    /// 输入翻译：恢复直接弹出输入面板。
    Input,
    /// 剪贴板自动翻译：文本已捕获（恢复直接弹板自动送翻）。
    Clipboard(String),
    /// 截图识别：挂起于框选前（恢复重新发起框选）。
    OcrRecognize,
    /// 截图翻译：挂起于框选前（OcrRecognize 语义同）或 OCR 已完成后（带文本/会话：恢复直接送翻）。
    OcrTranslate {
        text: Option<String>,
        session_id: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingAction {
    pub kind: PendingKind,
}

impl PendingAction {
    pub fn new(kind: PendingKind) -> Self {
        PendingAction { kind }
    }
}

/// 挂起队列（容量 1；last-wins——最新触发覆盖旧挂起，与 EX-13「仅呈现最新一轮」同语义）。
#[derive(Debug, Default)]
pub struct PendingQueue {
    inner: Mutex<Option<PendingAction>>,
}

impl PendingQueue {
    pub fn new() -> Self {
        PendingQueue::default()
    }

    pub fn len(&self) -> usize {
        self.lock()
            .map(|guard| usize::from(guard.is_some()))
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 入队（容量 1：占用时替换——最新一次触发优先）。
    pub fn enqueue(&self, action: PendingAction) {
        let mut guard = self.lock_guard();
        if let Some(old) = guard.as_ref() {
            log::debug!(
                "action_router: 挂起队列容量 1，替换旧动作（{:?} → {:?}）",
                old.kind,
                action.kind
            );
        }
        log::debug!("action_router: 翻译动作已挂起（{:?}）", action.kind);
        *guard = Some(action);
    }

    /// 取走挂起动作（恢复/丢弃后队列即空；重启＝新进程空队列，天然丢弃）。
    pub fn take(&self) -> Option<PendingAction> {
        self.lock_guard().take()
    }

    /// 只读观察（诊断/测试）。
    pub fn peek(&self) -> Option<PendingAction> {
        self.lock_guard().clone()
    }

    fn lock_guard(&self) -> MutexGuard<'_, Option<PendingAction>> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock(&self) -> Option<MutexGuard<'_, Option<PendingAction>>> {
        self.inner.lock().ok()
    }
}

static PENDING: OnceLock<PendingQueue> = OnceLock::new();

/// 全局挂起队列（进程内单份，易失态——spec 6.3 分离表）。
pub fn pending_queue() -> &'static PendingQueue {
    PENDING.get_or_init(PendingQueue::new)
}

fn enqueue(kind: PendingKind) {
    pending_queue().enqueue(PendingAction::new(kind));
}

// ---------------------------------------------------------------------------
// OCR 上下文（框选会话 → 类型/语种/文本；会话属易失态，随新会话替换）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct OcrContext {
    kind: OcrKind,
    lang: String,
    text: Option<String>,
}

static OCR_CONTEXTS: OnceLock<Mutex<HashMap<String, OcrContext>>> = OnceLock::new();

fn ocr_contexts() -> &'static Mutex<HashMap<String, OcrContext>> {
    OCR_CONTEXTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_ocr_context(session_id: &str, context: OcrContext) {
    let mut map = ocr_contexts()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    log::debug!(
        "action_router: 框选会话 {session_id} 上下文已登记（kind={:?}, lang={}）",
        context.kind,
        context.lang
    );
    map.insert(session_id.to_string(), context);
}

/// 清理会话上下文（新框选替换 / 会话关闭时调用）。
pub fn clear_ocr_context(session_id: &str) {
    let mut map = ocr_contexts()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if map.remove(session_id).is_some() {
        log::debug!("action_router: 框选会话 {session_id} 上下文已释放");
    }
}

fn ocr_context_of(session_id: &str) -> Option<OcrContext> {
    let map = ocr_contexts()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    map.get(session_id).cloned()
}

fn ocr_text_of(session_id: &str) -> Option<String> {
    ocr_context_of(session_id).and_then(|ctx| ctx.text)
}

/// 更新会话语种/文本（reocr 与识别完成后调用）。
fn update_ocr_text_and_lang(session_id: &str, lang: String, text: String) {
    let mut map = ocr_contexts()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(ctx) = map.get_mut(session_id) {
        ctx.lang = lang;
        ctx.text = Some(text);
    }
}

// ---------------------------------------------------------------------------
// 门控与恢复决策（纯函数，随任务单测）
// ---------------------------------------------------------------------------

/// 前置检查链决策（spec 5.6.1②a / FR-11.4）：
/// onboarding 可见 → 挂起（去引导）；否则翻译类且无 Key → 挂起 + 弹配置引导；否则放行。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateDecision {
    Proceed,
    SuspendForOnboarding,
    SuspendForGuide,
}

pub fn decide_entry(onboarding_visible: bool, needs_key: bool, has_key: bool) -> GateDecision {
    if onboarding_visible {
        GateDecision::SuspendForOnboarding
    } else if needs_key && !has_key {
        GateDecision::SuspendForGuide
    } else {
        GateDecision::Proceed
    }
}

/// 补配恢复决策（FR-11.4）：无挂起无事；有挂起且已有 Key → 继续执行；无 Key（跳过/清空）→ 丢弃。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeDecision {
    Nothing,
    Discard,
    Continue,
}

pub fn decide_resume(pending_exists: bool, has_key: bool) -> ResumeDecision {
    if !pending_exists {
        ResumeDecision::Nothing
    } else if has_key {
        ResumeDecision::Continue
    } else {
        ResumeDecision::Discard
    }
}

// ---------------------------------------------------------------------------
// 动作分发（唯一入口；T16 完整 handler 由 wire() 注入）
// ---------------------------------------------------------------------------

/// 动作分发回调注入接口（T16 用完整 action_router 实现并 `set_action_handler` 注入；
/// 未注入时经 minimal 直通——见最小直通段注释，生产路径无此形态）。
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

/// 五动作唯一入口（热键/托盘/单实例回调均经此）。
pub fn dispatch(app: &AppHandle, action: Action) {
    let started = Instant::now();
    log::debug!("action_router: dispatch 起始（{action:?}）");
    match HANDLER.get() {
        Some(handler) => handler.dispatch(app, action),
        None => {
            log::error!("action_router: handler 未注入（T16 wire 未执行），走 minimal 直通");
            minimal_dispatch(app, action);
        }
    }
    log::debug!(
        "action_router: dispatch 完毕（{action:?}，{} ms）",
        started.elapsed().as_millis()
    );
}

// ---------------------------------------------------------------------------
// T16 完整实现：Router（ActionHandler）
// ---------------------------------------------------------------------------

/// 完整路由器：按 flow_of 路由五动作并执行前置检查链（AppHandle 随回调传参，无自有状态）。
struct Router;

impl ActionHandler for Router {
    fn dispatch(&self, app: &AppHandle, action: Action) {
        match flow_of(action) {
            Flow::Selection => self.selection_translate(app),
            Flow::Input => self.input_translate(app),
            Flow::Capture(kind) => self.ocr_start(app, kind),
            Flow::ToggleScene => self.toggle_scene(app),
        }
    }
}

impl Router {
    /// 划词翻译（spec 5.6.1）：onboarding 挂起 → 取词 → hasKey 门控 → 面板（前端自动送翻）。
    fn selection_translate(&self, app: &AppHandle) {
        if onboarding_visible(app) {
            enqueue(PendingKind::Selection { text: None });
            return;
        }
        // 取词（INV-5：全程不显示/聚焦 Alya 窗口；EX-9/EX-10 通知在模块内完成）。
        let text = match crate::selection::run_selection(app) {
            Ok(text) => text,
            Err(_) => return,
        };
        if !has_key_now() {
            enqueue(PendingKind::Selection {
                text: Some(text.clone()),
            });
            if let Err(err) = open_guide_for(app) {
                log::error!("action_router: 划词翻译弹配置引导失败: {err}");
            }
            return;
        }
        fail_log(
            "划词翻译",
            show_panel(app, PanelMode::Selection, Some(text), None),
        );
    }

    /// 输入翻译（spec 5.6.4 / Q8 / EX-12）：onboarding 挂起 → hasKey 门控 → 输入面板。
    fn input_translate(&self, app: &AppHandle) {
        if onboarding_visible(app) {
            enqueue(PendingKind::Input);
            return;
        }
        if !has_key_now() {
            enqueue(PendingKind::Input);
            if let Err(err) = open_guide_for(app) {
                log::error!("action_router: 输入翻译弹配置引导失败: {err}");
            }
            return;
        }
        fail_log("输入翻译", show_panel(app, PanelMode::Input, None, None));
    }

    /// 截图识别/翻译（spec 5.6.2）：onboarding 挂起 → 面板强制隐藏 → 框选会话（识别/送翻分路经
    /// `SelectionCompletedHandler` 回调延续，见 `continue_ocr_flow`）。
    fn ocr_start(&self, app: &AppHandle, kind: OcrKind) {
        if onboarding_visible(app) {
            enqueue(match kind {
                OcrKind::Recognize => PendingKind::OcrRecognize,
                OcrKind::Translate => PendingKind::OcrTranslate {
                    text: None,
                    session_id: None,
                },
            });
            return;
        }
        // 框选进行中面板强制隐藏（spec 5.7.3 交汇矩阵；隐藏幂等，不取消进行中翻译 D10）。
        if let Err(err) = windows::translate::hide_panel(app) {
            log::warn!("action_router: 截图触发表前板隐藏失败: {err}");
        }
        let old_session = capture::active_session_id();
        match windows::mask::start_capture_session(app) {
            Ok(session_id) => {
                // 旧会话被替换：通知面板释放会话态（selection://closed）并清理路由器上下文。
                if let Some(old) = old_session {
                    let _ = app.emit_to(
                        WINDOW_TRANSLATE_LABEL,
                        EVENT_SELECTION_CLOSED,
                        SelectionClosedPayload {
                            session_id: old.clone(),
                        },
                    );
                    clear_ocr_context(&old);
                }
                let lang = config::read_global()
                    .map(|guard| guard.ocr.language.clone())
                    .unwrap_or_else(|err| {
                        log::error!("action_router: 读取 OCR 语种配置失败（回落 auto）: {err}");
                        "auto".to_string()
                    });
                register_ocr_context(
                    &session_id,
                    OcrContext {
                        kind,
                        lang,
                        text: None,
                    },
                );
            }
            Err(err) => log::error!("action_router: 截图框选触发失败: {err}"),
        }
    }

    /// 剪贴板自动翻译（spec 5.6.3 触发出口；文本已捕获，与划词同径但不再取词）。
    fn clipboard_translate(&self, app: &AppHandle, text: String) {
        match decide_entry(onboarding_visible(app), true, has_key_now()) {
            GateDecision::Proceed => {
                fail_log(
                    "剪贴板翻译",
                    show_panel(app, PanelMode::Clipboard, Some(text), None),
                );
            }
            GateDecision::SuspendForOnboarding => enqueue(PendingKind::Clipboard(text)),
            GateDecision::SuspendForGuide => {
                enqueue(PendingKind::Clipboard(text));
                if let Err(err) = open_guide_for(app) {
                    log::error!("action_router: 剪贴板翻译弹配置引导失败: {err}");
                }
            }
        }
    }

    /// 场景切换（Alt+W）：写 config → 原子落盘 → `settings://changed` 广播（INV-8，与托盘子菜单同径）。
    fn toggle_scene(&self, app: &AppHandle) {
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

// ---------------------------------------------------------------------------
// 回调注入实现（热键 / 剪贴板 / 引导完成 / 框选延续）
// ---------------------------------------------------------------------------

/// 热键回调：HotkeyAction → Action → dispatch（EX-13 单路径的入口收敛）。
struct RouterHotkeyCallback {
    app: AppHandle,
}

impl crate::hotkey::HotkeyCallback for RouterHotkeyCallback {
    fn dispatch(&self, action: HotkeyAction) {
        dispatch(&self.app, Action::from(action));
    }
}

/// 剪贴板触发回调：文本已由状态机捕获（仅 On 态 tick 才触发——Off 态不会到达本回调）。
struct RouterClipboardTrigger {
    app: AppHandle,
}

impl clipboard_monitor::ClipboardTranslateTrigger for RouterClipboardTrigger {
    fn on_clipboard_text(&self, text: String) {
        Router.clipboard_translate(&self.app, text);
    }
}

/// 引导完成回调（FR-11.4 恢复入口：取挂起队列 → 有 Key 继续 / 无 Key 丢弃通知）。
struct RouterGuideFinished {
    app: AppHandle,
}

impl windows::onboarding::GuideFinishedHandler for RouterGuideFinished {
    fn guide_finished(&self, has_key: bool) {
        resume_pending(&self.app, has_key);
    }
}

/// 框选延续回调（mask 命令裁剪成功后调用；识别/送翻分路在 `continue_ocr_flow`）。
struct RouterSelectionCompleted;

// ---------------------------------------------------------------------------
// 装配：wire（lib.rs setup 调用；重复调用直接报错——OnceLock 语义）
// ---------------------------------------------------------------------------

/// T16 装配：注入完整 handler 并接线全部回调（hotkey / clipboard / onboarding / mask）。
/// 依赖方向全部为 action_router → 各能力模块（spec 5.4，无反向）。
pub fn wire(app: &AppHandle) -> Result<(), AppError> {
    set_action_handler(Box::new(Router))?;
    hotkey::set_callback(Box::new(RouterHotkeyCallback { app: app.clone() }))?;
    clipboard_monitor::set_translate_trigger(Box::new(RouterClipboardTrigger {
        app: app.clone(),
    }))?;
    windows::onboarding::set_guide_finished_handler(Box::new(RouterGuideFinished {
        app: app.clone(),
    }))?;
    windows::mask::set_selection_completed_handler(Box::new(RouterSelectionCompleted))?;
    log::info!("action_router: 全链路汇聚已接线（热键/剪贴板/引导完成/框选延续 + 四命令）");
    Ok(())
}

// ---------------------------------------------------------------------------
// 门控与恢复执行
// ---------------------------------------------------------------------------

/// 引导可见判定（与 windows/translate 的压制投影同口径：onboarding 窗存在且可见）。
fn onboarding_visible(app: &AppHandle) -> bool {
    match app.get_webview_window(windows::onboarding::WINDOW_ONBOARDING_LABEL) {
        Some(window) => window.is_visible().unwrap_or(false),
        None => false,
    }
}

/// hasKey（DPAPI 解密口径，config::has_key 降级语义；读配置失败按无 Key 处理）。
fn has_key_now() -> bool {
    match config::read_global() {
        Ok(guard) => config::has_key(&guard),
        Err(err) => {
            log::error!("action_router: hasKey 判定读取配置失败（视同未配置）: {err}");
            false
        }
    }
}

/// 打开 FR-11.4 配置引导（guide 模式）：先隐藏面板（引导期面板压制——否则 topmost 面板会盖住引导窗），
/// 再复用引导窗直达隐私要点 + Key 步骤。
fn open_guide_for(app: &AppHandle) -> Result<(), AppError> {
    if let Err(err) = windows::translate::hide_panel(app) {
        log::warn!("action_router: 打开配置引导前面板隐藏失败（压制语义）: {err}");
    }
    let result = windows::onboarding::open_config_guide(app.clone());
    match &result {
        Ok(()) => log::info!("action_router: FR-11.4 配置引导已打开（guide 模式），翻译动作已挂起"),
        Err(err) => log::error!("action_router: 配置引导打开失败: {err}"),
    }
    result
}

/// FR-11.4 恢复入口（guide 完成/跳过回调）：取挂起队列 → 有 Key 继续 / 无 Key 丢弃并通知。
fn resume_pending(app: &AppHandle, has_key: bool) {
    let pending = pending_queue().take();
    match decide_resume(pending.is_some(), has_key) {
        ResumeDecision::Nothing => {
            log::debug!("action_router: 引导完成，挂起队列为空（无待恢复动作）");
        }
        ResumeDecision::Discard => {
            log::info!("action_router: 补配完成但仍未配置 Key（跳过/清空），挂起翻译已丢弃");
            if let Err(err) = app
                .notification()
                .builder()
                .title("Alya")
                .body("已跳过 API Key 配置，本次触发的翻译未执行。")
                .show()
            {
                log::error!("action_router: 挂起丢弃通知发送失败: {err}");
            }
        }
        ResumeDecision::Continue => {
            if let Some(action) = pending {
                log::info!(
                    "action_router: 补配成功，恢复挂起翻译动作（{:?}）",
                    action.kind
                );
                execute_resume(app, action);
            }
        }
    }
}

/// 执行挂起动作恢复（按挂起阶段的语义；应仅经 resume_pending 调用）。
fn execute_resume(app: &AppHandle, pending: PendingAction) {
    match pending.kind {
        PendingKind::Selection { text: Some(text) } => {
            // 取词已完成：直接弹板（前端形态 A 自动送翻），不再重取词。
            fail_log(
                "挂起恢复（划词）",
                show_panel(app, PanelMode::Selection, Some(text), None),
            );
        }
        PendingKind::Selection { text: None } => {
            // 挂起于取词前（onboarding 期触发）：恢复重新走完整划词链（门控此刻应已放行）。
            Router.selection_translate(app);
        }
        PendingKind::Input => {
            fail_log(
                "挂起恢复（输入）",
                show_panel(app, PanelMode::Input, None, None),
            );
        }
        PendingKind::Clipboard(text) => {
            fail_log(
                "挂起恢复（剪贴板）",
                show_panel(app, PanelMode::Clipboard, Some(text), None),
            );
        }
        PendingKind::OcrRecognize => {
            Router.ocr_start(app, OcrKind::Recognize);
        }
        PendingKind::OcrTranslate {
            text: Some(text),
            session_id,
        } => {
            // OCR 已完成（Alt+D 送翻前/送翻时挂起）：不再重新识别，直接展示 + 送翻。
            let session_id = session_id.filter(|sid| capture::session_is_current(sid));
            fail_log(
                "挂起恢复（截图翻译）",
                show_panel(app, PanelMode::Ocr, Some(text.clone()), session_id.clone()),
            );
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(err) = start_round(&app, TranslateSource::Ocr, text).await {
                    log::error!("action_router: 挂起恢复送翻发起失败: {err}");
                }
            });
        }
        PendingKind::OcrTranslate {
            text: None,
            session_id: _,
        } => {
            // 挂起于框选前：恢复重新发起框选（原动作语义不变）。
            Router.ocr_start(app, OcrKind::Translate);
        }
    }
}

// ---------------------------------------------------------------------------
// 命令（6.1：translate / cancel_translation / reocr / send_ocr_to_translate）
// ---------------------------------------------------------------------------

/// `translate {text, source} → {roundId}`（6.1）：面板提交入口（形态 A/B 自动送翻/手动提交）。
/// 门控（FR-11.4）：onboarding 可见 / 无 Key → 挂起（队列容量 1）并返回错误（面板已被压制隐藏），
/// 补配后自动继续；否则经 service 发起（EX-13 单路径）。
#[tauri::command]
pub async fn translate(
    app: AppHandle,
    text: String,
    source: TranslateSource,
) -> Result<TranslateResult, AppError> {
    submit_translation(&app, text, source).await
}

/// `cancel_translation {roundId} → Result`（6.1）：显式取消（Esc 中止进行中翻译，design-spec 3.5）。
#[tauri::command]
pub async fn cancel_translation(_app: AppHandle, round_id: String) -> Result<(), AppError> {
    let service = crate::translate::service::global();
    service.cancel_translation(&round_id).await
}

/// `reocr {sessionId, lang} → {text}`（6.1/FR-6.2）：对保留裁剪块换语种重识别；
/// 结果经 `ocr://result` 事件推送（面板镜像）+ 返回值（调用方契约）。
#[tauri::command]
pub async fn reocr(
    app: AppHandle,
    session_id: String,
    lang: String,
) -> Result<ReocrResult, AppError> {
    let crop = capture::get_crop(&session_id)?;
    let text = ocr::recognize(crop.png_bytes, OcrLang::from_config(&lang)).await?;
    update_ocr_text_and_lang(&session_id, lang, text.clone());
    let payload = OcrResultPayload {
        session_id: session_id.clone(),
        text: text.clone(),
    };
    if let Err(err) = app.emit_to(WINDOW_TRANSLATE_LABEL, EVENT_OCR_RESULT, payload) {
        log::warn!("action_router: ocr://result 推送失败（返回值已保证契约）: {err}");
    }
    Ok(ReocrResult { text })
}

/// `send_ocr_to_translate {sessionId} → {roundId}`（6.1/FR-6.5）：截图识别结果一键送翻。
/// 门控（FR-7.3/FR-11.4）：无 Key → 挂起文本并弹配置引导（OCR 已照常完成，仅翻译环节转引导）。
#[tauri::command]
pub async fn send_ocr_to_translate(
    app: AppHandle,
    session_id: String,
) -> Result<TranslateResult, AppError> {
    let text = ocr_text_of(&session_id)
        .ok_or_else(|| AppError::Capture("截图会话无识别文本（尚未完成识别）".to_string()))?;
    match decide_entry(onboarding_visible(&app), true, has_key_now()) {
        GateDecision::Proceed => start_round(&app, TranslateSource::Ocr, text).await,
        GateDecision::SuspendForOnboarding => {
            enqueue(PendingKind::OcrTranslate {
                text: Some(text.clone()),
                session_id: Some(session_id.clone()),
            });
            Err(AppError::Config(
                "配置引导进行中，本次截图翻译已挂起，配置完成后自动继续".to_string(),
            ))
        }
        GateDecision::SuspendForGuide => {
            enqueue(PendingKind::OcrTranslate {
                text: Some(text.clone()),
                session_id: Some(session_id.clone()),
            });
            open_guide_for(&app)?;
            Err(AppError::Config(
                "尚未配置 API Key，已打开配置引导；配置完成后自动继续翻译".to_string(),
            ))
        }
    }
}

/// 面板/命令侧翻译提交（FR-11.4 门 + EX-13 单路径；sse 执行器为生产实现）。
pub async fn submit_translation(
    app: &AppHandle,
    text: String,
    source: TranslateSource,
) -> Result<TranslateResult, AppError> {
    if text.trim().is_empty() {
        return Err(AppError::EmptyText("输入为空，未发起翻译".to_string()));
    }
    match decide_entry(onboarding_visible(app), true, has_key_now()) {
        GateDecision::Proceed => start_round(app, source, text).await,
        GateDecision::SuspendForOnboarding => {
            enqueue(pending_of_submit(source, &text));
            Err(AppError::Config(
                "配置引导进行中，本次翻译已挂起，配置完成后自动继续".to_string(),
            ))
        }
        GateDecision::SuspendForGuide => {
            enqueue(pending_of_submit(source, &text));
            open_guide_for(app)?;
            Err(AppError::Config(
                "尚未配置 API Key，已打开配置引导；配置完成后自动继续翻译".to_string(),
            ))
        }
    }
}

fn pending_of_submit(source: TranslateSource, text: &str) -> PendingKind {
    match source {
        TranslateSource::Selection => PendingKind::Selection {
            text: Some(text.to_string()),
        },
        TranslateSource::Input => PendingKind::Input,
        TranslateSource::Clipboard => PendingKind::Clipboard(text.to_string()),
        TranslateSource::Ocr => PendingKind::OcrTranslate {
            text: Some(text.to_string()),
            session_id: None,
        },
    }
}

/// 发起一轮（service 顶替语义 EX-13 单路径；执行器构造仅持 AppHandle，无状态）。
async fn start_round(
    app: &AppHandle,
    source: TranslateSource,
    text: String,
) -> Result<TranslateResult, AppError> {
    let executor: Arc<SseTranslateExecutor> = Arc::new(SseTranslateExecutor::new(app.clone()));
    crate::translate::service::global()
        .start_translate(source, text, &executor)
        .await
}

// ---------------------------------------------------------------------------
// 框选延续（mask 注入回调的 handler；识别/送翻分路）
// ---------------------------------------------------------------------------

impl windows::mask::SelectionCompletedHandler for RouterSelectionCompleted {
    fn on_completed(&self, app: &AppHandle, session_id: String, monitor_index: u32) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(err) = continue_ocr_flow(&app, &session_id, monitor_index).await {
                log::error!("action_router: 框选延续流程失败 sessionId={session_id}: {err}");
            }
        });
    }
}

/// 框选完成的延续流程（spec 5.6.2 ④~⑧；裁剪与遮罩隐藏已完成）：
/// 识别 → 分路（识别形态 / 送翻门控+面板+流式翻译；EX-6 系统通知；FR-7.2 空文本提示）。
async fn continue_ocr_flow(
    app: &AppHandle,
    session_id: &str,
    monitor_index: u32,
) -> Result<(), AppError> {
    let context = match ocr_context_of(session_id) {
        Some(context) => context,
        None => {
            // 会话非本模块发起（防御路径）：仅记日志，不产生任何动作。
            log::debug!(
                "action_router: 框选延续忽略（无会话上下文，可能为直接命令触发）sessionId={session_id}"
            );
            return Ok(());
        }
    };
    let crop = capture::get_crop(session_id)?;
    let text = match ocr::recognize(crop.png_bytes, OcrLang::from_config(&context.lang)).await {
        Ok(text) => text,
        Err(err) => {
            notify_ocr_error(app, &err);
            return Ok(());
        }
    };
    log::debug!(
        "action_router: OCR 完成 sessionId={session_id} 屏 {monitor_index}（{} 字符）",
        text.chars().count()
    );
    update_ocr_text_and_lang(session_id, context.lang.clone(), text.clone());

    match context.kind {
        OcrKind::Recognize => {
            // 识别模式：面板识别形态（空文本 = FR-7.2 面板提示，不送翻）。
            fail_log(
                "截图识别面板",
                show_panel(
                    app,
                    PanelMode::Ocr,
                    Some(text),
                    Some(session_id.to_string()),
                ),
            );
        }
        OcrKind::Translate => {
            if text.trim().is_empty() {
                // FR-7.2：识别空文本 → 面板提示"未识别到文字"，不发起翻译请求。
                fail_log(
                    "截图翻译空文本提示",
                    show_panel(
                        app,
                        PanelMode::Ocr,
                        Some(String::new()),
                        Some(session_id.to_string()),
                    ),
                );
                return Ok(());
            }
            match decide_entry(onboarding_visible(app), true, has_key_now()) {
                GateDecision::Proceed => {}
                _ => {
                    // FR-7.3：OCR 已照常完成，翻译环节转配置引导（挂起文本）。
                    enqueue(PendingKind::OcrTranslate {
                        text: Some(text.clone()),
                        session_id: Some(session_id.to_string()),
                    });
                    open_guide_for(app)?;
                    return Ok(());
                }
            }
            fail_log(
                "截图翻译面板",
                show_panel(
                    app,
                    PanelMode::Ocr,
                    Some(text.clone()),
                    Some(session_id.to_string()),
                ),
            );
            start_round(app, TranslateSource::Ocr, text).await?;
        }
    }
    Ok(())
}

/// OCR 错误呈现（EX-6 → 系统通知附官方指引；内部错误 → 通用通知，不静默失败）。
fn notify_ocr_error(app: &AppHandle, err: &AppError) {
    let body = if err.ex_code() == "EX-6" {
        err.to_string()
    } else {
        log::warn!("action_router: OCR 运行期失败（内部编码）: {err}");
        "截图识别失败，请重试。".to_string()
    };
    if let Err(notify_err) = app.notification().builder().title("Alya").body(body).show() {
        log::error!("action_router: OCR 错误通知发送失败: {notify_err}");
    }
}

// ---------------------------------------------------------------------------
// 既有接口：minimal 直通（异常装配路径；T16 完整路径经 wire()+dispatch）
// ---------------------------------------------------------------------------

fn minimal_dispatch(app: &AppHandle, action: Action) {
    match action {
        Action::InputTranslate => {
            fail_log("输入翻译", show_panel(app, PanelMode::Input, None, None));
        }
        Action::SelectionTranslate => match crate::selection::run_selection(app) {
            Ok(text) => fail_log(
                "划词翻译",
                show_panel(app, PanelMode::Selection, Some(text), None),
            ),
            Err(err) => log::debug!(
                "action_router: 划词翻译取消（{}，通知已由 selection 模块发出）",
                err.ex_code()
            ),
        },
        Action::OcrRecognize | Action::OcrTranslate => {
            if let Err(err) = windows::mask::start_capture_session(app) {
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
    windows::translate::show_panel(
        app,
        PanelShowPayload {
            mode,
            text,
            session_id,
        },
    )
}

/// 场景变更（托盘子菜单单选 / ToggleScene 共用）：写 config → 原子落盘 → `settings://changed`
/// 广播（INV-8 三方镜像：面板分段控件/托盘勾选/设置页；托盘经监听重建菜单——无递归，重建不带事件）。
/// 面板分段控件经 `set_settings.scene` 补丁走同一 config 字段与广播形状（单一事实源，无分歧路径）。
pub fn set_scene(app: &AppHandle, scene: config::SceneMode) -> Result<(), AppError> {
    let dir = config::app_config_dir(app)?;
    {
        let mut guard = config::write_global()?;
        guard.scene.active = scene;
    }
    let settings = config::read_global()?;
    config::save_to(&dir, &settings)?;
    let patch = crate::events::SettingsPatch {
        scene: Some(crate::events::ScenePatch {
            active: Some(scene),
        }),
        ..Default::default()
    };
    app.emit(crate::events::EVENT_SETTINGS_CHANGED, patch)
        .map_err(|err| AppError::Config(format!("settings://changed 广播失败: {err}")))?;
    log::info!("action_router: 活动场景已切换为 {scene:?}");
    Ok(())
}

// ---------------------------------------------------------------------------
// 单元测试：路由分发 / 挂起队列（入队/容量 1/取走/重启丢弃）/ 补配继续 / 压制门控判定
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_of_routes_five_actions_exactly() {
        assert_eq!(flow_of(Action::SelectionTranslate), Flow::Selection);
        assert_eq!(flow_of(Action::InputTranslate), Flow::Input);
        assert_eq!(
            flow_of(Action::OcrRecognize),
            Flow::Capture(OcrKind::Recognize)
        );
        assert_eq!(
            flow_of(Action::OcrTranslate),
            Flow::Capture(OcrKind::Translate)
        );
        assert_eq!(flow_of(Action::ToggleScene), Flow::ToggleScene);
    }

    #[test]
    fn hotkey_action_converts_to_action_preserving_semantics() {
        for (hotkey, expected) in [
            (HotkeyAction::SelectionTranslate, Action::SelectionTranslate),
            (HotkeyAction::InputTranslate, Action::InputTranslate),
            (HotkeyAction::OcrRecognize, Action::OcrRecognize),
            (HotkeyAction::OcrTranslate, Action::OcrTranslate),
            (HotkeyAction::ToggleScene, Action::ToggleScene),
        ] {
            assert_eq!(Action::from(hotkey), expected);
        }
    }

    #[test]
    fn pending_queue_capacity_one_last_wins_and_fully_volatile() {
        let queue = PendingQueue::new();
        assert!(queue.is_empty());
        queue.enqueue(PendingAction::new(PendingKind::Input));
        assert_eq!(queue.len(), 1);
        queue.enqueue(PendingAction::new(PendingKind::Selection { text: None }));
        // 容量 1：新挂起替换旧挂起（last-wins，EX-13 仅最新语义）
        assert_eq!(queue.len(), 1);
        assert_eq!(
            queue.peek(),
            Some(PendingAction::new(PendingKind::Selection { text: None }))
        );
        let taken = queue.take();
        assert_eq!(
            taken,
            Some(PendingAction::new(PendingKind::Selection { text: None }))
        );
        assert!(queue.is_empty(), "取走后队列为空（恢复/丢弃一次性消费）");
    }

    #[test]
    fn pending_queue_restart_discards_old_pending() {
        // 「重启丢弃」＝新进程新队列：fresh 实例天然为空（挂起队列为进程内存态，无持久化）。
        let fresh = PendingQueue::new();
        assert!(fresh.is_empty());
        assert_eq!(fresh.take(), None);
        // 旧队列即使有挂起，也与新进程无关。
        let old = PendingQueue::new();
        old.enqueue(PendingAction::new(PendingKind::Input));
        assert_eq!(old.len(), 1);
        let fresh_after_restart = PendingQueue::new();
        assert!(fresh_after_restart.is_empty());
    }

    #[test]
    fn decide_entry_covers_onboarding_guide_and_proceed() {
        // onboarding 可见：翻译类与截图类均挂起（引导期压制统一，5.7.3）
        assert_eq!(
            decide_entry(true, true, false),
            GateDecision::SuspendForOnboarding
        );
        assert_eq!(
            decide_entry(true, false, true),
            GateDecision::SuspendForOnboarding
        );
        // 无 Key：翻译类入口 → 引导；截图识别（needs_key=false）→ 放行（FR-6/7.3 OCR 照常完成）
        assert_eq!(
            decide_entry(false, true, false),
            GateDecision::SuspendForGuide
        );
        assert_eq!(decide_entry(false, false, false), GateDecision::Proceed);
        // 有 Key → 放行
        assert_eq!(decide_entry(false, true, true), GateDecision::Proceed);
    }

    #[test]
    fn decide_resume_continues_only_with_key() {
        assert_eq!(decide_resume(false, true), ResumeDecision::Nothing);
        assert_eq!(decide_resume(false, false), ResumeDecision::Nothing);
        assert_eq!(decide_resume(true, true), ResumeDecision::Continue);
        assert_eq!(decide_resume(true, false), ResumeDecision::Discard);
    }

    #[test]
    fn pending_of_submit_maps_sources_to_resumable_kinds() {
        assert_eq!(
            pending_of_submit(TranslateSource::Selection, "选文"),
            PendingKind::Selection {
                text: Some("选文".to_string())
            }
        );
        assert_eq!(
            pending_of_submit(TranslateSource::Input, "输入"),
            PendingKind::Input
        );
        assert_eq!(
            pending_of_submit(TranslateSource::Clipboard, "剪贴"),
            PendingKind::Clipboard("剪贴".to_string())
        );
        assert_eq!(
            pending_of_submit(TranslateSource::Ocr, "OCR"),
            PendingKind::OcrTranslate {
                text: Some("OCR".to_string()),
                session_id: None
            }
        );
    }

    #[test]
    fn ocr_context_register_read_unset_roundtrip() {
        // 独立复制注册逻辑（对全局 map 的写入经访问器；此处验证纯函数语义等价性）。
        let session = "s-t16-test";
        register_ocr_context(
            session,
            OcrContext {
                kind: OcrKind::Translate,
                lang: "auto".to_string(),
                text: None,
            },
        );
        let ctx = ocr_context_of(session).unwrap_or_else(|| panic!("应有上下文"));
        assert_eq!(ctx.kind, OcrKind::Translate);
        assert_eq!(ctx.lang, "auto");
        assert_eq!(ctx.text, None);
        update_ocr_text_and_lang(session, "zh-CN".to_string(), "识别文本".to_string());
        let ctx = ocr_context_of(session).unwrap_or_else(|| panic!("应有上下文"));
        assert_eq!(ctx.lang, "zh-CN");
        assert_eq!(ctx.text.as_deref(), Some("识别文本"));
        assert_eq!(ocr_text_of(session).as_deref(), Some("识别文本"));
        clear_ocr_context(session);
        assert_eq!(ocr_context_of(session), None, "会话替换/关闭后上下文释放");
        // 防御：从未登记过的会话 → None（非本模块发起的会话不产生延续动作）
        assert_eq!(ocr_context_of("s-unknown"), None);
    }
}
