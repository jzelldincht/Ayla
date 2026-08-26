//! clipboard_monitor：剪贴板监听状态机（T5 实现，spec 5.6.3 + 5.8 EX-8 + D2/D3 + FR-5.7）。
//!
//! 状态机（一把 Mutex 保护全部字段；tick 周期 500ms 只查 `GetClipboardSequenceNumber()`，序号
//! 变化才读内容——无消息窗口、CPU 近零）：
//!
//! ```text
//! 状态：Off / On{ baseline:(seq, text), suppressing:bool }
//! enable：读当前 (seq,text) 作为 baseline（开启瞬间不触发存量）→ On [FR-5.2/5.3]
//! disable：→ Off（立即停止响应）                                        [FR-5.6]
//! 每个 tick（仅 On）：
//!   cur_seq == baseline.seq → 无事
//!   读文本：None（非文本）→ baseline.seq = cur_seq；静默 [EX-8]
//!          Some(t)：suppressing → baseline = (cur_seq, t)；静默（吞取词写入/恢复）[FR-5.7]
//!                   t == baseline.text → baseline.seq = cur_seq；静默（D2 内容去重）[FR-5.5]
//!                   chars(t) > MAX_AUTO_CHARS → baseline 更新；TooLong（D3 通知跳过）
//!                   其余 → baseline = (cur_seq, t)；Trigger（触发翻译）[FR-5.2]
//! ```
//!
//! FR-5.7 抑制时序（T6 取词模块调用约定，同一把 Mutex 保证）：
//! - `begin_suppression` 在**备份之前**调用（置位先于任何写入）；
//! - `end_suppression` 在恢复/放弃恢复并「刷新基线」之后调用（先刷新基线后清位）；
//! - 两次调用之间不持有本锁（抑制语义由“置位先于每次写入、清位晚于基线刷新”的顺序保证，
//!   期间可能发生的 tick 只能看到 suppressing=true 而吞掉内容）；
//! - 恢复竞态：T6 恢复前校验失败放弃恢复时（期间用户自己复制了新内容），end_suppression
//!   仍会刷新基线——监听基线以用户新内容为准（内容本身已被抑制窗消费，不重复触发，D2 口径），
//!   取词链路以读到的用户新内容触发翻译，监听恢复正常工作。
//!
//! 触发翻译的出口经 `ClipboardTranslateTrigger` 回调注入（同 T4 模式）：T16 建 action_router 后
//! `set_translate_trigger` 接线；未接线时仅记 Debug 打点，不模拟动作。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

use crate::config;
use crate::error::AppError;
use crate::events::{SettingsPatch, EVENT_SETTINGS_CHANGED};

/// 轮询间隔（spec 5.6.3：500ms，tick 只查序号不读内容）。
pub const POLL_INTERVAL_MS: u64 = 500;
/// 自动监听链路原文长度上限（D3：50,000 字符，常量可调；仅作用于自动链路，手动输入不限）。
pub const MAX_AUTO_CHARS: usize = 50_000;

// ---------------------------------------------------------------------------
// 剪贴板读写抽象（生产＝Win32 序号 + arboard；测试＝假实现注入）
// ---------------------------------------------------------------------------

/// 剪贴板后端抽象：序号查询 + 文本读取（tick/测试均经此注入）。
pub trait ClipboardBackend {
    /// 当前剪贴板序列号（生产实现＝`GetClipboardSequenceNumber`；u32 系统计数，失败返回 0）。
    fn sequence_number(&self) -> u32;
    /// 读剪贴板文本：`Ok(None)` ＝ 非文本（图片/文件/空等，FR-5.4 静默忽略）；
    /// `Err` ＝ 真实读取异常（如被其他进程占用）——调用方记日志并跳过本 tick，不伪装成功。
    fn read_text(&mut self) -> Result<Option<String>, String>;
}

/// 生产后端：user32 序号 + arboard 文本读取。
pub struct ArboardBackend {
    clipboard: arboard::Clipboard,
}

impl ArboardBackend {
    pub fn new() -> Result<Self, String> {
        arboard::Clipboard::new()
            .map(|clipboard| Self { clipboard })
            .map_err(|err| err.to_string())
    }
}

