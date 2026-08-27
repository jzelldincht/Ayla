//! hotkey：全局热键管理服务（T4 实现，spec 5.7.1 + 5.8 EX-7 + 7.2 性能打点）。
//!
//! 职责：
//! - 启动（`init`）从 config.hotkeys 读五项逐一注册（tauri-plugin-global-shortcut），每项收集
//!   `{function, combo, ok|err}`；有失败项聚合为**一则**系统通知列出功能名（EX-7），失败项计入
//!   降级态（`degraded`：划词不可用直至改键；其余四项托盘兜底，FR-14.3）。
//! - `rebind_hotkey`（6.1 命令，可被 invoke_handler 注册）：五项内部查重拒绝 → unregister 旧 →
//!   register 新 → 成功才写 config；register 失败回退注册旧键、不改配置、返回 AppError（EX-7，
//!   设置页标红 + 系统通知）。任何成功的注册变更都更新 `registered: HashMap<Function, Combo>`。
//! - 回调经 HotkeyAction 注入接口：T16 建 action_router 后经 `set_callback` 接线；未接线时仅打点
//!   不模拟动作（打点即显式指出未接线，非静默失败）。
//! - 性能打点：`log::debug!("hotkey_received: {action:?}")`（7.2 热键回调分发至 handler 预算 20ms）。
//! - 快捷键字符串与 tauri 2 插件约定一致，形如 `"Alt+Q"`（插件内部经 global_hotkey 解析）。
//!
//! 锁序：仅 `STATE`（本模块 Mutex）与 config 锁两把——`rebind_hotkey` 内先 STATE 后 config（写配置），
//! 无任何路径反向嵌套（`init` 读取 config 为瞬时克隆后即释放）。全局锁序声明仍是「先 config 后
//! sessions」，本锁与此无关但须保持单向；回调分发（`dispatch_action`）不在任何锁内执行。

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};
use tauri_plugin_notification::NotificationExt;

use crate::config;
use crate::error::AppError;
use crate::events::HotkeyFunction;

/// 五项可改热键功能（FR-14.2：剪贴板监听不设热键）。顺序即注册顺序（FR-14.1）。
const FUNCTIONS: [HotkeyFunction; 5] = [
    HotkeyFunction::Selection,
    HotkeyFunction::Input,
    HotkeyFunction::OcrRecognize,
    HotkeyFunction::OcrTranslate,
    HotkeyFunction::ToggleScene,
];

// ---------------------------------------------------------------------------
// 触发动作与回调注入接口（T16 由 action_router 接线）
// ---------------------------------------------------------------------------

/// 热键触发产生的统一动作（spec 5.4 五动作；名称与 action_router 的入口保持一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    SelectionTranslate,
    InputTranslate,
    OcrRecognize,
    OcrTranslate,
    ToggleScene,
}

/// 动作分发回调注入接口：T16 用 action_router 实现并 `set_callback` 注入；
/// 未注入时触发仅记 Debug 打点（不模拟任何动作，避免绕过 action_router 直发翻译）。
pub trait HotkeyCallback: Send + Sync + 'static {
    fn dispatch(&self, action: HotkeyAction);
}

static CALLBACK: OnceLock<Box<dyn HotkeyCallback>> = OnceLock::new();

/// 注入 action_router 回调（仅一次；重复注入拒绝，防多源接线）。
pub fn set_callback(callback: Box<dyn HotkeyCallback>) -> Result<(), AppError> {
    CALLBACK
        .set(callback)
        .map_err(|_| AppError::Config("hotkey: action_router 回调重复注入".to_string()))
}

fn dispatch_action(action: HotkeyAction) {
    match CALLBACK.get() {
        Some(callback) => callback.dispatch(action),
        None => {
            // 显式记录未接线：热键已触发但不产生动作（T16 接线前属于预期路径，非静默失败）。
            log::debug!("hotkey: 动作 {action:?} 已触发，action_router 回调未接线（T16 接入）");
        }
    }
}

// ---------------------------------------------------------------------------
// 注册结果与汇总（startup 聚合，EX-7）
// ---------------------------------------------------------------------------

