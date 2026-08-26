//! selection：取词编排（T6 实现，spec 5.6.1②b [FR-3.1]、性能预算 7.2 取词 260ms、INV-5）。
//!
//! 时序契约（逐条落实 spec 5.6.1②b + FR-5.7，与 clipboard_monitor 的抑制接口配对）：
//!
//! ```text
//! 1. begin_suppression            —— 置抑制标志（**备份之前**，T5 接口契约：置位先于任何写入）
//! 2. 备份剪贴板（内容 + 序号）      —— 序号用于后续变化轮询；内容用于恢复
//! 3. 注入 Ctrl+C（SendInput，仅发按键，不碰任何窗口句柄——INV-5：全程不显示/聚焦 Alya 窗口）
//! 4. 轮询序号变化（上限 220ms）     —— 变化才读文本；无变化 → EX-10
//! 5. 读取取词所得（空白/非文本 → EX-9；读取异常 → EX-10）
//! 6. 恢复前校验：当前内容仍 == 取词所得才恢复备份；否则放弃恢复（用户自己复制了新内容，
//!    防覆盖用户数据）；
//! 7. end_suppression              —— 先刷新监听基线、后清位（T5 接口契约）
//! ```
//!
//! 失败路径（注入失败/超时/空白）同样执行 7（清位），绝不残留抑制标志导致监听静默（FR-5.7）。
//! 恢复校验失败（放弃恢复）时，监听基线以用户新内容为准（clipboard_monitor 语义），
//! 取词链路以读到的文本触发翻译，用户内容不会被旧备份覆盖。
//!
//! 通知口径（系统 toast，design-spec 4.4 面板外通知一行）：
//! - EX-9（空/空白选区）：「未获取到选中文本」（FR-3.2、需求 EX-9 呈现）；
//! - EX-10（超时/模拟复制无返回）：「未能获取文本」（FR-3.3、design-spec 4.4）。
//!   注意：requirements.md v0.4 的 EX-10 行文为「未获取到选中文本」，与 design-spec 4.4 的
//!   「未能获取文本」不一致；本实现按主 Agent T6 指令与 design-spec 4.4 执行，
//!   详见 implementation-report T6 节偏离说明。
//!
//! 击键与剪贴板均抽象为 trait（生产＝windows SendInput + arboard/user32；测试＝假实现注入）。

use std::time::{Duration, Instant};

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::clipboard_monitor::{self, ClipboardBackend, ClipboardMonitor};
use crate::error::AppError;

/// 取词默认超时上限（spec 5.6.1②b：220ms；属 7.2 取词环节预算）。
pub const SELECT_TIMEOUT: Duration = Duration::from_millis(220);
/// 序号变化轮询间隔（10ms 粒度，220ms 内最多 22 次查询，成本极低）。
pub const SELECT_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// EX-9 系统通知文案（FR-3.2 / design-spec 4.4：未获取到选中文本）。
pub const NOTIFY_NO_TEXT: &str = "未获取到选中文本";
/// EX-10 系统通知文案（FR-3.3 / design-spec 4.4：未能获取文本）。
pub const NOTIFY_FAILED: &str = "未能获取文本";

// ---------------------------------------------------------------------------
// 剪贴板 / 击键抽象（生产实现 + 测试注入）
// ---------------------------------------------------------------------------

/// 取词所需剪贴板操作（在 `clipboard_monitor::ClipboardBackend` 基础上追加写入能力）。
/// 设计为它的子 trait：T6 的恢复/清位接口需要把同一对象交给 `end_suppression`。
pub trait ClipboardOps: ClipboardBackend {
    /// 写文本（恢复原内容/清空自身残留）；返回失败原因（不吞异常，由调用方记录）。
    fn set_text(&mut self, text: &str) -> Result<(), String>;
}

/// 模拟复制注入抽象（生产＝SendInput 注入 Ctrl+C 到前台窗口；测试＝假注入器直接改假剪贴板）。
pub trait CopyInjector {
    /// 向前台窗口注入一次 Copy 组合键。`Err` ＝ 注入失败（EX-10 分类）。
    fn inject_copy(&self) -> Result<(), String>;
}