impl ClipboardBackend for ArboardBackend {
    fn sequence_number(&self) -> u32 {
        // SAFETY: GetClipboardSequenceNumber 无引用/指针参数，仅返回 u32 系统序号；
        // 不产生借用安全风险（user32 剪贴板序号是系统级单调计数）。
        unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() }
    }

    fn read_text(&mut self) -> Result<Option<String>, String> {
        match self.clipboard.get_text() {
            Ok(text) => Ok(Some(text)),
            // 非文本（内容为图片/文件等或剪贴板为空）：EX-8 静默忽略语义。
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(err) => Err(err.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// 状态机核心
// ---------------------------------------------------------------------------

/// 一次 tick 的决策结果（触发翻译/超限通知由运行时（监听循环）执行呈现动作）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickOutcome {
    /// 无事发生（含非文本、内容去重、抑制窗吞写等静默路径）。
    Nothing,
    /// 新文本 → 应触发翻译（T16 经回调注入接线到 action_router）。
    Trigger { text: String },
    /// 超过 `MAX_AUTO_CHARS`（D3）→ 应跳过翻译并系统通知。
    TooLong { chars: usize },
}

/// On 态数据（spec 5.6.3：baseline + suppressing，同一 Mutex 内）。
#[derive(Debug)]
struct OnState {
    baseline_seq: u32,
    baseline_text: Option<String>,
    suppressing: bool,
}

#[derive(Debug, Default)]
enum MonitorState {
    #[default]
    Off,
    On(OnState),
}

#[derive(Debug, Default)]
struct MonitorInner {
    state: MonitorState,
}

/// 剪贴板监听状态机（进程内单份；`ClipboardMonitor::default()` 供测试独立建实例）。
#[derive(Debug, Default)]
pub struct ClipboardMonitor {
    inner: Mutex<MonitorInner>,
    /// 监听循环代次：enable 自增启动、disable 自增令旧循环退出（立即失效，FR-5.6）。
    loop_generation: AtomicU64,
}

static INSTANCE: OnceLock<ClipboardMonitor> = OnceLock::new();
static TRIGGER: OnceLock<Box<dyn ClipboardTranslateTrigger>> = OnceLock::new();

/// 全局监听状态机（进程内单份；与 config 锁无嵌套——本模块方法内的临界区均为短临界区，禁止在
/// 持有本锁期间调用任何可能重入本模块或长时间阻塞的接口）。
pub fn global() -> &'static ClipboardMonitor {
    INSTANCE.get_or_init(ClipboardMonitor::default)
}

/// 翻译触发回调注入接口（T16 接线 action_router；重复注入拒绝）。
pub trait ClipboardTranslateTrigger: Send + Sync + 'static {
    fn on_clipboard_text(&self, text: String);
}

pub fn set_translate_trigger(trigger: Box<dyn ClipboardTranslateTrigger>) -> Result<(), AppError> {
    TRIGGER
        .set(trigger)
        .map_err(|_| AppError::Config("clipboard_monitor: 翻译回调重复注入".to_string()))
}

/// 触发出口：未接线时显式打点（INV-7：仅记字符数，绝不记原文内容）。
fn dispatch_translate(text: String) {
    match TRIGGER.get() {
        Some(trigger) => trigger.on_clipboard_text(text),
        None => log::debug!(
            "clipboard_monitor: 检测到新文本（{} 字符）但翻译回调未接线（T16 接入 action_router）",
            text.chars().count()
        ),
    }
}

impl ClipboardMonitor {
    /// 开启监听：读当前 (seq, text) 为 baseline（存量内容不触发，FR-5.2/5.3）。
    pub fn enable(&self, backend: &mut dyn ClipboardBackend) -> Result<(), AppError> {
        let seq = backend.sequence_number();
        let text = backend.read_text().map_err(|err| {
            AppError::Config(format!("clipboard_monitor: 开启时读取剪贴板失败: {err}"))
        })?;
        let mut inner = self.lock_state()?;
        inner.state = MonitorState::On(OnState {
            baseline_seq: seq,
            baseline_text: text,
            suppressing: false,
        });
        log::info!("clipboard_monitor: 监听开启（baseline seq={seq}）");
        Ok(())
    }

