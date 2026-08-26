//! service：翻译会话管理（T7 实现，spec 5.7.7 会话模型；EX-13 顶替；分档判定纯函数）。
//!
//! 会话模型（spec 5.7.7）：`sessions: Mutex<HashMap<Source, TranslationRound>>` +
//! 全局 `generation: AtomicU64`。`TranslationRound { round_id, cancel: CancellationToken,
//! join_handle }`。
//!
//! 新翻译顶替（EX-13）：同一来源新请求 → bump generation → 旧行 `cancel()`（CancellationToken 触发 +
//! reqwest 流被 drop → TCP 关闭，服务端感知断连停止生成，不残留配额幽灵连接【A7】）→ 等待旧任务
//! join（上限 100ms）→ 才发起新请求。**任意时刻每来源至多一条活跃连接**：本模块对每个来源提供
//! 一个串行门（tokio::sync::Mutex）——同源并发请求在门后排队，前一个完成 cancel+join+spawn 后
//! 下一个才开始，从根上排除「双活跃连接」竞态。
//!
//! 锁序：**先 config 后 sessions**（config 读锁必须在本模块 sessions 锁之前获取且不跨锁持有——
//! 见 `start_translate` 中先组装 prompt、后进 `start_translate_with_prompt` 的实现）。
//!
//! 分档判定（C1 裁决：Rust 纯函数并单测）：`decide_render_mode(text_len, threshold)`
//! → `stream`（≤1000）| `block`（>1000）；计数口径 Q10＝字符数（含空白与换行），
//! 见 `count_chars`。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::config::{self, PresentationMode};
use crate::error::AppError;
use crate::events::{RenderMode, TranslateResult, TranslateSource};

/// EX-13 顶替时等待旧任务退出的上限（spec 5.7.7：100ms）。
pub const JOIN_OLD_TIMEOUT: Duration = Duration::from_millis(100);

/// 一轮翻译会话（活跃行；旧行被顶替时经 token 取消 + join handle 回收）。
#[derive(Debug)]
pub struct TranslationRound {
    pub round_id: String,
    token: CancellationToken,
    handle: tauri::async_runtime::JoinHandle<()>,
}

/// 翻译执行器抽象：生产＝sse.rs 的 `SseTranslateExecutor`（发流式请求 + 事件推送）；
/// 测试＝假执行器（统计活跃数/可注入不响应取消的任务）。
///
/// 契约：`spawn` 起动一个独立任务，任务在自身正常完成或被 `cancel` 取消后结束。
pub trait TranslateExecutor: Send + Sync + 'static {
    fn spawn(
        &self,
        round_id: String,
        request_text: String,
        cancel: CancellationToken,
    ) -> tauri::async_runtime::JoinHandle<()>;
}

/// 翻译会话表（进程内单份；锁序：先 config 后 sessions，禁止嵌套反向）。
pub struct TranslationService {
    sessions: Mutex<HashMap<TranslateSource, TranslationRound>>,
    /// 每来源串行门（同源 start 串行化，防「双活跃连接」竞态）。
    gates: Mutex<HashMap<TranslateSource, Arc<tokio::sync::Mutex<()>>>>,
    /// 全局代次（bump 语义 + roundId 唯一来源）。
    generation: AtomicU64,
    join_old_timeout: Duration,
}

impl Default for TranslationService {
    fn default() -> Self {
        TranslationService::new(JOIN_OLD_TIMEOUT)
    }
}

impl TranslationService {
    /// 显式构造（测试可缩短 join 上限以加速）。
    pub fn new(join_old_timeout: Duration) -> Self {
        TranslationService {
            sessions: Mutex::new(HashMap::new()),
            gates: Mutex::new(HashMap::new()),
            generation: AtomicU64::new(0),
            join_old_timeout,
        }
    }

    /// 某来源的活跃轮 id（诊断/测试）。无活跃轮 → None。
    pub fn active_round_id(&self, source: TranslateSource) -> Option<String> {
        match self.lock_sessions() {
            Ok(map) => map.get(&source).map(|round| round.round_id.clone()),
            Err(err) => {
                log::error!("translate: 会话表查询失败（视同无活跃轮）: {err}");
                None
            }
        }
    }