/// 生产剪贴板后端：user32 序号 + arboard 读写（与 clipboard_monitor 的 ArboardBackend 同源 API）。
pub struct WinClipboard {
    clipboard: arboard::Clipboard,
}

impl WinClipboard {
    pub fn new() -> Result<Self, String> {
        arboard::Clipboard::new()
            .map(|clipboard| Self { clipboard })
            .map_err(|err| err.to_string())
    }
}

impl ClipboardBackend for WinClipboard {
    fn sequence_number(&self) -> u32 {
        // SAFETY: GetClipboardSequenceNumber 无引用/指针参数，仅返回 u32 系统序号（user32 系统级计数）。
        unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() }
    }

    fn read_text(&mut self) -> Result<Option<String>, String> {
        match self.clipboard.get_text() {
            Ok(text) => Ok(Some(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(err) => Err(err.to_string()),
        }
    }
}

impl ClipboardOps for WinClipboard {
    fn set_text(&mut self, text: &str) -> Result<(), String> {
        self.clipboard
            .set_text(text.to_string())
            .map_err(|err| err.to_string())
    }
}

/// 生产击键注入：windows SendInput 依次发 Ctrl 按下、C 按下、C 释放、Ctrl 释放。
/// 只发虚拟键消息，不获取/聚焦/创建任何窗口（INV-5：取词全程不显示/聚焦 Alya 窗口）。
pub struct SendInputInjector;

impl CopyInjector for SendInputInjector {
    fn inject_copy(&self) -> Result<(), String> {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
            VK_C, VK_CONTROL,
        };

        fn keydown(vk: VIRTUAL_KEY) -> INPUT {
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        ..Default::default()
                    },
                },
            }
        }
        fn keyup(vk: VIRTUAL_KEY) -> INPUT {
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        dwFlags: KEYEVENTF_KEYUP,
                        ..Default::default()
                    },
                },
            }
        }

        let inputs = [
            keydown(VK_CONTROL),
            keydown(VK_C),
            keyup(VK_C),
            keyup(VK_CONTROL),
        ];
        // SAFETY: SendInput 仅向系统输入队列注入事件；pinputs 指向栈上数组且 cbsize 为其总字节数，
        // 内核只读该内存（返回注入成功的事件个数，不写回）。
        let injected = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
        if injected == inputs.len() as u32 {
            Ok(())
        } else {
            Err(format!(
                "SendInput 仅注入 {injected}/{} 个事件",
                inputs.len()
            ))
        }
    }
}

/// 取词参数（测试注入：缩短超时与轮询粒度，不实测等待 220ms）。
#[derive(Debug, Clone, Copy)]
pub struct SelectionOptions {
    pub timeout: Duration,
    pub poll_interval: Duration,
}

impl Default for SelectionOptions {
    fn default() -> Self {
        SelectionOptions {
            timeout: SELECT_TIMEOUT,
            poll_interval: SELECT_POLL_INTERVAL,
        }
    }
}

// ---------------------------------------------------------------------------
// 编排核心（纯逻辑，测试经 trait 注入驱动）
// ---------------------------------------------------------------------------

/// 执行取词全链路（spec 5.6.1②b 时序，见模块头）。
/// 无论成功失败，函数返回时抑制标志必已清除（失败路径同样清理，FR-5.7）。
pub fn select_text(
    clipboard: &mut dyn ClipboardOps,
    injector: &dyn CopyInjector,
    monitor: &ClipboardMonitor,
    options: &SelectionOptions,
) -> Result<String, AppError> {
    // 1) 抑制：备份之前置位（T5 契约：置位先于任何写入，写入序号变化必然落入抑制窗）。
    monitor.begin_suppression()?;

    let outcome = capture_and_restore(clipboard, injector, monitor, options);

    // 7) 清位：恢复/放弃恢复并刷新基线之后调用（T5 契约：先刷新基线后清位）。
    //    失败路径也必须清位——否则抑制位卡死会吞掉用户后续复制内容。
    if let Err(err) = monitor.end_suppression(clipboard) {
        log::error!("selection: 结束抑制失败（抑制标志可能残留，监听基线未刷新）: {err}");
    }
    outcome
}