    /// 关闭监听：立即失效（FR-5.6）+ 令监听循环退出。
    pub fn disable(&self) -> Result<(), AppError> {
        let mut inner = self.lock_state()?;
        inner.state = MonitorState::Off;
        self.loop_generation.fetch_add(1, Ordering::SeqCst);
        log::info!("clipboard_monitor: 监听关闭（立即失效）");
        Ok(())
    }

    /// 是否处于 On 态（缺省 Off；锁中毒按 Off 处理并记错误日志，防止监听自嗨）。
    pub fn is_on(&self) -> bool {
        match self.lock_state() {
            Ok(inner) => matches!(inner.state, MonitorState::On(_)),
            Err(err) => {
                log::error!("clipboard_monitor: is_on 查询失败（视同 Off）: {err}");
                false
            }
        }
    }

    /// 当前监听循环代次（disable/新 enable 均会使旧循环退出）。
    pub fn generation(&self) -> u64 {
        self.loop_generation.load(Ordering::SeqCst)
    }

    /// 处理一个 tick（500ms；仅 On 态有行为；Off 态恒 Nothing）。
    pub fn tick(&self, backend: &mut dyn ClipboardBackend) -> Result<TickOutcome, AppError> {
        let mut inner = self.lock_state()?;
        let on = match &mut inner.state {
            MonitorState::Off => return Ok(TickOutcome::Nothing),
            MonitorState::On(on) => on,
        };

        let cur_seq = backend.sequence_number();
        if cur_seq == on.baseline_seq {
            return Ok(TickOutcome::Nothing);
        }

        let text = match backend.read_text() {
            Ok(text) => text,
            Err(err) => {
                // 剪贴板被占用等瞬时异常：记日志、跳过本 tick（基线不动，下轮重试）；
                // 监听不受扰（EX-8“不打断监听”口径扩展至异常读取）。
                log::warn!("clipboard_monitor: 读取剪贴板失败，跳过本 tick: {err}");
                return Ok(TickOutcome::Nothing);
            }
        };

        match text {
            // 非文本（EX-8）：只升序号，静默。
            None => {
                on.baseline_seq = cur_seq;
                Ok(TickOutcome::Nothing)
            }
            Some(t) => {
                if on.suppressing {
                    // 抑制窗（FR-5.7）：取词写入与恢复均视为自身操作，吞入基线，静默。
                    on.baseline_seq = cur_seq;
                    on.baseline_text = Some(t);
                    return Ok(TickOutcome::Nothing);
                }
                if on.baseline_text.as_deref() == Some(t.as_str()) {
                    // 内容去重（D2）：重复复制同文本只升序号，不重复触发（FR-5.5）。
                    on.baseline_seq = cur_seq;
                    return Ok(TickOutcome::Nothing);
                }
                let chars = t.chars().count();
                on.baseline_seq = cur_seq;
                on.baseline_text = Some(t.clone());
                if chars > MAX_AUTO_CHARS {
                    log::warn!(
                        "clipboard_monitor: 剪贴板文本 {chars} 字符超过上限 {MAX_AUTO_CHARS}，跳过自动翻译（D3）"
                    );
                    return Ok(TickOutcome::TooLong { chars });
                }
                log::debug!(
                    "clipboard_monitor: 检测到新文本（{chars} 字符），触发翻译（出口经回调注入，T16 接 action_router）"
                );
                Ok(TickOutcome::Trigger { text: t })
            }
        }
    }

    /// 取词抑制：备份前置位（T6 在**备份之前**调用；Off 态为无操作）。
    /// 置位先于取词写剪贴板，保证写入的序号变化必然落在抑制窗内（FR-5.7）。
    pub fn begin_suppression(&self) -> Result<(), AppError> {
        let mut inner = self.lock_state()?;
        if let MonitorState::On(on) = &mut inner.state {
            on.suppressing = true;
            log::debug!("clipboard_monitor: 抑制窗开启（取词进行中）");
        }
        Ok(())
    }