    /// 当前活跃轮数（每来源至多 1，值域 0..=来源数）。
    pub fn active_connections(&self) -> usize {
        match self.lock_sessions() {
            Ok(map) => map.len(),
            Err(err) => {
                log::error!("translate: 会话表统计失败（视同 0）: {err}");
                0
            }
        }
    }

    /// 翻译入口（spec 5.6.1⑥/5.6.2⑧）：读活动场景 → 构造提示词（定向 target＝面板临时值，
    /// 否则设置页默认值 FR-2.7）→ 进会话表。锁序：先 config（读完即释放）后 sessions。
    /// `E` 泛型化执行器：生产传 `&Arc<dyn TranslateExecutor>`，测试可传具体类型保持断言可访问。
    pub async fn start_translate<E: TranslateExecutor + ?Sized>(
        &self,
        source: TranslateSource,
        text: String,
        executor: &Arc<E>,
    ) -> Result<TranslateResult, AppError> {
        // —— config 读锁只在这一个短作用域内持有（先 config 后 sessions 的锁序起点）——
        let prompt = {
            let settings = config::read_global()?;
            let scene = settings.scene.active;
            let target = crate::translate::prompt::panel_temp_target()
                .unwrap_or_else(|| settings.translation.default_target_lang.clone());
            crate::translate::prompt::build_prompt(scene, Some(&target), &text)?
        };
        self.start_translate_with_prompt(source, prompt, executor)
            .await
    }

    /// 以现成提示词启动一轮（ocr 等已构造提示词的调用方与单元测试使用）。
    /// EX-13 顶替时序：bump generation → cancel 旧行 → join 旧任务（≤100ms）→ spawn 新任务。
    pub async fn start_translate_with_prompt<E: TranslateExecutor + ?Sized>(
        &self,
        source: TranslateSource,
        request_text: String,
        executor: &Arc<E>,
    ) -> Result<TranslateResult, AppError> {
        let gate = self.gate(source)?;
        let _gate_guard = gate.lock_owned().await;

        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let round_id = format!("r-{generation}");

        // 顶替旧行：cancel → join（≤join_old_timeout）后才发新请求（spec 5.7.7；EX-13）。
        let old_round = {
            let mut map = self.lock_sessions()?;
            map.remove(&source)
        };
        if let Some(old) = old_round {
            old.token.cancel();
            match tokio::time::timeout(self.join_old_timeout, old.handle).await {
                Ok(Ok(())) => log::debug!("translate: 旧轮已取消并退出（round={}）", old.round_id),
                Ok(Err(join_err)) => log::warn!(
                    "translate: 旧轮任务退出异常（round={}）: {join_err}",
                    old.round_id
                ),
                Err(_elapsed) => log::warn!(
                    "translate: 旧轮未在 {:?} 内退出（round={}，将由网络超时兜底关闭）",
                    self.join_old_timeout,
                    old.round_id
                ),
            }
        }

        // 新轮入表（先 spawn 后入表：spawn 内部可能异步发起请求，入表时本轮即活跃）。
        let cancel = CancellationToken::new();
        let handle = executor.spawn(round_id.clone(), request_text, cancel.clone());
        {
            let mut map = self.lock_sessions()?;
            map.insert(
                source,
                TranslationRound {
                    round_id: round_id.clone(),
                    token: cancel,
                    handle,
                },
            );
            log::debug!(
                "translate: 新轮已登记（source={source:?}, round={round_id}, 活跃数={}）",
                map.len()
            );
        }
        Ok(TranslateResult { round_id })
    }

    /// 显式取消（6.1 `cancel_translation {roundId}`）：cancel token + join（≤100ms）。
    /// 轮次不存在/已结束 → 内部错误（幂等调用方按已结束处理）。
    pub async fn cancel_translation(&self, round_id: &str) -> Result<(), AppError> {
        let old = {
            let mut map = self.lock_sessions()?;
            let mut matched: Option<TranslateSource> = None;
            for (source, round) in map.iter() {
                if round.round_id == round_id {
                    matched = Some(*source);
                    break;
                }
            }
            match matched {
                Some(source) => map.remove(&source),
                None => None,
            }
        };
        match old {
            Some(round) => {
                let id = round.round_id.clone();
                round.token.cancel();
                match tokio::time::timeout(self.join_old_timeout, round.handle).await {
                    Ok(Ok(())) => log::debug!("translate: 轮已取消并退出（round={id}）"),
                    Ok(Err(join_err)) => {
                        log::warn!("translate: 被取消任务退出异常（round={id}）: {join_err}")
                    }
                    Err(_) => log::warn!(
                        "translate: 被取消任务未在 {:?} 内退出（round={id}，底层连接将由自身超时兜底）",
                        self.join_old_timeout
                    ),
                }
                Ok(())
            }
            None => Err(AppError::Config(format!(
                "翻译轮次不存在或已结束（round={round_id}）"
            ))),
        }
    }