/// 单项注册结果（启动聚合表；`ok=false` 时 `error` 为插件错误描述）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterResult {
    pub function: HotkeyFunction,
    pub combo: String,
    pub ok: bool,
    pub error: Option<String>,
}

/// 启动注册汇总：`failed` 为降级态功能清单（FR-14.3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterSummary {
    pub results: Vec<RegisterResult>,
    pub failed: Vec<HotkeyFunction>,
}

/// 从未注册结果表提取失败功能（纯逻辑，供测试）。
pub fn collect_failures(results: &[RegisterResult]) -> Vec<HotkeyFunction> {
    results
        .iter()
        .filter(|result| !result.ok)
        .map(|result| result.function)
        .collect()
}

// ---------------------------------------------------------------------------
// 服务状态：registered 表 + 降级态
// ---------------------------------------------------------------------------

/// 热键服务状态：注册表（function → combo）与降级态清单（启动注册失败项）。
#[derive(Debug)]
struct HotkeyState {
    registered: HashMap<HotkeyFunction, String>,
    degraded: Vec<HotkeyFunction>,
}

static STATE: OnceLock<Mutex<HotkeyState>> = OnceLock::new();

fn state_lock() -> Result<MutexGuard<'static, HotkeyState>, AppError> {
    let lock = STATE
        .get()
        .ok_or_else(|| AppError::Config("hotkey: 服务未初始化（先 init 启动注册）".to_string()))?;
    lock.lock()
        .map_err(|_| AppError::Config("hotkey: 热键状态锁中毒".to_string()))
}

/// 注册表快照（供托盘等镜像消费：spec 5.7.1「托盘据此渲染失效提示」为可选增强）。
pub fn registered_snapshot() -> Result<HashMap<HotkeyFunction, String>, AppError> {
    Ok(state_lock()?.registered.clone())
}

/// 降级态功能清单（启动注册失败；改键成功后逐项移除）。
pub fn degraded_functions() -> Result<Vec<HotkeyFunction>, AppError> {
    Ok(state_lock()?.degraded.clone())
}

/// `hotkey_registration_failures` 命令（T13 新增契约；design-spec 7.2 数据源）——
/// 设置页快捷键行尾常驻 warning 图标：返回启动注册失败（=降级态）功能清单，
/// 为空表示五项全部注册成功。服务未初始化（异常装配路径）如实返回错误，前端保守渲染。
#[tauri::command]
pub fn hotkey_registration_failures() -> Result<Vec<HotkeyFunction>, AppError> {
    degraded_functions()
}

/// 退出清理（T15 托盘「退出」路径，spec 5.7.9 清理序第 1 步）：注销 `registered` 表内全部热键。
/// 幂等；失败仅记日志（进程即将退出，无恢复义务；不持有本模块锁调用插件）。
pub fn shutdown(app: &AppHandle) {
    let combos: Vec<String> = match state_lock() {
        Ok(guard) => guard.registered.values().cloned().collect(),
        Err(err) => {
            log::warn!("hotkey: 退出清理读取注册表失败（跳过注销）: {err}");
            return;
        }
    };
    for combo in &combos {
        if let Err(err) = app.global_shortcut().unregister(combo.as_str()) {
            log::warn!("hotkey: 退出注销失败 {combo}: {err}");
        }
    }
    log::info!("hotkey: 退出清理完成（已注销 {} 项）", combos.len());
}

// ---------------------------------------------------------------------------
// 启动注册（setup 调用；聚合逻辑可独立测试）
// ---------------------------------------------------------------------------