    /// 取词抑制结束：刷新基线后清位（T6 在恢复/放弃恢复完成后调用；顺序：先刷新后清位）。
    /// 读剪贴板失败时仍清位（防止抑制位卡死吞掉内容），基线维持旧值并记错误日志。
    pub fn end_suppression(&self, backend: &mut dyn ClipboardBackend) -> Result<(), AppError> {
        let mut inner = self.lock_state()?;
        let on = match &mut inner.state {
            MonitorState::Off => return Ok(()),
            MonitorState::On(on) => on,
        };
        let cur_seq = backend.sequence_number();
        match backend.read_text() {
            Ok(text) => {
                on.baseline_seq = cur_seq;
                on.baseline_text = text;
                log::debug!("clipboard_monitor: 抑制窗结束，基线已刷新（seq={cur_seq}），清位");
            }
            Err(err) => {
                log::error!(
                    "clipboard_monitor: 抑制窗结束读剪贴板失败（基线未刷新且清位，防止抑制卡死）: {err}"
                );
            }
        }
        on.suppressing = false;
        Ok(())
    }

    fn lock_state(&self) -> Result<MutexGuard<'_, MonitorInner>, AppError> {
        self.inner
            .lock()
            .map_err(|_| AppError::Config("clipboard_monitor: 状态锁中毒".to_string()))
    }
}

// ---------------------------------------------------------------------------
// 监听循环（tauri async_runtime spawn；500ms；On 态且代次匹配时持续运行）
// ---------------------------------------------------------------------------

/// 启动监听循环（禁止从持有本模块状态锁的临界区内调用）。
pub fn spawn_loop(app: &AppHandle) -> Result<(), AppError> {
    let monitor = global();
    let generation = monitor.loop_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut backend = match ArboardBackend::new() {
            Ok(backend) => backend,
            Err(err) => {
                log::error!("clipboard_monitor: 剪贴板初始化失败，监听循环退出: {err}");
                return;
            }
        };
        loop {
            if !monitor.is_on() || monitor.generation() != generation {
                break;
            }
            match monitor.tick(&mut backend) {
                Ok(TickOutcome::Nothing) => {}
                Ok(TickOutcome::Trigger { text }) => dispatch_translate(text),
                Ok(TickOutcome::TooLong { chars }) => notify_too_long(&app_handle, chars),
                Err(err) => log::warn!("clipboard_monitor: tick 失败: {err}"),
            }
            tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
        }
        log::debug!("clipboard_monitor: 监听循环退出（关闭或新代次接管）");
    });
    Ok(())
}

/// D3 系统通知：内容过长已跳过（自动监听链路）。
fn notify_too_long(app: &AppHandle, chars: usize) {
    let body =
        format!("剪贴板内容过长（{chars} 字符，超过 {MAX_AUTO_CHARS} 上限），已跳过自动翻译。");
    if let Err(err) = app.notification().builder().title("Alya").body(body).show() {
        log::error!("clipboard_monitor: 超限通知发送失败（D3 呈现层）: {err}");
    }
}

// ---------------------------------------------------------------------------
// set_clipboard_monitor 命令（6.1：三方开关统一入口，INV-8）
// ---------------------------------------------------------------------------

/// `set_clipboard_monitor {enabled}`：写 config + `settings://changed` 广播三方同步（INV-8），
/// 监听循环在 tauri async_runtime spawn；enable 失败回滚 config（内存+落盘），保持事实源一致。
#[tauri::command]
pub fn set_clipboard_monitor(app: AppHandle, enabled: bool) -> Result<(), AppError> {
    let dir = config::app_config_dir(&app)?;

    // 1) 写 config（INV-8：config 为单一事实源；旧值留作失败回滚）
    let old = {
        let mut guard = config::write_global()?;
        let old = guard.clipboard_monitor;
        guard.clipboard_monitor = enabled;
        old
    };

    // 2) 状态机切换（enable 可能失败：开启时读剪贴板异常 → 回滚 config）
    if enabled {
        match global().enable(
            &mut ArboardBackend::new()
                .map_err(|err| AppError::Config(format!("clipboard_monitor: 初始化失败: {err}")))?,
        ) {
            Ok(()) => {}
            Err(err) => {
                let rollback_result = rollback_monitor_flag(&dir, old);
                log::error!(
                    "clipboard_monitor: 开启失败，回滚配置: {err}（回滚结果: {rollback_result:?}）"
                );
                return Err(err);
            }
        }
    } else {
        global().disable()?;
    }

    // 3) 落盘（失败则状态机回滚，避免内存/磁盘分叉）
    let settings_guard = config::read_global()?;
    if let Err(err) = config::save_to(&dir, &settings_guard) {
        log::error!("clipboard_monitor: config 落盘失败，回滚状态机: {err}");
        let _ = global().disable();
        let _ = rollback_monitor_flag(&dir, old);
        return Err(err);
    }

    // 4) 监听循环（仅开启时 spawn；enable/disable 的代次管理保证关闭立即生效）
    if enabled {
        spawn_loop(&app)?;
    }

    // 5) 三方同步广播（INV-8；补丁仅带本字段——事件的 apiKey 剥离纪律见 events.rs）。
    let patch = SettingsPatch {
        clipboard_monitor: Some(enabled),
        ..Default::default()
    };
    if let Err(err) = app.emit(EVENT_SETTINGS_CHANGED, patch) {
        // 广播失败：三处 UI 镜像滞后至下次 get_settings（INV-8 收敛口径），命令本身已完成。
        log::error!("clipboard_monitor: settings://changed 广播失败: {err}");
    }
    Ok(())
}