    fn gate(&self, source: TranslateSource) -> Result<Arc<tokio::sync::Mutex<()>>, AppError> {
        let mut gates = self
            .gates
            .lock()
            .map_err(|_| AppError::Config("translate: 会话门表锁中毒".to_string()))?;
        Ok(gates
            .entry(source)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone())
    }

    fn lock_sessions(
        &self,
    ) -> Result<MutexGuard<'_, HashMap<TranslateSource, TranslationRound>>, AppError> {
        self.sessions
            .lock()
            .map_err(|_| AppError::Config("translate: 会话表锁中毒".to_string()))
    }
}

static SERVICE: OnceLock<TranslationService> = OnceLock::new();

/// 全局翻译会话表（进程内单份；T16 装配后由此发翻译）。
pub fn global() -> &'static TranslationService {
    SERVICE.get_or_init(TranslationService::default)
}

// ---------------------------------------------------------------------------
// 分档判定（C1 裁决：Rust 纯函数；FR-10.1/10.2；Q10 口径＝字符数含空白换行）
// ---------------------------------------------------------------------------

/// 计数口径（Q10）：字符数（含空白与换行）。长文整段阈值比较以本函数结果为准。
pub fn count_chars(text: &str) -> usize {
    text.chars().count()
}

/// 分档呈现判定：`text_len <= threshold` → 流式；`> threshold` → 整段（999/1000/1001 边界）。
pub fn decide_render_mode(text_len: usize, threshold: u32) -> RenderMode {
    if text_len > threshold as usize {
        RenderMode::Block
    } else {
        RenderMode::Stream
    }
}

/// 呈现决策全量（FR-10.1 两档）：`stream_all`（出厂默认）恒流式；`staged` 才走分档阈值。
pub fn decide_presentation(
    presentation: PresentationMode,
    text_len: usize,
    threshold: u32,
) -> RenderMode {
    match presentation {
        PresentationMode::StreamAll => RenderMode::Stream,
        PresentationMode::Staged => decide_render_mode(text_len, threshold),
    }
}