/// 启动逐一注册五项：收集聚合结果；有失败则**一则**系统通知列出功能名（EX-7），
/// 失败项计入降级态（FR-14.3）。重复调用返回错误，防双源注册竞态。
pub fn init(app: &AppHandle) -> Result<RegisterSummary, AppError> {
    if STATE.get().is_some() {
        return Err(AppError::Config(
            "hotkey: init 已执行（启动注册仅一次）".to_string(),
        ));
    }
    STATE
        .set(Mutex::new(HotkeyState {
            registered: HashMap::new(),
            degraded: Vec::new(),
        }))
        .map_err(|_| AppError::Config("hotkey: 全局状态初始化失败".to_string()))?;

    let settings = config::read_global()?;
    let mut guard = state_lock()?;
    let mut results = Vec::with_capacity(FUNCTIONS.len());
    let mut failed = Vec::new();

    for function in FUNCTIONS {
        let combo = combo_of(&settings, function);
        match app
            .global_shortcut()
            .on_shortcut(combo.as_str(), make_handler(action_of(function)))
        {
            Ok(()) => {
                log::info!("hotkey: 注册成功 {function:?} = {combo}");
                guard.registered.insert(function, combo.clone());
                results.push(RegisterResult {
                    function,
                    combo,
                    ok: true,
                    error: None,
                });
            }
            Err(err) => {
                log::error!("hotkey: 注册失败 {function:?} = {combo}：{err}");
                failed.push(function);
                results.push(RegisterResult {
                    function,
                    combo,
                    ok: false,
                    error: Some(err.to_string()),
                });
            }
        }
    }
    guard.degraded = failed.clone();
    let summary = RegisterSummary { results, failed };
    if !summary.failed.is_empty() {
        notify_registration_failures(app, &summary.failed);
    }
    log::info!(
        "hotkey: 启动注册完成 成功 {}/{}，失败 {}（见 RegisterSummary）",
        FUNCTIONS.len() - summary.failed.len(),
        FUNCTIONS.len(),
        summary.failed.len()
    );
    Ok(summary)
}

/// EX-7 聚合通知（一则）：列出全部注册失败的功能名。
fn notify_registration_failures(app: &AppHandle, failed: &[HotkeyFunction]) {
    let names = failed
        .iter()
        .map(|function| function_display_name(*function))
        .collect::<Vec<_>>()
        .join("、");
    let mut body = format!(
        "以下功能的快捷键注册失败（可能已被系统或其他程序占用），可在设置中重新指定后立即恢复：{names}。"
    );
    if failed.contains(&HotkeyFunction::Selection) {
        body.push_str("划词翻译需改键后方可触发；其余功能仍可经托盘菜单使用（FR-14.3）。");
    }
    notify_hotkey_error(app, body);
}

fn notify_hotkey_error(app: &AppHandle, body: String) {
    if let Err(err) = app.notification().builder().title("Alya").body(body).show() {
        log::error!("hotkey: 系统通知发送失败（EX-7 呈现层）：{err}");
    }
}

// ---------------------------------------------------------------------------
// rebind_hotkey 命令（spec 5.7.1 改键流程；6.1 契约）
// ---------------------------------------------------------------------------

/// rebind 流程的纯决策（供测试；命令内实际按此执行）：
/// - 新旧键相同 → Noop（幂等成功，无任何注册/配置变更）；
/// - 新键注册成功 → Commit（写 config + 更新 registered 表）；
/// - 新键注册失败 → Revert（回退注册旧键、不改配置，返回 EX-7 错误）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebindDecision {
    Noop,
    Commit,
    Revert,
}

pub fn rebind_decision(old_combo: &str, new_combo: &str, register_new_ok: bool) -> RebindDecision {
    if old_combo == new_combo {
        RebindDecision::Noop
    } else if register_new_ok {
        RebindDecision::Commit
    } else {
        RebindDecision::Revert
    }
}

/// 内部五项查重（纯逻辑）：其余四项已占用 `new_combo` 时返回占用方功能（EX-7 拒绝）。
pub fn dup_check(
    registered: &HashMap<HotkeyFunction, String>,
    function: HotkeyFunction,
    new_combo: &str,
) -> Option<HotkeyFunction> {
    registered
        .iter()
        .find(|(owner, combo)| **owner != function && combo.as_str() == new_combo)
        .map(|(owner, _)| *owner)
}