/// 回滚 config 内存标志 + 落盘（仅用于命令失败路径，保证事实源与监听状态一致）。
fn rollback_monitor_flag(dir: &std::path::Path, old: bool) -> Result<(), AppError> {
    {
        let mut guard = config::write_global()?;
        guard.clipboard_monitor = old;
    }
    let settings_guard = config::read_global()?;
    config::save_to(dir, &settings_guard)
}

/// 启动恢复监听（FR-5.3：重启保持上次状态）。失败仅记录不阻断启动——
/// 监听是可选能力，用户可在设置页重新开关（命令入口自愈）；日志不含任何剪贴板内容（INV-7）。
pub fn start_if_enabled(app: &AppHandle) {
    let enabled = match config::read_global() {
        Ok(guard) => guard.clipboard_monitor,
        Err(err) => {
            log::error!("clipboard_monitor: 启动读取配置失败: {err}");
            return;
        }
    };
    if !enabled {
        return;
    }
    let result = (|| -> Result<(), AppError> {
        let mut backend =
            ArboardBackend::new().map_err(|err| AppError::Config(format!("初始化失败: {err}")))?;
        global().enable(&mut backend)?;
        spawn_loop(app)?;
        Ok(())
    })();
    if let Err(err) = result {
        log::error!("clipboard_monitor: 启动恢复监听失败（可在设置页重新开关自愈）: {err}");
    }
}