fn capture_and_restore(
    clipboard: &mut dyn ClipboardOps,
    injector: &dyn CopyInjector,
    _monitor: &ClipboardMonitor, // 预留：恢复竞态/清位观察点（当前时序校验在 select_text 层）
    options: &SelectionOptions,
) -> Result<String, AppError> {
    // 2) 备份（内容 + 序号）：内容用于恢复、序号用于识别「取词写入是否发生」。
    let backup_seq = clipboard.sequence_number();
    let backup_text = clipboard.read_text().map_err(|err| {
        AppError::SelectionFailed(format!("备份剪贴板失败（当前内容未被修改）: {err}"))
    })?;

    // 3) 注入 Ctrl+C（INV-5：不触碰任何 Alya 窗口；系统限制场景由上层按 EX-10 呈现）。
    injector
        .inject_copy()
        .map_err(|err| AppError::SelectionFailed(format!("模拟复制注入失败: {err}")))?;

    // 4) 轮询序号变化（上限 220ms；无变化 = 目标应用不支持模拟复制 → EX-10）。
    let deadline = Instant::now() + options.timeout;
    loop {
        if clipboard.sequence_number() != backup_seq {
            break;
        }
        if Instant::now() >= deadline {
            return Err(AppError::SelectionFailed(format!(
                "{}ms 内未检测到剪贴板序号变化（应用可能不支持模拟复制）",
                options.timeout.as_millis()
            )));
        }
        std::thread::sleep(options.poll_interval);
    }

    // 5) 读取取词所得。
    let text = match clipboard.read_text() {
        Ok(Some(text)) => text,
        Ok(None) => {
            return Err(AppError::EmptyText(
                "剪贴板变化后内容为非文本（未取得选中文本）".to_string(),
            ));
        }
        Err(err) => {
            return Err(AppError::SelectionFailed(format!(
                "读取取词结果失败: {err}"
            )));
        }
    };
    if text.trim().is_empty() {
        return Err(AppError::EmptyText(
            "取词所得为空白（未选中有效文本）".to_string(),
        ));
    }

    // 6) 恢复前校验：当前内容仍是取词所得才恢复，否则放弃（防覆盖用户在竞争窗口内复制的新内容）。
    match clipboard.read_text() {
        Ok(Some(current)) if current == text => {
            match &backup_text {
                Some(backup) => {
                    if let Err(err) = clipboard.set_text(backup) {
                        // 恢复失败不阻断取词（文本已取得）；显式记录，不静默。
                        log::warn!("selection: 恢复原剪贴板内容失败（取词残留留在剪贴板）: {err}");
                    }
                }
                None => {
                    // 原内容为非文本（图片/文件无法经 arboard 恢复）：清掉取词残留，不污染用户剪贴板。
                    if let Err(err) = clipboard.set_text("") {
                        log::warn!("selection: 清除取词残留失败（非文本备份不可恢复）: {err}");
                    }
                }
            }
        }
        Ok(Some(_)) => {
            log::debug!("selection: 恢复前校验：用户已复制新内容（当前 ≠ 取词所得），放弃恢复以防覆盖用户数据");
        }
        Ok(None) => {
            log::debug!(
                "selection: 恢复前校验：剪贴板已变为非文本（用户复制了图片/文件），放弃恢复"
            );
        }
        Err(err) => {
            log::warn!("selection: 恢复前校验读取失败（放弃恢复以防写坏用户数据）: {err}");
        }
    }

    Ok(text)
}

// ---------------------------------------------------------------------------
// 生产入口（通知呈现 + selection_done 打点；INV-5 全程不显示/聚焦 Alya 窗口）
// ---------------------------------------------------------------------------

/// 生产取词入口：`action_router.selection_translate` ②b 调用（T16 接线）。
/// 失败时返回 Err 且已按 EX-9/EX-10 分路发送系统通知（不弹面板）。
pub fn run_selection(app: &AppHandle) -> Result<String, AppError> {
    let started = Instant::now();
    let result = (|| -> Result<String, AppError> {
        let mut clipboard = WinClipboard::new()
            .map_err(|err| AppError::SelectionFailed(format!("剪贴板初始化失败: {err}")))?;
        let injector = SendInputInjector;
        select_text(
            &mut clipboard,
            &injector,
            clipboard_monitor::global(),
            &SelectionOptions::default(),
        )
    })();

    match &result {
        Ok(text) => {
            // 7.2 selection_done 打点：环节耗时 + 仅记字符数（INV-7：绝不记录原文内容）。
            log::debug!(
                "selection: selection_done 取词完成（{}ms，{} 字符）",
                started.elapsed().as_millis(),
                text.chars().count()
            );
        }
        Err(err) => {
            notify_selection_error(app, err);
            log::warn!(
                "selection: 取词失败（{}，{}ms）",
                err.ex_code(),
                started.elapsed().as_millis()
            );
        }
    }
    result
}