/// `rebind_hotkey {function, combo}`（6.1）：内部查重 → 解旧 → 注新 → 成功才写 config；
/// 注新失败回退注册旧键且不改配置，返回 EX-7 `AppError`（设置页标红 + 系统通知）。
#[tauri::command]
pub fn rebind_hotkey(
    app: AppHandle,
    function: HotkeyFunction,
    combo: String,
) -> Result<(), AppError> {
    if combo.trim().is_empty() {
        return Err(conflict_error(
            function,
            &combo,
            "快捷键不能为空（至少一个修饰键 + 主键）",
        ));
    }

    let mut guard = state_lock()?;

    // 1) 与其余四项查重（内部冲突拒绝，EX-7）
    if let Some(owner) = dup_check(&guard.registered, function, &combo) {
        let err = conflict_error(
            function,
            &combo,
            &format!(
                "已被「{}」占用，请先解除或另选组合",
                function_display_name(owner)
            ),
        );
        notify_hotkey_error(&app, err.to_string());
        return Err(err);
    }

    // 2) 旧键（registered 表同步维护；未注册态不允许改键）
    let old_combo = match guard.registered.get(&function) {
        Some(old) => old.clone(),
        None => {
            return Err(AppError::Config(format!(
                "hotkey: {function:?} 未处于注册态（启动注册失败），请先改键恢复"
            )))
        }
    };

    // 3) 解注旧键（失败则中止，状态不变）
    app.global_shortcut()
        .unregister(old_combo.as_str())
        .map_err(|err| {
            AppError::HotkeyConflict(format!(
                "解注旧快捷键 {old_combo} 失败：{err}（状态未变更）"
            ))
        })?;

    // 4) 注新键并按纯决策分支（Revert＝回退注册旧键、不改配置；Commit＝成功才写 config）
    let register_new_ok = app
        .global_shortcut()
        .on_shortcut(combo.as_str(), make_handler(action_of(function)))
        .is_ok();

    match rebind_decision(&old_combo, &combo, register_new_ok) {
        RebindDecision::Noop => {
            // 同键改键（old == combo）：已按 解旧→注新 幂等完成（等价 Noop），不改配置。
            guard.registered.insert(function, combo);
            Ok(())
        }
        RebindDecision::Revert => {
            if let Err(rollback_err) = app
                .global_shortcut()
                .on_shortcut(old_combo.as_str(), make_handler(action_of(function)))
            {
                // 回退也失败：进降级态（该功能直至改键都不可用），仍返回原错误。
                log::error!(
                    "hotkey: 回退注册旧键 {old_combo} 失败：{rollback_err}；{function:?} 进入降级态"
                );
                guard.degraded.retain(|f| *f != function);
                guard.degraded.push(function);
            }
            let err = conflict_error(
                function,
                &combo,
                "注册失败（可能已被系统或其他程序占用），已回退为原快捷键",
            );
            notify_hotkey_error(&app, err.to_string());
            Err(err)
        }
        RebindDecision::Commit => {
            // 5) 成功才写 config（INV-8：config 为事实源）
            {
                let mut settings = config::write_global()?;
                set_combo(&mut settings, function, &combo);
            }
            let settings_guard = config::read_global()?;
            config::save_to(&config::app_config_dir(&app)?, &settings_guard)?;
            // 6) 更新 registered 表 + 移出降级态（任何成功的注册变更都更新表，spec 5.7.1）
            guard.registered.insert(function, combo.clone());
            guard.degraded.retain(|f| *f != function);
            log::info!("hotkey: 改键成功 {function:?} = {combo}（原 {old_combo}）");
            Ok(())
        }
    }
}

fn conflict_error(function: HotkeyFunction, combo: &str, reason: &str) -> AppError {
    AppError::HotkeyConflict(format!(
        "快捷键「{combo}」绑定失败：{}（{}）",
        function_display_name(function),
        reason
    ))
}

// ---------------------------------------------------------------------------
// 回调构造与动作映射
// ---------------------------------------------------------------------------