// ---------------------------------------------------------------------------
// 单元测试：全转移表（新文本/重复/非文本/抑制窗/恢复竞态/超限/关闭；trait 注入假实现）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    fn ok_of<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(err) => panic!("期望成功，实际错误: {err}"),
        }
    }

    /// 假剪贴板：seq 单调递增，单元格内容按 seq 存储；`fail_reads` 注入读取失败。
    #[derive(Default)]
    struct FakeBackend {
        seq: u32,
        cells: HashMap<u32, Option<String>>,
        fail_reads: bool,
    }

    impl FakeBackend {
        /// 为当前 seq 填充内容（模拟一次剪贴板写入）。
        fn set(&mut self, content: Option<&str>) {
            self.seq += 1;
            self.cells.insert(self.seq, content.map(|s| s.to_string()));
        }
    }

    impl ClipboardBackend for FakeBackend {
        fn sequence_number(&self) -> u32 {
            self.seq
        }
        fn read_text(&mut self) -> Result<Option<String>, String> {
            if self.fail_reads {
                return Err("fake: 剪贴板被占用（测试注入）".to_string());
            }
            match self.cells.get(&self.seq) {
                Some(content) => Ok(content.clone()),
                None => Ok(None),
            }
        }
    }

    /// 便捷：开启监听（baseline 为当前内容）。
    fn enable_with(monitor: &ClipboardMonitor, backend: &mut FakeBackend) {
        ok_of(monitor.enable(backend));
    }

    #[test]
    fn enable_baseline_not_triggered_and_new_text_triggers() {
        let monitor = ClipboardMonitor::default();
        let mut backend = FakeBackend::default();
        backend.set(Some("存量内容")); // 开启前的旧内容
        enable_with(&monitor, &mut backend);

        // 序号未变 → 无事
        assert_eq!(ok_of(monitor.tick(&mut backend)), TickOutcome::Nothing);
        // 新文本 → Trigger（存量内容不触发，之后新内容正常触发；FR-5.2）
        backend.set(Some("新文本"));
        assert_eq!(
            ok_of(monitor.tick(&mut backend)),
            TickOutcome::Trigger {
                text: "新文本".to_string()
            }
        );
        // 触发后基线已更新 → 再 tick 无事
        assert_eq!(ok_of(monitor.tick(&mut backend)), TickOutcome::Nothing);
    }

    #[test]
    fn duplicate_same_text_only_updates_seq() {
        let monitor = ClipboardMonitor::default();
        let mut backend = FakeBackend::default();
        backend.set(Some("相同文本"));
        enable_with(&monitor, &mut backend);

        // 复制相同文本（序号变化但内容相同）→ 只升序号不触发（D2/FR-5.5）
        backend.set(Some("相同文本"));
        assert_eq!(ok_of(monitor.tick(&mut backend)), TickOutcome::Nothing);
        // 再次重复 → 仍不触发
        backend.set(Some("相同文本"));
        assert_eq!(ok_of(monitor.tick(&mut backend)), TickOutcome::Nothing);
        // 换不同文本 → 触发
        backend.set(Some("另一文本"));
        assert_eq!(
            ok_of(monitor.tick(&mut backend)),
            TickOutcome::Trigger {
                text: "另一文本".to_string()
            }
        );
    }

    #[test]
    fn non_text_is_silent_and_monitoring_resumes() {
        let monitor = ClipboardMonitor::default();
        let mut backend = FakeBackend::default();
        backend.set(Some("文本a"));
        enable_with(&monitor, &mut backend);

        // 非文本（图片/文件）→ 静默（EX-8），序号升入基线
        backend.set(None);
        assert_eq!(ok_of(monitor.tick(&mut backend)), TickOutcome::Nothing);
        // 非文本后同一序号再 tick → 无事（基线已推进，避免重复读）
        assert_eq!(ok_of(monitor.tick(&mut backend)), TickOutcome::Nothing);
        // 随后文本内容正常触发（监听未被破坏）
        backend.set(Some("文本b"));
        assert_eq!(
            ok_of(monitor.tick(&mut backend)),
            TickOutcome::Trigger {
                text: "文本b".to_string()
            }
        );
    }

    #[test]
    fn suppression_window_swallows_write_and_restore() {
        let monitor = ClipboardMonitor::default();
        let mut backend = FakeBackend::default();
        backend.set(Some("原内容"));
        enable_with(&monitor, &mut backend);

        // T6 时序：备份前（begin）→ 注入 Ctrl+C（写取词所得）→ 恢复原内容 → end
        ok_of(monitor.begin_suppression());
        backend.set(Some("取词所得")); // 模拟复制写入
        assert_eq!(ok_of(monitor.tick(&mut backend)), TickOutcome::Nothing);
        backend.set(Some("原内容")); // 模拟恢复写入
        assert_eq!(ok_of(monitor.tick(&mut backend)), TickOutcome::Nothing);
        ok_of(monitor.end_suppression(&mut backend));

        // 抑制窗内两次序号变化均被吞掉：基线已刷新为「原内容」，稳态无事
        assert_eq!(ok_of(monitor.tick(&mut backend)), TickOutcome::Nothing);
        // 用户其后复制新内容 → 正常触发（FR-5.7 期间内容不触发监听）
        backend.set(Some("用户新复制"));
        assert_eq!(
            ok_of(monitor.tick(&mut backend)),
            TickOutcome::Trigger {
                text: "用户新复制".to_string()
            }
        );
    }

    #[test]
    fn restore_race_abandons_restore_and_user_content_becomes_baseline() {
        let monitor = ClipboardMonitor::default();
        let mut backend = FakeBackend::default();
        backend.set(Some("原内容"));
        enable_with(&monitor, &mut backend);

        // 竞态：取词窗口内用户自己复制了新内容（恢复前校验发现当前 ≠ 取词所得 → 放弃恢复）
        ok_of(monitor.begin_suppression());
        backend.set(Some("取词所得")); // 注入写入（吞）
        assert_eq!(ok_of(monitor.tick(&mut backend)), TickOutcome::Nothing);
        backend.set(Some("用户竞态复制")); // 用户自己的新复制（吞入基线，以用户新内容为准）
        assert_eq!(ok_of(monitor.tick(&mut backend)), TickOutcome::Nothing);
        // 放弃恢复：无恢复写入；end_suppression 仍刷新基线（= 用户新内容）并清位
        ok_of(monitor.end_suppression(&mut backend));

        // 用户竞态内容已被抑制窗消费（未重复触发，D2 口径）；监听状态一致，后续可正常触发
        assert_eq!(ok_of(monitor.tick(&mut backend)), TickOutcome::Nothing);
        backend.set(Some("后续复制"));
        assert_eq!(
            ok_of(monitor.tick(&mut backend)),
            TickOutcome::Trigger {
                text: "后续复制".to_string()
            }
        );
    }

    #[test]
    fn too_long_skips_translation_and_updates_baseline() {
        let monitor = ClipboardMonitor::default();
        let mut backend = FakeBackend::default();
        backend.set(Some("短文本"));
        enable_with(&monitor, &mut backend);

        let long_text = "测".repeat(MAX_AUTO_CHARS + 1);
        backend.set(Some(&long_text));
        assert_eq!(
            ok_of(monitor.tick(&mut backend)),
            TickOutcome::TooLong {
                chars: MAX_AUTO_CHARS + 1
            }
        );
        // baseline 已更新为超限文本：重复复制不重复通知（去重口径延续 D3）
        backend.set(Some(&long_text));
        assert_eq!(ok_of(monitor.tick(&mut backend)), TickOutcome::Nothing);
    }

    #[test]
    fn off_state_never_triggers_even_with_changes() {
        let monitor = ClipboardMonitor::default();
        let mut backend = FakeBackend::default();
        // 从未开启（或已 disable）→ 任何变化都不触发（FR-5.6 关闭立即失效）
        backend.set(Some("开启前的旧内容"));
        assert_eq!(ok_of(monitor.tick(&mut backend)), TickOutcome::Nothing);
        backend.set(Some("任意新内容"));
        assert_eq!(ok_of(monitor.tick(&mut backend)), TickOutcome::Nothing);
        assert!(!monitor.is_on());
    }

    #[test]
    fn disable_immediately_stops_triggering() {
        let monitor = ClipboardMonitor::default();
        let mut backend = FakeBackend::default();
        backend.set(Some("基线"));
        enable_with(&monitor, &mut backend);
        assert!(monitor.is_on());

        ok_of(monitor.disable());
        assert!(!monitor.is_on());
        backend.set(Some("关闭后复制"));
        assert_eq!(ok_of(monitor.tick(&mut backend)), TickOutcome::Nothing);
        // 关闭态 begin/end 抑制均为无操作（不残留状态）
        ok_of(monitor.begin_suppression());
        ok_of(monitor.end_suppression(&mut backend));
    }

    #[test]
    fn enable_propagates_read_error() {
        let monitor = ClipboardMonitor::default();
        let mut failing = FakeBackend {
            fail_reads: true,
            ..Default::default()
        };
        // 开启时读剪贴板失败 → 明确报错（不静默以“非文本”伪装成功）
        assert!(monitor.enable(&mut failing).is_err());
        assert!(!monitor.is_on());
    }

    #[test]
    fn read_error_during_tick_skips_and_keeps_monitoring() {
        let monitor = ClipboardMonitor::default();
        let mut backend = FakeBackend::default();
        backend.set(Some("基线"));
        enable_with(&monitor, &mut backend);

        backend.seq += 1; // 序号已变化
        backend.fail_reads = true; // 读取失败（如剪贴板被占用）
        assert_eq!(ok_of(monitor.tick(&mut backend)), TickOutcome::Nothing);
        // 基线未推进：恢复后同一内容再次 tick 仍处于“变化”状态并可正确读取
        backend.fail_reads = false;
        backend
            .cells
            .insert(backend.seq, Some("新文本".to_string()));
        assert_eq!(
            ok_of(monitor.tick(&mut backend)),
            TickOutcome::Trigger {
                text: "新文本".to_string()
            }
        );
    }
}