// ---------------------------------------------------------------------------
// 单元测试：同源顶替 / 跨源隔离 / 取消后 join / 并发不变量 / 分档边界三分例
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex as StdMutex;

    use crate::events::TranslateSource;

    /// 假执行器：记录 start/finish 次序与活跃并发峰值；可注入「无视取消」（join 超时路径）。
    struct FakeExecutor {
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
        events: Arc<StdMutex<Vec<String>>>,
        /// true＝响应取消并退出；false＝for-ever pending（模仿不响应取消的连接）。
        respond_to_cancel: bool,
    }

    impl Default for FakeExecutor {
        fn default() -> Self {
            FakeExecutor {
                active: Arc::new(AtomicUsize::new(0)),
                max_active: Arc::new(AtomicUsize::new(0)),
                events: Arc::new(StdMutex::new(Vec::new())),
                respond_to_cancel: true,
            }
        }
    }

    impl FakeExecutor {
        fn events(&self) -> Vec<String> {
            match self.events.lock() {
                Ok(events) => events.clone(),
                Err(_) => Vec::new(),
            }
        }
    }

    impl TranslateExecutor for FakeExecutor {
        fn spawn(
            &self,
            round_id: String,
            _request_text: String,
            cancel: CancellationToken,
        ) -> tauri::async_runtime::JoinHandle<()> {
            // start 事件与活跃计数在 spawn 入口**同步**登记：即使后台任务尚未被调度，
            // 测试断言的事件次序与并发峰值也是确定性的（取消→join→新 spawn 的时序由门保证）。
            let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(now, Ordering::SeqCst);
            let _ = self
                .events
                .lock()
                .map(|mut e| e.push(format!("start:{round_id}")));
            let active = self.active.clone();
            let events = self.events.clone();
            let respond = self.respond_to_cancel;
            tauri::async_runtime::spawn(async move {
                if respond {
                    cancel.cancelled().await;
                    active.fetch_sub(1, Ordering::SeqCst);
                    let _ = events
                        .lock()
                        .map(|mut e| e.push(format!("finish:{round_id}")));
                } else {
                    // 不响应取消：模拟网络层卡死（join 上限路径）。
                    std::future::pending::<()>().await;
                }
            })
        }
    }

    fn ok_of<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(err) => panic!("期望成功，实际错误: {err}"),
        }
    }

    #[tokio::test]
    async fn same_source_supersedes_cancels_and_joins_before_new_spawn() {
        let service = TranslationService::default();
        let executor = Arc::new(FakeExecutor::default());
        let first = ok_of(
            service
                .start_translate_with_prompt(
                    TranslateSource::Selection,
                    "p1".to_string(),
                    &executor,
                )
                .await,
        );
        assert_eq!(first.round_id, "r-1");
        assert_eq!(
            service
                .active_round_id(TranslateSource::Selection)
                .as_deref(),
            Some("r-1")
        );

        let second = ok_of(
            service
                .start_translate_with_prompt(
                    TranslateSource::Selection,
                    "p2".to_string(),
                    &executor,
                )
                .await,
        );
        assert_eq!(second.round_id, "r-2");
        assert_eq!(
            service
                .active_round_id(TranslateSource::Selection)
                .as_deref(),
            Some("r-2")
        );

        // EX-13 时序：旧轮 finish 必须先于新轮 start（取消后 join 完成才发新请求）。
        let events = executor.events();
        assert_eq!(
            events,
            vec!["start:r-1", "finish:r-1", "start:r-2"],
            "旧轮必须被取消并 join 完成后才 spawn 新轮"
        );
        assert_eq!(
            executor.active.load(Ordering::SeqCst),
            1,
            "任意时刻每来源至多一条活跃连接"
        );
        assert_eq!(service.active_connections(), 1);
    }

    #[tokio::test]
    async fn cross_sources_isolated() {
        let service = TranslationService::default();
        let executor = Arc::new(FakeExecutor::default());
        let _ = ok_of(
            service
                .start_translate_with_prompt(
                    TranslateSource::Input,
                    "p-input".to_string(),
                    &executor,
                )
                .await,
        );
        let _ = ok_of(
            service
                .start_translate_with_prompt(
                    TranslateSource::Selection,
                    "p-selection".to_string(),
                    &executor,
                )
                .await,
        );
        // 跨源互不影响：两个来源同时活跃（各一条连接）。
        assert_eq!(service.active_connections(), 2);
        assert_eq!(executor.active.load(Ordering::SeqCst), 2);

        // 取消 Input 轮 → Selection 轮不受影响。
        ok_of(service.cancel_translation("r-1").await);
        assert_eq!(service.active_round_id(TranslateSource::Input), None);
        assert_eq!(
            service
                .active_round_id(TranslateSource::Selection)
                .as_deref(),
            Some("r-2"),
            "跨源隔离：取消 Input 不得影响 Selection"
        );
        assert_eq!(executor.active.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cancel_translation_stops_round() {
        let service = TranslationService::default();
        let executor = Arc::new(FakeExecutor::default());
        let round = ok_of(
            service
                .start_translate_with_prompt(TranslateSource::Clipboard, "p".to_string(), &executor)
                .await,
        );
        ok_of(service.cancel_translation(&round.round_id).await);
        assert_eq!(
            service.active_round_id(TranslateSource::Clipboard),
            None,
            "取消后本轮应从会话表移除"
        );
        // 再次取消同一轮 → 已结束（幂等语义：返回错误由调用方按已结束处理）
        assert!(service.cancel_translation(&round.round_id).await.is_err());
        // 未知轮 id 同样报错
        assert!(service.cancel_translation("r-9999").await.is_err());
    }

    #[tokio::test]
    async fn join_old_timeout_is_bounded_when_old_task_ignores_cancel() {
        // 旧任务不响应取消（模拟连接未及时关闭）：新请求仍在 join 上限后发出。
        let service = TranslationService::new(Duration::from_millis(30));
        let executor = Arc::new(FakeExecutor {
            respond_to_cancel: false,
            ..Default::default()
        });
        let started = std::time::Instant::now();
        let first = ok_of(
            service
                .start_translate_with_prompt(TranslateSource::Input, "p1".to_string(), &executor)
                .await,
        );
        let second = ok_of(
            service
                .start_translate_with_prompt(TranslateSource::Input, "p2".to_string(), &executor)
                .await,
        );
        assert_ne!(first.round_id, second.round_id, "顶替必须产生新轮 id");
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_secs(5),
            "join 上限必须封顶（实际 {elapsed:?}）"
        );
        assert_eq!(
            service.active_round_id(TranslateSource::Input).as_deref(),
            Some(second.round_id.as_str()),
            "新轮必须已登记（旧任务未退出属 join 超时兜底场景）"
        );
    }

    #[tokio::test]
    async fn concurrent_same_source_never_exceeds_one_active_round() {
        let service = TranslationService::default();
        let executor = Arc::new(FakeExecutor::default());
        // 同源并发双发：串行门保证二者依次执行（cancel+join 后才轮到下一个），活跃数峰值恒为 1。
        let (a, b) = tokio::join!(
            service.start_translate_with_prompt(
                TranslateSource::Selection,
                "pa".to_string(),
                &executor
            ),
            service.start_translate_with_prompt(
                TranslateSource::Selection,
                "pb".to_string(),
                &executor
            )
        );
        let a = ok_of(a);
        let b = ok_of(b);
        assert_ne!(a.round_id, b.round_id);
        assert_eq!(
            executor.max_active.load(Ordering::SeqCst),
            1,
            "并发不变量：任意时刻每来源至多一条活跃连接"
        );
        assert_eq!(service.active_connections(), 1);
    }

    #[test]
    fn decide_render_mode_boundaries_999_1000_1001() {
        // FR-10.2 边界：≤1000 流式；>1000 整段。
        assert_eq!(decide_render_mode(999, 1000), RenderMode::Stream);
        assert_eq!(decide_render_mode(1000, 1000), RenderMode::Stream);
        assert_eq!(decide_render_mode(1001, 1000), RenderMode::Block);
        // 阈值不为默认值时同样适用
        assert_eq!(decide_render_mode(2000, 2000), RenderMode::Stream);
        assert_eq!(decide_render_mode(2001, 2000), RenderMode::Block);
        // 0 长度（空输入由上层 EX-9 拦截，本函数仍应定义行为：流式）
        assert_eq!(decide_render_mode(0, 1000), RenderMode::Stream);
    }

    #[test]
    fn count_chars_includes_whitespace_and_newlines_q10() {
        // Q10 口径：计数含全部字符（含空白与换行）。
        let text = format!("a{}b", "\n".repeat(997)); // 999 字符
        assert_eq!(count_chars(&text), 999);
        assert_eq!(
            decide_render_mode(count_chars(&text), 1000),
            RenderMode::Stream
        );
        let text = format!("a{}b", "\n".repeat(998)); // 1000 字符
        assert_eq!(count_chars(&text), 1000);
        assert_eq!(
            decide_render_mode(count_chars(&text), 1000),
            RenderMode::Stream
        );
        let text = format!("a{}b", "\n".repeat(999)); // 1001 字符
        assert_eq!(count_chars(&text), 1001);
        assert_eq!(
            decide_render_mode(count_chars(&text), 1000),
            RenderMode::Block
        );
        // 中文/emoji 等按 Unicode 字符计（scalar），非字节
        assert_eq!(count_chars("你好\n😀"), 4);
    }

    #[test]
    fn decide_presentation_stream_all_always_stream() {
        // 出厂默认「全部流式」：即使超过阈值也恒流式（FR-10.1 两档）。
        assert_eq!(
            decide_presentation(PresentationMode::StreamAll, 100_000, 1000),
            RenderMode::Stream
        );
        // 分档呈现才应用阈值。
        assert_eq!(
            decide_presentation(PresentationMode::Staged, 1000, 1000),
            RenderMode::Stream
        );
        assert_eq!(
            decide_presentation(PresentationMode::Staged, 1001, 1000),
            RenderMode::Block
        );
    }
}