/// 功能 → 动作（action_router 五入口同名）。
pub fn action_of(function: HotkeyFunction) -> HotkeyAction {
    match function {
        HotkeyFunction::Selection => HotkeyAction::SelectionTranslate,
        HotkeyFunction::Input => HotkeyAction::InputTranslate,
        HotkeyFunction::OcrRecognize => HotkeyAction::OcrRecognize,
        HotkeyFunction::OcrTranslate => HotkeyAction::OcrTranslate,
        HotkeyFunction::ToggleScene => HotkeyAction::ToggleScene,
    }
}

/// 功能呈现名（简体中文；通知列名与设置页共用一份文案）。
pub fn function_display_name(function: HotkeyFunction) -> &'static str {
    match function {
        HotkeyFunction::Selection => "划词翻译",
        HotkeyFunction::Input => "输入翻译",
        HotkeyFunction::OcrRecognize => "截图识别",
        HotkeyFunction::OcrTranslate => "截图翻译",
        HotkeyFunction::ToggleScene => "切换场景",
    }
}

/// 构造注册回调：`Pressed` 才分发（RegisterHotKey 对按下/释放各回调一次，仅按一次动作）+
/// 7.2 性能打点（热键回调分发起点）。
fn make_handler(
    action: HotkeyAction,
) -> impl Fn(&AppHandle, &Shortcut, ShortcutEvent) + Send + Sync + 'static {
    move |_app, _shortcut, event| {
        if event.state != ShortcutState::Pressed {
            return;
        }
        log::debug!("hotkey_received: {action:?}");
        dispatch_action(action);
    }
}

// ---------------------------------------------------------------------------
// config.hotkeys 读写辅助（对 spec 6.3 五字段的唯一切换点）
// ---------------------------------------------------------------------------

fn combo_of(settings: &config::Settings, function: HotkeyFunction) -> String {
    let hotkeys = &settings.hotkeys;
    match function {
        HotkeyFunction::Selection => hotkeys.selection.clone(),
        HotkeyFunction::Input => hotkeys.input.clone(),
        HotkeyFunction::OcrRecognize => hotkeys.ocr_recognize.clone(),
        HotkeyFunction::OcrTranslate => hotkeys.ocr_translate.clone(),
        HotkeyFunction::ToggleScene => hotkeys.toggle_scene.clone(),
    }
}

fn set_combo(settings: &mut config::Settings, function: HotkeyFunction, combo: &str) {
    let hotkeys = &mut settings.hotkeys;
    match function {
        HotkeyFunction::Selection => hotkeys.selection = combo.to_string(),
        HotkeyFunction::Input => hotkeys.input = combo.to_string(),
        HotkeyFunction::OcrRecognize => hotkeys.ocr_recognize = combo.to_string(),
        HotkeyFunction::OcrTranslate => hotkeys.ocr_translate = combo.to_string(),
        HotkeyFunction::ToggleScene => hotkeys.toggle_scene = combo.to_string(),
    }
}