/// EX-9/EX-10 系统通知（design-spec 4.4 面板外通知口径）；其余内部错误仅记日志。
fn notify_selection_error(app: &AppHandle, err: &AppError) {
    let message = match err.ex_code() {
        "EX-9" => NOTIFY_NO_TEXT,
        "EX-10" => NOTIFY_FAILED,
        _ => return, // 内部错误（如状态锁中毒）不上系统通知：非用户操作场景。
    };
    if let Err(notify_err) = app
        .notification()
        .builder()
        .title("Alya")
        .body(message)
        .show()
    {
        log::error!("selection: 系统通知发送失败: {notify_err}");
    }
}

// ---------------------------------------------------------------------------
// 单元测试：成功/超时/空白/恢复竞态/抑制时序/失败清理（trait 注入假实现）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};

    use crate::clipboard_monitor::TickOutcome;

    /// 共享剪贴板材存储：clipboard 与 injector 经同一 Arc<Mutex<>> 观察/写入，模拟真实剪贴板。
    #[derive(Default)]
    struct FakeStore {
        seq: u32,
        cells: HashMap<u32, Option<String>>,
        /// set_text 写入日志（断言恢复/清空行为）。
        writes: Vec<String>,
        /// injector 每次注入时写入的文本（None = 注入后为非文本）。
        copied: Option<String>,
        /// true = 注入器不写剪贴板（模拟目标应用不响应复制）。
        inject_noop: bool,
    }

    struct FakeClipboard {
        store: Arc<Mutex<FakeStore>>,
        monitor: &'static ClipboardMonitor,
        fail_reads: bool,
        fail_writes: bool,
        /// 第 N 次 read_text 前先注入用户竞态内容（模拟用户复制窗口）。
        race_on_read: u32,
    }

    impl FakeClipboard {
        fn observe_suppressing(&self, op: &str) {
            assert!(
                self.monitor.is_suppressing(),
                "抑制标志应在 {op} 时已置位（T6 时序：置位先于备份、清位晚于恢复）"
            );
        }

        fn lock_store(&self) -> std::sync::MutexGuard<'_, FakeStore> {
            match self.store.lock() {
                Ok(guard) => guard,
                Err(_) => panic!("fake: 存储锁中毒"),
            }
        }
    }

    impl ClipboardBackend for FakeClipboard {
        fn sequence_number(&self) -> u32 {
            self.lock_store().seq
        }

        fn read_text(&mut self) -> Result<Option<String>, String> {
            if self.fail_reads {
                return Err("fake: 剪贴板被占用（测试注入）".to_string());
            }
            if self.race_on_read > 0 {
                self.race_on_read -= 1;
                if self.race_on_read == 0 {
                    // 模拟用户在竞争窗口内复制了新内容（恢复前校验将看到 ≠ 取词所得）。
                    let mut store = self.lock_store();
                    store.seq += 1;
                    let seq = store.seq;
                    store.cells.insert(seq, Some("用户竞态复制".to_string()));
                }
            }
            let store = self.lock_store();
            Ok(store.cells.get(&store.seq).cloned().flatten())
        }
    }

    impl ClipboardOps for FakeClipboard {
        fn set_text(&mut self, text: &str) -> Result<(), String> {
            self.observe_suppressing("恢复写入");
            if self.fail_writes {
                return Err("fake: 写入失败（测试注入）".to_string());
            }
            let mut store = self.lock_store();
            store.seq += 1;
            store.writes.push(text.to_string());
            let seq = store.seq;
            store.cells.insert(seq, Some(text.to_string()));
            Ok(())
        }
    }

    struct FakeInjector {
        store: Arc<Mutex<FakeStore>>,
    }

    impl CopyInjector for FakeInjector {
        fn inject_copy(&self) -> Result<(), String> {
            let mut store = match self.store.lock() {
                Ok(guard) => guard,
                Err(_) => return Err("fake: 存储锁中毒".to_string()),
            };
            if store.inject_noop {
                return Ok(()); // 模拟应用不响应：不写剪贴板，序号不变（轮询超时路径）。
            }
            store.seq += 1;
            let seq = store.seq;
            let copied = store.copied.clone();
            store.cells.insert(seq, copied);
            Ok(())
        }
    }

    /// 每个测试独立 monitor（避免全局状态串扰）；实例经 Box::leak 拥有 'static 生命周期。
    fn test_monitor() -> &'static ClipboardMonitor {
        let monitor: &'static ClipboardMonitor = Box::leak(Box::new(ClipboardMonitor::default()));
        // 保留引用防测试析构顺序告警（实际为持有句柄，无逻辑作用）。
        static LEAKED: OnceLock<Mutex<Vec<&'static ClipboardMonitor>>> = OnceLock::new();
        if let Ok(mut v) = LEAKED.get_or_init(|| Mutex::new(Vec::new())).lock() {
            v.push(monitor);
        }
        monitor
    }

    fn setup(initial: Option<&str>) -> (&'static ClipboardMonitor, Arc<Mutex<FakeStore>>) {
        let monitor = test_monitor();
        let store = FakeStore {
            cells: HashMap::from([(0u32, initial.map(|s| s.to_string()))]),
            ..Default::default()
        };
        (monitor, Arc::new(Mutex::new(store)))
    }

    fn fake_clipboard(
        monitor: &'static ClipboardMonitor,
        store: Arc<Mutex<FakeStore>>,
        fail_writes: bool,
    ) -> FakeClipboard {
        FakeClipboard {
            store,
            monitor,
            fail_reads: false,
            fail_writes,
            race_on_read: 0,
        }
    }

    fn fast_options() -> SelectionOptions {
        SelectionOptions {
            timeout: Duration::from_millis(80),
            poll_interval: Duration::from_millis(2),
        }
    }

    fn ok_of<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(err) => panic!("期望成功，实际错误: {err}"),
        }
    }

    #[test]
    fn success_selects_text_restores_backup_and_clears_suppression() {
        let (monitor, store) = setup(Some("原内容"));
        let mut clipboard = fake_clipboard(monitor, store, false);
        ok_of(monitor.enable(&mut clipboard)); // On 态：FR-5.7 抑制语义才生效（Off=无操作不测时序）
        let injector = FakeInjector {
            store: clipboard.store.clone(),
        };
        {
            let mut store = clipboard.lock_store();
            store.copied = Some("选中文本".to_string());
        }
        // 成功路径：备份读时抑制已置位（observe_suppressing 断言）；恢复写时抑制仍置位；返回时已清位。
        let text = ok_of(select_text(
            &mut clipboard,
            &injector,
            monitor,
            &fast_options(),
        ));
        assert_eq!(text, "选中文本");
        {
            let store = clipboard.lock_store();
            assert_eq!(store.writes, vec!["原内容".to_string()], "应恢复原内容");
        }
        assert!(
            !monitor.is_suppressing(),
            "清位应晚于恢复（select_text 返回前完成）"
        );
    }

    #[test]
    fn timeout_reports_ex10_and_recovers_suppression() {
        let (monitor, store) = setup(Some("原内容"));
        let mut clipboard = fake_clipboard(monitor, store, false);
        ok_of(monitor.enable(&mut clipboard));
        {
            let mut store = clipboard.lock_store();
            store.inject_noop = true; // 应用不响应 → 序号永不变化
        }
        let injector = FakeInjector {
            store: clipboard.store.clone(),
        };
        let err = match select_text(&mut clipboard, &injector, monitor, &fast_options()) {
            Ok(_) => panic!("超时应报 EX-10 而非成功"),
            Err(err) => err,
        };
        assert_eq!(err.ex_code(), "EX-10");
        assert!(!monitor.is_suppressing(), "失败路径也必须清位（FR-5.7）");
        // 清位后监听以当前内容为基线：相同内容 tick 无事（不误触发、不残留抑制）。
        assert_eq!(ok_of(monitor.tick(&mut clipboard)), TickOutcome::Nothing);
    }

    #[test]
    fn blank_and_none_text_report_ex9() {
        // 空白（纯空白取词结果）
        let (monitor, store) = setup(None);
        let mut clipboard = fake_clipboard(monitor, store, false);
        ok_of(monitor.enable(&mut clipboard));
        {
            let mut store = clipboard.lock_store();
            store.copied = Some("   \t\n ".to_string());
        }
        let injector = FakeInjector {
            store: clipboard.store.clone(),
        };
        let err = match select_text(&mut clipboard, &injector, monitor, &fast_options()) {
            Ok(_) => panic!("空白取词应报 EX-9"),
            Err(err) => err,
        };
        assert_eq!(err.ex_code(), "EX-9");
        assert!(!monitor.is_suppressing());

        // 非文本（剪贴板变化后内容为 None）
        let (monitor, store) = setup(None);
        let mut clipboard = fake_clipboard(monitor, store, false);
        ok_of(monitor.enable(&mut clipboard));
        {
            let mut store = clipboard.lock_store();
            store.copied = None;
        }
        let injector = FakeInjector {
            store: clipboard.store.clone(),
        };
        let err = match select_text(&mut clipboard, &injector, monitor, &fast_options()) {
            Ok(_) => panic!("非文本取词应报 EX-9"),
            Err(err) => err,
        };
        assert_eq!(err.ex_code(), "EX-9");
        assert!(!monitor.is_suppressing());
    }

    #[test]
    fn restore_race_abandons_restore_to_protect_user_data() {
        let (monitor, store) = setup(Some("原内容"));
        let mut clipboard = fake_clipboard(monitor, store, false);
        ok_of(monitor.enable(&mut clipboard));
        {
            let mut store = clipboard.lock_store();
            store.copied = Some("选中文本".to_string());
        }
        // 武装后第 3 次 read_text（= 恢复前校验读；enable 的基线读发生在武装之前）前注入用户竞态内容。
        clipboard.race_on_read = 3;
        let injector = FakeInjector {
            store: clipboard.store.clone(),
        };
        let text = ok_of(select_text(
            &mut clipboard,
            &injector,
            monitor,
            &fast_options(),
        ));
        assert_eq!(text, "选中文本");
        {
            let store = clipboard.lock_store();
            assert!(
                store.writes.is_empty(),
                "竞态场景不得写回旧备份（防覆盖用户新内容）"
            );
        }
        assert!(!monitor.is_suppressing());
        // end_suppression 已刷新基线为用户竞态内容，且该内容已被抑制窗消费（D2 口径不重复触发）。
        assert_eq!(ok_of(monitor.tick(&mut clipboard)), TickOutcome::Nothing);
    }

    #[test]
    fn suppression_timing_begin_before_backup_end_after_restore() {
        let (monitor, store) = setup(Some("原内容"));
        let mut clipboard = fake_clipboard(monitor, store, false);
        ok_of(monitor.enable(&mut clipboard));
        {
            let mut store = clipboard.lock_store();
            store.copied = Some("选中文本".to_string());
        }
        let injector = FakeInjector {
            store: clipboard.store.clone(),
        };
        // FakeClipboard 的 read_text/set_text 在调用时断言标志状态：
        // - 备份读（第一次 read_text）时标志必须已置位 → 证明「置位先于备份」；
        // - 恢复写（set_text）时标志必须仍为 true → 证明「清位晚于恢复」。
        ok_of(select_text(
            &mut clipboard,
            &injector,
            monitor,
            &fast_options(),
        ));
        assert!(!monitor.is_suppressing(), "返回时抑制标志必须已清除");
    }

    #[test]
    fn injection_failure_reports_ex10_and_cleans_up() {
        let (monitor, store) = setup(Some("原内容"));
        let mut clipboard = fake_clipboard(monitor, store, false);
        ok_of(monitor.enable(&mut clipboard));
        struct FailingInjector;
        impl CopyInjector for FailingInjector {
            fn inject_copy(&self) -> Result<(), String> {
                Err("fake: 注入失败（测试注入）".to_string())
            }
        }
        let err = match select_text(&mut clipboard, &FailingInjector, monitor, &fast_options()) {
            Ok(_) => panic!("注入失败应报 EX-10"),
            Err(err) => err,
        };
        assert_eq!(err.ex_code(), "EX-10");
        assert!(!monitor.is_suppressing());
    }

    #[test]
    fn restore_write_failure_warns_but_selection_succeeds() {
        let (monitor, store) = setup(Some("原内容"));
        let mut clipboard = fake_clipboard(monitor, store, true); // 恢复写失败（如剪贴板被瞬间占用）
        ok_of(monitor.enable(&mut clipboard));
        {
            let mut store = clipboard.lock_store();
            store.copied = Some("选中文本".to_string());
        }
        let injector = FakeInjector {
            store: clipboard.store.clone(),
        };
        // 文本已取得 → 返回成功；恢复失败仅记录（不静默但也不伪装取词失败）。
        let text = ok_of(select_text(
            &mut clipboard,
            &injector,
            monitor,
            &fast_options(),
        ));
        assert_eq!(text, "选中文本");
        assert!(!monitor.is_suppressing());
    }

    #[test]
    fn non_text_backup_restore_clears_own_residue() {
        let (monitor, store) = setup(None); // 原内容为非文本（图片/文件无法经 arboard 恢复）
        let mut clipboard = fake_clipboard(monitor, store, false);
        ok_of(monitor.enable(&mut clipboard));
        {
            let mut store = clipboard.lock_store();
            store.copied = Some("选中文本".to_string());
        }
        let injector = FakeInjector {
            store: clipboard.store.clone(),
        };
        let text = ok_of(select_text(
            &mut clipboard,
            &injector,
            monitor,
            &fast_options(),
        ));
        assert_eq!(text, "选中文本");
        {
            let store = clipboard.lock_store();
            assert_eq!(
                store.writes,
                vec!["".to_string()],
                "非文本备份不可恢复，应清空取词残留而非把脏数据留在用户剪贴板"
            );
        }
    }

    #[test]
    fn after_success_monitor_baseline_is_restored_content_and_user_copy_triggers() {
        // 端到端：取词成功后，监听基线 = 恢复内容；随后用户新复制仍正常触发（抑制未卡死）。
        let (monitor, store) = setup(Some("原内容"));
        let mut clipboard = fake_clipboard(monitor, store, false);
        ok_of(monitor.enable(&mut clipboard));
        {
            let mut store = clipboard.lock_store();
            store.copied = Some("选中文本".to_string());
        }
        let injector = FakeInjector {
            store: clipboard.store.clone(),
        };
        ok_of(select_text(
            &mut clipboard,
            &injector,
            monitor,
            &fast_options(),
        ));
        // 基线 = 「原内容」（恢复后 end_suppression 刷新）：相同内容 tick 无事。
        assert_eq!(ok_of(monitor.tick(&mut clipboard)), TickOutcome::Nothing);
        // 用户其后复制新内容 → 正常触发（FR-5.2，抑制未残留）。
        {
            let mut store = clipboard.lock_store();
            store.seq += 1;
            let seq = store.seq;
            store.cells.insert(seq, Some("用户新复制".to_string()));
        }
        assert_eq!(
            ok_of(monitor.tick(&mut clipboard)),
            TickOutcome::Trigger {
                text: "用户新复制".to_string()
            }
        );
    }

    #[test]
    fn read_failure_during_backup_reports_ex10_and_cleans_up() {
        let (monitor, store) = setup(Some("原内容"));
        let mut clipboard = fake_clipboard(monitor, store, false);
        ok_of(monitor.enable(&mut clipboard));
        clipboard.fail_reads = true; // 备份读失败（剪贴板被占用）
        let injector = FakeInjector {
            store: clipboard.store.clone(),
        };
        let err = match select_text(&mut clipboard, &injector, monitor, &fast_options()) {
            Ok(_) => panic!("备份读失败应报 EX-10"),
            Err(err) => err,
        };
        assert_eq!(err.ex_code(), "EX-10");
        assert!(!monitor.is_suppressing(), "备份失败同样必须清位");
    }
}