// ---------------------------------------------------------------------------
// 单元测试：内部查重 / 回退决策 / 结果聚合 / 名称与动作映射（纯逻辑，不触插件）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(pairs: &[(HotkeyFunction, &str)]) -> HashMap<HotkeyFunction, String> {
        pairs
            .iter()
            .map(|(function, combo)| (*function, combo.to_string()))
            .collect()
    }

    #[test]
    fn dup_check_rejects_occupied_combo_but_allows_self() {
        let registered = entries(&[
            (HotkeyFunction::Selection, "Alt+Q"),
            (HotkeyFunction::Input, "Alt+E"),
            (HotkeyFunction::OcrRecognize, "Alt+S"),
            (HotkeyFunction::OcrTranslate, "Alt+D"),
            (HotkeyFunction::ToggleScene, "Alt+W"),
        ]);
        // 与「输入翻译」冲突 → 返回占用方（EX-7 指明功能名）
        assert_eq!(
            dup_check(&registered, HotkeyFunction::Selection, "Alt+E"),
            Some(HotkeyFunction::Input)
        );
        // 与自己重复不算冲突（改键为同值属幂等 Noop，不触发注册）
        assert_eq!(
            dup_check(&registered, HotkeyFunction::Selection, "Alt+Q"),
            None
        );
        // 空闲组合允许
        assert_eq!(
            dup_check(&registered, HotkeyFunction::Selection, "Ctrl+Shift+Y"),
            None
        );
    }

    #[test]
    fn rebind_decision_covers_noop_commit_revert() {
        // 同键幂等：不注册不改配置
        assert_eq!(
            rebind_decision("Alt+Q", "Alt+Q", false),
            RebindDecision::Noop,
            "同键应 Noop，即使注册失败也无需回退"
        );
        // 新键注册成功 → 提交（写 config + 更新表）
        assert_eq!(
            rebind_decision("Alt+Q", "Ctrl+1", true),
            RebindDecision::Commit
        );
        // 新键注册失败 → 回退（重注旧键，不改 config）
        assert_eq!(
            rebind_decision("Alt+Q", "Ctrl+1", false),
            RebindDecision::Revert
        );
    }

    #[test]
    fn collect_failures_aggregates_ok_and_err_results() {
        let results = vec![
            RegisterResult {
                function: HotkeyFunction::Selection,
                combo: "Alt+Q".into(),
                ok: true,
                error: None,
            },
            RegisterResult {
                function: HotkeyFunction::Input,
                combo: "Alt+E".into(),
                ok: false,
                error: Some("hotkey is already registered".into()),
            },
            RegisterResult {
                function: HotkeyFunction::OcrRecognize,
                combo: "Alt+S".into(),
                ok: false,
                error: Some("invalid combo".into()),
            },
        ];
        assert_eq!(
            collect_failures(&results),
            vec![HotkeyFunction::Input, HotkeyFunction::OcrRecognize],
            "聚合仅收集失败项（EX-7 通知列名）"
        );
        // 全成功 → 空降级清单（无通知）
        let ok_only = vec![RegisterResult {
            function: HotkeyFunction::Selection,
            combo: "Alt+Q".into(),
            ok: true,
            error: None,
        }];
        assert!(collect_failures(&ok_only).is_empty());
    }

    #[test]
    fn display_names_and_actions_cover_five_functions() {
        assert_eq!(function_display_name(HotkeyFunction::Selection), "划词翻译");
        assert_eq!(function_display_name(HotkeyFunction::Input), "输入翻译");
        assert_eq!(
            function_display_name(HotkeyFunction::OcrRecognize),
            "截图识别"
        );
        assert_eq!(
            function_display_name(HotkeyFunction::OcrTranslate),
            "截图翻译"
        );
        assert_eq!(
            function_display_name(HotkeyFunction::ToggleScene),
            "切换场景"
        );
        assert_eq!(
            action_of(HotkeyFunction::Selection),
            HotkeyAction::SelectionTranslate
        );
        assert_eq!(
            action_of(HotkeyFunction::ToggleScene),
            HotkeyAction::ToggleScene
        );
    }

    #[test]
    fn combo_read_write_updates_only_target_function() {
        let mut settings = config::Settings::default();
        assert_eq!(combo_of(&settings, HotkeyFunction::Selection), "Alt+Q");
        set_combo(&mut settings, HotkeyFunction::Selection, "Ctrl+Shift+Y");
        assert_eq!(
            combo_of(&settings, HotkeyFunction::Selection),
            "Ctrl+Shift+Y"
        );
        // 其余四项不受影响（FR-14.1 出厂值保持）
        assert_eq!(combo_of(&settings, HotkeyFunction::Input), "Alt+E");
        assert_eq!(combo_of(&settings, HotkeyFunction::OcrRecognize), "Alt+S");
        assert_eq!(combo_of(&settings, HotkeyFunction::OcrTranslate), "Alt+D");
        assert_eq!(combo_of(&settings, HotkeyFunction::ToggleScene), "Alt+W");
    }

    #[test]
    fn conflict_error_carries_ex7_code_and_function_name() {
        let err = conflict_error(HotkeyFunction::Selection, "Ctrl+D", "已被「截图识别」占用");
        assert_eq!(err.ex_code(), "EX-7");
        let msg = err.to_string();
        assert!(msg.contains("划词翻译"), "EX-7 通知须指明功能名");
        assert!(msg.contains("Ctrl+D"));
    }
}
