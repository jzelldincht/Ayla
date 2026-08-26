//! sse：SSE 流式客户端（T8 实现，spec 5.7.7；INV-2 网络出站入口之一）。
//!
//! 覆盖内容：
//! 1. **请求形态**：`POST {base}/chat/completions`，头 `Authorization: Bearer <key>`、
//!    `Content-Type: application/json`；体**仅** `{model, stream:true, messages:[{role:"user",content}]}`
//!    ——不附加 system 消息、不加温度等参数（spec 5.7.7 明示，业务基线提示词逐字生效）。
//! 2. **解析**：`bytes_stream` → `eventsource_stream` → 每个 `data:` JSON 取
//!    `choices[0].delta.content` 增量；`[DONE]` 终止；流体内出现 error 对象 → EX-14。
//! 3. **HTTP 分类**：401/403→EX-2、402→EX-3、5xx/其他→EX-14；连接类→EX-1（spec 5.8）。
//! 4. **两级超时（D1）**：响应头前 30s→EX-4；流中相邻 chunk 间隔 30s→EX-5（保留 partial）。
//! 5. **微批**（FR-10.4 / J2）：速率超约 60 事件/秒时合并 16ms 微批再推，保护 UI 线程。
//! 6. **取消链路**（A7）：CancellationToken 触发 + reqwest 流 drop 关闭 TCP——服务端感知断连。
//!
//! 日志纪律（INV-7）：本模块只记 round/字符数/错误归类，绝不记录原文/译文/Key/Authorization。

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::OnceLock;
use std::time::Duration;

use eventsource_stream::{Event, EventStreamError, Eventsource};
use futures_util::stream::Stream;
use futures_util::StreamExt;
use serde_json::Value;
use tauri::Emitter;
// 取消令牌：tokio 1.52+ 已迁至 tokio-util（tokio::sync::CancellationToken 在 1.53.1 已移除，
// 见 Cargo.toml 同款注释——spec 5.7.7 会话模型取用 tokio_util::sync::CancellationToken）。
use tokio_util::sync::CancellationToken;

use crate::error::AppError;
use crate::events::{
    TranslationBeginPayload, TranslationChunkPayload, TranslationDonePayload,
    TranslationErrorPayload, EVENT_TRANSLATION_BEGIN, EVENT_TRANSLATION_CHUNK,
    EVENT_TRANSLATION_DONE, EVENT_TRANSLATION_ERROR, WINDOW_TRANSLATE_LABEL,
};

/// 第一级超时（D1：连接建立 + 发出请求 + 收到响应头；30s 固定，Q9 不开放设置）。
pub const DEFAULT_FIRST_BYTE_TIMEOUT: Duration = Duration::from_secs(30);
/// 第二级超时（D1：流中相邻数据块间隔；30s）。
pub const DEFAULT_CHUNK_GAP_TIMEOUT: Duration = Duration::from_secs(30);
/// 微批：速率窗口（1s）。
pub const DEFAULT_BATCH_WINDOW: Duration = Duration::from_secs(1);
/// 微批：速率阈值（事件/秒，约 60——spec 5.7.7「速率超约 60 事件/秒」）。
pub const DEFAULT_BATCH_RATE: usize = 60;
/// 微批：合并推送间隔（16ms，J2 微批上限）。
pub const DEFAULT_BATCH_INTERVAL: Duration = Duration::from_millis(16);

// ---------------------------------------------------------------------------
// 类型与配置
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub first_byte: Duration,
    pub chunk_gap: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Timeouts {
            first_byte: DEFAULT_FIRST_BYTE_TIMEOUT,
            chunk_gap: DEFAULT_CHUNK_GAP_TIMEOUT,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BatchOptions {
    pub window: Duration,
    pub rate: usize,
    pub interval: Duration,
}

impl Default for BatchOptions {
    fn default() -> Self {
        BatchOptions {
            window: DEFAULT_BATCH_WINDOW,
            rate: DEFAULT_BATCH_RATE,
            interval: DEFAULT_BATCH_INTERVAL,
        }
    }
}

/// 流式请求参数（host/模型/Key；Key 仅内存态，INV-1）。
#[derive(Debug)]
pub struct ChatRequest<'a> {
    pub base_url: &'a str,
    pub model: &'a str,
    pub api_key: &'a str,
}

/// 流式结果：
/// - `Completed`：正常完成（含 [DONE] 终止）；
/// - `Cancelled`：token 取消（EX-13 顶替/显式取消——上层不呈现错误，本轮已被替换）；
/// - `Failed`：分类错误（EX-1/2/3/4/5/14），`partial`＝EX-5 保留的已收增量。
#[derive(Debug)]
pub enum StreamOutcome {
    Completed { text: String },
    Cancelled,
    Failed { error: AppError, partial: String },
}

/// 共享 HTTP 客户端（TLS/连接池复用；两条网络出口共用，INV-2）。
static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();

pub fn client() -> Result<&'static reqwest::Client, AppError> {
    let cached = CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .build()
            .map_err(|err| err.to_string())
    });
    match cached {
        Ok(client) => Ok(client),
        Err(err) => Err(AppError::Config(format!("HTTP 客户端创建失败: {err}"))),
    }
}

// ---------------------------------------------------------------------------
// 纯函数分类器（spec 5.8 映射；随任务单测）
// ---------------------------------------------------------------------------

/// HTTP 状态码 → EX 分类：`None`＝2xx 成功；401/403→EX-2；402→EX-3；其余非 2xx→EX-14。
pub fn classify_status(status: u16) -> Option<AppError> {
    match status {
        200..=299 => None,
        401 | 403 => Some(AppError::Auth(format!("HTTP {status}"))),
        402 => Some(AppError::Quota(format!("HTTP {status}"))),
        _ => Some(AppError::ServerResponse(format!("HTTP {status}"))),
    }
}

/// 单个 SSE `data:` 帧的解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataStep {
    /// 增量文本（`choices[0].delta.content`）。
    Delta(String),
    /// 流终止标记（`[DONE]`）。
    Done,
    /// 流内异常（顶层 `error` 对象或数据非 JSON）→ EX-14。
    Error(String),
    /// 无增量（空 delta/role 首块/choices 缺失）——静默跳过。
    Skip,
}

/// 解析一个数据帧（纯函数：分包/粘包由 eventsource-stream 完成，本函数只做语义判定）。
pub fn step_data(data: &str) -> DataStep {
    if data == "[DONE]" {
        return DataStep::Done;
    }
    match serde_json::from_str::<Value>(data) {
        Ok(value) => {
            if let Some(error) = value.get("error") {
                if !error.is_null() {
                    return DataStep::Error(error.to_string());
                }
            }
            let content = value
                .get("choices")
                .and_then(|choices| choices.as_array())
                .and_then(|array| array.first())
                .and_then(|choice| choice.get("delta"))
                .and_then(|delta| delta.get("content"));
            match content {
                Some(content) => match content.as_str() {
                    Some(content) => DataStep::Delta(content.to_string()),
                    // content 键存在但非字符串（异常形态）：跳过不误报（不影响流生命）。
                    None => DataStep::Skip,
                },
                // 空 delta（role 首块等）与缺失 choices：协议容忍形态，跳过。
                None => DataStep::Skip,
            }
        }
        Err(_) => DataStep::Error("数据事件非 JSON".to_string()),
    }
}

/// SSE 流层错误分类（spec 5.8）：
/// - `Transport`（底层流错误）→ EX-5（流 Err＝传输中断，保留 partial）；
/// - `Parser`/`Utf8`（事件帧/编码损坏）→ EX-14（服务端返回无效响应）。
pub fn classify_stream_error<E: std::fmt::Display>(err: &EventStreamError<E>) -> AppError {
    match err {
        EventStreamError::Transport(inner) => {
            AppError::StreamIdle(format!("传输连接中断: {inner}"))
        }
        EventStreamError::Parser(_) | EventStreamError::Utf8(_) => {
            AppError::ServerResponse("SSE 流格式异常（事件帧解析失败或非法 UTF-8）".to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// 微批（FR-10.4/J2：速率超阈值时 16ms 合并推送）
// ---------------------------------------------------------------------------

/// 微批器：1s 滑动窗口内事件数 > `rate` 判定为「高刷」→ 合并入 pending，由定时 flush 推送；
/// 速率回落后立即冲刷积压并恢复即时推送（保留近期增量语义，不丢块）。
struct RateBatcher {
    opts: BatchOptions,
    timestamps: VecDeque<tokio::time::Instant>,
    pending: String,
    batch_mode: bool,
}

impl RateBatcher {
    fn new(opts: BatchOptions) -> Self {
        RateBatcher {
            opts,
            timestamps: VecDeque::new(),
            pending: String::new(),
            batch_mode: false,
        }
    }

    /// 记录一个增量并决定推送策略（即时/合并）。
    fn push(&mut self, delta: &str, on_delta: &mut dyn FnMut(String)) {
        let now = tokio::time::Instant::now();
        self.timestamps.push_back(now);
        while let Some(first) = self.timestamps.front() {
            match now.checked_duration_since(*first) {
                Some(elapsed) if elapsed > self.opts.window => {
                    self.timestamps.pop_front();
                }
                _ => break,
            }
        }
        let rate = self.timestamps.len();
        if rate > self.opts.rate {
            self.batch_mode = true;
            self.pending.push_str(delta);
        } else if self.batch_mode {
            // 速率回落：先冲刷积压再恢复即时推送。
            self.batch_mode = false;
            self.flush(on_delta);
            on_delta(delta.to_string());
        } else {
            on_delta(delta.to_string());
        }
    }

    fn flush(&mut self, on_delta: &mut dyn FnMut(String)) {
        if !self.pending.is_empty() {
            on_delta(std::mem::take(&mut self.pending));
        }
    }

    fn should_flush(&self) -> bool {
        self.batch_mode && !self.pending.is_empty()
    }
}

// ---------------------------------------------------------------------------
// 核心泵（事件驱动：取消 / 微批定时 / 流读取 三路 select）
// ---------------------------------------------------------------------------

/// 消费 SSE 事件流直至完成/取消/失败（解析 + 两级超时第二级 + 微批 + 取消，spec 5.7.7）。
/// `stream` 以 `Pin<&mut S>` 传入，兼容 eventsource-stream 的 `EventStream`（可 pin 可回环）。
pub async fn pump_events<S, E, F>(
    stream: Pin<&mut S>,
    opts: BatchOptions,
    gap_timeout: Duration,
    cancel: &CancellationToken,
    on_delta: &mut F,
) -> StreamOutcome
where
    S: Stream<Item = Result<Event, EventStreamError<E>>>,
    E: std::fmt::Display,
    F: FnMut(String),
{
    let mut stream = stream;
    let mut batcher = RateBatcher::new(opts);
    let mut full_text = String::new();
    let mut next_flush: Option<tokio::time::Instant> = None;

    loop {
        let flush_future = match next_flush {
            Some(instant) => futures_util::future::Either::Left(tokio::time::sleep_until(instant)),
            None => futures_util::future::Either::Right(std::future::pending()),
        };

        tokio::select! {
            _ = cancel.cancelled() => {
                return StreamOutcome::Cancelled;
            }
            _ = flush_future => {
                // 微批定时：合并推送（仅 batch_mode 且有积压时会 arm，见下方事件分支）。
                batcher.flush(on_delta);
                next_flush = None;
            }
            next = tokio::time::timeout(gap_timeout, stream.next()) => {
                match next {
                    Err(_elapsed) => {
                        // D1 第二级：相邻 chunk 间隔超时 → EX-5，保留 partial。
                        batcher.flush(on_delta);
                        return StreamOutcome::Failed {
                            error: AppError::StreamIdle(format!(
                                "相邻数据块间隔超过 {gap_timeout:?}"
                            )),
                            partial: full_text,
                        };
                    }
                    Ok(None) => {
                        // EOF 但未见 [DONE]：连接中断，不伪装为完整结果（EX-5 口径）。
                        batcher.flush(on_delta);
                        return StreamOutcome::Failed {
                            error: AppError::StreamIdle("连接已关闭但未收到 [DONE] 终止标记".to_string()),
                            partial: full_text,
                        };
                    }
                    Ok(Some(Err(err))) => {
                        let error = classify_stream_error(&err);
                        let partial = if matches!(error, AppError::StreamIdle(_)) {
                            full_text.clone()
                        } else {
                            String::new()
                        };
                        batcher.flush(on_delta);
                        return StreamOutcome::Failed { error, partial };
                    }
                    Ok(Some(Ok(event))) => {
                        match step_data(&event.data) {
                            DataStep::Delta(delta) => {
                                full_text.push_str(&delta);
                                batcher.push(&delta, on_delta);
                                if batcher.should_flush() && next_flush.is_none() {
                                    next_flush =
                                        Some(tokio::time::Instant::now() + opts.interval);
                                }
                            }
                            DataStep::Done => {
                                batcher.flush(on_delta);
                                return StreamOutcome::Completed { text: full_text };
                            }
                            DataStep::Error(message) => {
                                // 流内 error 对象 / 非 JSON 数据 → EX-14（无 partial 承诺）。
                                return StreamOutcome::Failed {
                                    error: AppError::ServerResponse(message),
                                    partial: String::new(),
                                };
                            }
                            DataStep::Skip => {}
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 流式请求（发送 → 第一级超时 → 状态分类 → 事件泵）
// ---------------------------------------------------------------------------

/// 发起翻译流式请求（spec 5.7.7 请求形态；INV-2 网络出口）。
/// 第一级超时在「响应头前」（D1）；取消经 token 立即中断（reqwest 请求 future 被 drop）。
pub async fn stream_chat<F: FnMut(String)>(
    request: &ChatRequest<'_>,
    content: String,
    cancel: CancellationToken,
    timeouts: Timeouts,
    on_delta: &mut F,
) -> StreamOutcome {
    let client = match client() {
        Ok(client) => client,
        Err(err) => {
            return StreamOutcome::Failed {
                error: err,
                partial: String::new(),
            };
        }
    };

    // 请求形态：仅 {model, stream:true, messages:[user]}（spec 5.7.7——不加任何其他参数）。
    let url = format!(
        "{}/chat/completions",
        request.base_url.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "model": request.model,
        "stream": true,
        "messages": [ { "role": "user", "content": content } ],
    });
    let built = match client
        .post(&url)
        .bearer_auth(request.api_key)
        .json(&body)
        .build()
    {
        Ok(built) => built,
        Err(err) => {
            return StreamOutcome::Failed {
                error: AppError::Config(format!("翻译请求构造失败（检查接口地址）: {err}")),
                partial: String::new(),
            };
        }
    };

    // 第一级超时（D1）：连接 + 发请求 + 响应头；取消时 request future 被 drop → TCP 关闭（A7）。
    let response = match tokio::select! {
        result = tokio::time::timeout(timeouts.first_byte, client.execute(built)) => result,
        _ = cancel.cancelled() => return StreamOutcome::Cancelled,
    } {
        Ok(Ok(response)) => response,
        Ok(Err(err)) => {
            return StreamOutcome::Failed {
                // 连接类（DNS/TCP/TLS）→ EX-1（spec 5.8）。
                error: AppError::Network(format!("{err}")),
                partial: String::new(),
            };
        }
        Err(_elapsed) => {
            return StreamOutcome::Failed {
                error: AppError::TimeoutFirstByte(format!(
                    "{:?} 内未收到响应头",
                    timeouts.first_byte
                )),
                partial: String::new(),
            };
        }
    };

    if let Some(err) = classify_status(response.status().as_u16()) {
        return StreamOutcome::Failed {
            error: err,
            partial: String::new(),
        };
    }

    // 事件泵（解析/Ex-5 超时/微批/取消；第二级超时在 pump 内）。
    let event_stream = response.bytes_stream().eventsource();
    let mut pinned = std::pin::pin!(event_stream);
    pump_events(
        pinned.as_mut(),
        BatchOptions::default(),
        timeouts.chunk_gap,
        &cancel,
        on_delta,
    )
    .await
}

// ---------------------------------------------------------------------------
// 生产执行器（service::TranslateExecutor 实现；事件经 events.rs payload emit 到 translate 窗）
// ---------------------------------------------------------------------------

/// 生产翻译执行器：读 config → 定 renderMode → emit begin → 流式消费（chunk 推送）→ done/error。
/// 构造/接线（T16）：`SseTranslateExecutor::new(app_handle)` + `service::start_translate`。
pub struct SseTranslateExecutor {
    app: tauri::AppHandle,
}

impl SseTranslateExecutor {
    pub fn new(app: tauri::AppHandle) -> Self {
        SseTranslateExecutor { app }
    }
}

/// 执行环境快照（config 读锁在构造时即释放——先 config 后 sessions 锁序）。
struct ExecuteEnv {
    base_url: String,
    model: String,
    api_key: String,
    render_mode: crate::events::RenderMode,
}

impl ExecuteEnv {
    fn resolve(request_text: &str) -> Result<Self, AppError> {
        let (base_url, model, api_key, presentation, threshold) = {
            let settings = crate::config::read_global()?;
            let api_key = match &settings.ai.api_key_cipher {
                Some(cipher) => crate::config::crypto::unprotect_str(cipher).map_err(|err| {
                    AppError::Config(format!("API Key 无法解出（视同未配置）: {}", err.class()))
                })?,
                None => return Err(AppError::Config("未配置 API Key".to_string())),
            };
            (
                settings.ai.base_url.clone(),
                settings.ai.model.clone(),
                api_key,
                settings.translation.presentation,
                settings.translation.long_text_threshold_chars,
            )
            // config 读锁在块尾释放（克隆副本返回，不跨 await 持锁）。
        };
        let render_mode = crate::translate::service::decide_presentation(
            presentation,
            crate::translate::service::count_chars(request_text),
            threshold,
        );
        Ok(ExecuteEnv {
            base_url,
            model,
            api_key,
            render_mode,
        })
    }
}

impl crate::translate::service::TranslateExecutor for SseTranslateExecutor {
    fn spawn(
        &self,
        round_id: String,
        request_text: String,
        cancel: CancellationToken,
    ) -> tauri::async_runtime::JoinHandle<()> {
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            // 1) 执行环境（config 读锁不跨 await；Key 仅内存态 INV-1；错误经 EX 呈现）。
            let env = match ExecuteEnv::resolve(&request_text) {
                Ok(env) => env,
                Err(err) => {
                    let payload = TranslationErrorPayload::from_error(round_id.clone(), &err, None);
                    if let Err(emit_err) =
                        app.emit_to(WINDOW_TRANSLATE_LABEL, EVENT_TRANSLATION_ERROR, payload)
                    {
                        log::error!("translate: 环境错误推送失败（round={round_id}）: {emit_err}");
                    }
                    return;
                }
            };

            // 2) begin（C1：renderMode 由 Rust 分档判定，前端按此消费）。
            let begin = TranslationBeginPayload {
                round_id: round_id.clone(),
                render_mode: env.render_mode,
            };
            if let Err(err) = app.emit_to(WINDOW_TRANSLATE_LABEL, EVENT_TRANSLATION_BEGIN, begin) {
                log::error!("translate: begin 推送失败（round={round_id}）: {err}");
            }

            // 3) 流式消费（chunk 微批推送；取消=token + 流 drop 断连）。
            let request = ChatRequest {
                base_url: &env.base_url,
                model: &env.model,
                api_key: &env.api_key,
            };
            let outcome = stream_chat(
                &request,
                request_text,
                cancel,
                Timeouts::default(),
                &mut |delta| {
                    let chunk = TranslationChunkPayload {
                        round_id: round_id.clone(),
                        delta,
                    };
                    if let Err(err) =
                        app.emit_to(WINDOW_TRANSLATE_LABEL, EVENT_TRANSLATION_CHUNK, chunk)
                    {
                        log::error!("translate: chunk 推送失败（round={round_id}）: {err}");
                    }
                },
            )
            .await;

            // 4) 终态：done / 取消（静默，本轮已被顶替或显式取消）/ error（EX-5 带 partial）。
            match outcome {
                StreamOutcome::Completed { .. } => {
                    let done = TranslationDonePayload {
                        round_id: round_id.clone(),
                    };
                    if let Err(err) =
                        app.emit_to(WINDOW_TRANSLATE_LABEL, EVENT_TRANSLATION_DONE, done)
                    {
                        log::error!("translate: done 推送失败（round={round_id}）: {err}");
                    }
                }
                StreamOutcome::Cancelled => {
                    log::debug!("translate: 轮已取消（round={round_id}，被顶替或显式取消）");
                }
                StreamOutcome::Failed { error, partial } => {
                    let partial = if matches!(error, AppError::StreamIdle(_)) {
                        Some(partial)
                    } else {
                        None
                    };
                    let payload =
                        TranslationErrorPayload::from_error(round_id.clone(), &error, partial);
                    if let Err(err) =
                        app.emit_to(WINDOW_TRANSLATE_LABEL, EVENT_TRANSLATION_ERROR, payload)
                    {
                        log::error!("translate: error 推送失败（round={round_id}）: {err}");
                    }
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// 单元测试：解析纯逻辑（分包粘包/[DONE]/error/空 delta）+ 微批 + EX-5 partial 保留
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use futures_util::stream;

    fn delta_frame(content: &str) -> String {
        format!("data: {content}\n\n")
    }

    fn delta_json(chunk: &str) -> String {
        delta_frame(
            &serde_json::to_string(&serde_json::json!({
                "choices": [ { "delta": { "content": chunk } } ]
            }))
            .unwrap_or_else(|_| "{}".to_string()),
        )
    }

    /// 分/时延迟的合成字节流（paused 时钟下确定性测试）。
    fn delayed_chunks(
        pairs: Vec<(Duration, Vec<u8>)>,
    ) -> impl Stream<Item = Result<Vec<u8>, std::io::Error>> + Send {
        stream::iter(pairs).then(|(delay, bytes)| async move {
            tokio::time::sleep(delay).await;
            Ok::<Vec<u8>, std::io::Error>(bytes)
        })
    }

    async fn run_pump(
        pairs: Vec<(Duration, Vec<u8>)>,
        opts: BatchOptions,
        gap: Duration,
    ) -> (StreamOutcome, Vec<String>) {
        let event_stream = delayed_chunks(pairs).eventsource();
        let mut pinned = std::pin::pin!(event_stream);
        let mut pushes = Vec::new();
        let outcome = pump_events(
            pinned.as_mut(),
            opts,
            gap,
            &CancellationToken::new(),
            &mut |delta| pushes.push(delta),
        )
        .await;
        (outcome, pushes)
    }

    // ---------- 纯函数 ----------

    #[test]
    fn step_data_classifies_done_delta_skip_and_error() {
        assert_eq!(step_data("[DONE]"), DataStep::Done);
        assert_eq!(
            step_data(r#"{"choices":[{"delta":{"content":"你好"}}]}"#),
            DataStep::Delta("你好".to_string())
        );
        // 空 delta（role-only 首块）→ Skip
        assert_eq!(
            step_data(r#"{"choices":[{"delta":{"role":"assistant"}}]}"#),
            DataStep::Skip
        );
        // choices 缺失/空数组 → Skip（协议容忍）
        assert_eq!(step_data(r#"{"choices":[]}"#), DataStep::Skip);
        assert_eq!(step_data(r#"{"foo":1}"#), DataStep::Skip);
        // content 非字符串 → Skip（不误报中断）
        assert_eq!(
            step_data(r#"{"choices":[{"delta":{"content":null}}]}"#),
            DataStep::Skip
        );
        // 流内 error 对象 → Error（EX-14 分类源）
        let step = step_data(r#"{"error":{"message":"rate limit"}}"#);
        match step {
            DataStep::Error(_) => {}
            other => panic!("error 对象应归为 Error，实际 {other:?}"),
        }
        // 数据非 JSON → Error
        let step = step_data("hello world");
        match step {
            DataStep::Error(msg) => assert!(msg.contains("非 JSON")),
            other => panic!("非 JSON 应归为 Error，实际 {other:?}"),
        }
    }

    #[test]
    fn classify_status_maps_ex_codes() {
        assert!(classify_status(200).is_none());
        assert!(classify_status(204).is_none());
        for (status, expected) in [
            (401u16, "EX-2"),
            (403, "EX-2"),
            (402, "EX-3"),
            (500, "EX-14"),
            (503, "EX-14"),
            (418, "EX-14"),
            (429, "EX-14"),
        ] {
            let err = classify_status(status).unwrap_or_else(|| panic!("HTTP {status} 应被分类"));
            assert_eq!(err.ex_code(), expected, "HTTP {status} 分类");
        }
    }

    #[test]
    fn classify_stream_error_maps_transport_and_utf8() {
        let transport: Result<Event, EventStreamError<std::io::Error>> =
            Err(EventStreamError::Transport(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "reset",
            )));
        let err = match transport {
            Err(err) => classify_stream_error(&err),
            Ok(_) => panic!("构造错误样本失败"),
        };
        assert_eq!(
            err.ex_code(),
            "EX-5",
            "Transport 流错误→EX-5（保留 partial）"
        );

        // Parser 变体（NomError）非 crate 公共类型，无法外部构造——此处以
        // Utf8 变体（可构造）覆盖「帧/编码异常→EX-14」同一分支。
        let utf8_err = match String::from_utf8(vec![0xff, 0xfe]) {
            Err(err) => err,
            Ok(_) => panic!("构造非法 UTF-8 失败"),
        };
        let utf8: Result<Event, EventStreamError<std::io::Error>> =
            Err(EventStreamError::Utf8(utf8_err));
        let err = match utf8 {
            Err(err) => classify_stream_error(&err),
            Ok(_) => panic!("构造错误样本失败"),
        };
        assert_eq!(err.ex_code(), "EX-14", "Utf8 帧损坏→EX-14");
    }

    // ---------- 事件泵：解析/超时/微批（paused 时钟，无真实等待） ----------

    #[tokio::test(start_paused = true)]
    async fn pump_fragmented_chunks_merge_and_done() {
        // 分包：一个 data 帧的 UTF-8 字节被拆成两段（eventsource 重组）；随后 [DONE] 终止。
        let frame = delta_json("你好");
        let bytes = frame.as_bytes().to_vec();
        let mid = 5; // 可能落在多字节字符中间（UTF-8 流缓冲兜底）
        let (head, tail) = bytes.split_at(mid);
        let outcome = run_pump(
            vec![
                (Duration::ZERO, head.to_vec()),
                (Duration::ZERO, tail.to_vec()),
                (Duration::ZERO, b"data: [DONE]\n\n".to_vec()),
            ],
            BatchOptions::default(),
            Duration::from_secs(10),
        )
        .await;
        match outcome.0 {
            StreamOutcome::Completed { text } => assert_eq!(text, "你好"),
            other => panic!("应正常完成，实际 {other:?}"),
        }
        assert_eq!(outcome.1, vec!["你好".to_string()]);
    }

    #[tokio::test(start_paused = true)]
    async fn pump_multiple_events_same_write_and_skip_empty_deltas() {
        // 粘包：同一字节块含两个 data 事件；空 delta 块穿插在中间。
        let body = format!(
            "{}{}{}",
            delta_json("A"),
            delta_frame(r#"{"choices":[{"delta":{"role":"assistant"}}]}"#),
            delta_json("B"),
        );
        let mut appended = body.into_bytes();
        appended.extend_from_slice(b"data: [DONE]\n\n");
        let outcome = run_pump(
            vec![(Duration::ZERO, appended)],
            BatchOptions::default(),
            Duration::from_secs(10),
        )
        .await;
        match outcome.0 {
            StreamOutcome::Completed { text } => assert_eq!(text, "AB"),
            other => panic!("应正常完成，实际 {other:?}"),
        }
        assert_eq!(outcome.1, vec!["A".to_string(), "B".to_string()]);
    }

    #[tokio::test(start_paused = true)]
    async fn pump_stream_error_event_fails_ex14() {
        let frame = delta_frame(r#"{"error":{"message":"internal"}}"#);
        let outcome = run_pump(
            vec![(Duration::ZERO, frame.into_bytes())],
            BatchOptions::default(),
            Duration::from_secs(10),
        )
        .await;
        match outcome.0 {
            StreamOutcome::Failed { error, partial } => {
                assert_eq!(error.ex_code(), "EX-14");
                assert!(partial.is_empty(), "EX-14 无 partial 承诺");
                assert!(!error.to_string().is_empty());
            }
            other => panic!("流内 error 应归为 EX-14，实际 {other:?}"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn pump_gap_timeout_keeps_partial_ex5() {
        // 第一块立即到达，第二块超过 chunk-gap 阈值 → EX-5 且 partial 保留已收增量。
        let first = delta_json("部分译文");
        let second = delta_json("后段");
        let outcome = run_pump(
            vec![
                (Duration::ZERO, first.into_bytes()),
                (Duration::from_millis(300), second.into_bytes()),
            ],
            BatchOptions::default(),
            Duration::from_millis(80),
        )
        .await;
        match outcome.0 {
            StreamOutcome::Failed { error, partial } => {
                assert_eq!(error.ex_code(), "EX-5");
                assert_eq!(partial, "部分译文", "EX-5 必须保留 partial");
            }
            other => panic!("超时应归为 EX-5，实际 {other:?}"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn pump_eof_without_done_is_ex5_not_success() {
        // EOF 但无 [DONE]：不伪装为完整结果（EX-5 口径）。
        let frame = delta_json("只有一段");
        let outcome = run_pump(
            vec![(Duration::ZERO, frame.into_bytes())],
            BatchOptions::default(),
            Duration::from_secs(10),
        )
        .await;
        match outcome.0 {
            StreamOutcome::Failed { error, partial } => {
                assert_eq!(error.ex_code(), "EX-5");
                assert_eq!(partial, "只有一段");
            }
            other => panic!("EOF 无 [DONE] 应归为 EX-5，实际 {other:?}"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn pump_cancel_returns_cancelled() {
        let cancel = CancellationToken::new();
        let stream = delayed_chunks(vec![(Duration::from_secs(5), delta_json("x").into_bytes())])
            .eventsource();
        let mut pinned = std::pin::pin!(stream);
        let mut pushes = Vec::new();
        cancel.cancel(); // 取消发生在任何事件之前
        let outcome = pump_events(
            pinned.as_mut(),
            BatchOptions::default(),
            Duration::from_secs(10),
            &cancel,
            &mut |delta| pushes.push(delta),
        )
        .await;
        assert!(matches!(outcome, StreamOutcome::Cancelled));
        assert!(pushes.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn pump_fast_rate_batches_into_16ms_micro_batch() {
        // 100 事件瞬间到达（速率远超 60/秒并落入同一窗口）→ 即时推送 ≤ 速率阈值 + 合并批。
        let opts = BatchOptions {
            window: Duration::from_millis(500),
            rate: 3,
            interval: Duration::from_millis(16),
        };
        let mut frames = Vec::new();
        for i in 0..10 {
            frames.push((Duration::ZERO, delta_json(&i.to_string()).into_bytes()));
        }
        frames.push((Duration::ZERO, b"data: [DONE]\n\n".to_vec()));
        let outcome = run_pump(frames, opts, Duration::from_secs(10)).await;
        match &outcome.0 {
            StreamOutcome::Completed { text } => assert_eq!(text, "0123456789"),
            other => panic!("应正常完成，实际 {other:?}"),
        }
        // 3 即时 + 1 合并批（剩余 7 合并为一推）＝4 次推送，远少于 10 次推送。
        assert_eq!(
            outcome.1.len(),
            4,
            "高刷速率必须合并微批（实际 {outcome:?}）"
        );
        assert_eq!(outcome.1[0], "0");
        assert_eq!(outcome.1[1], "1");
        assert_eq!(outcome.1[2], "2");
        assert_eq!(outcome.1[3], "3456789", "批内顺序保持且内容完整");
    }

    #[tokio::test(start_paused = true)]
    async fn pump_timer_flushes_batch_mid_stream() {
        // 高速突发 → 16ms 定时器在流进行中冲刷一次；速率回落后的尾事件即时推送。
        let opts = BatchOptions {
            window: Duration::from_millis(500),
            rate: 3,
            interval: Duration::from_millis(16),
        };
        let mut frames = Vec::new();
        for i in 0..8 {
            frames.push((Duration::ZERO, delta_json(&i.to_string()).into_bytes()));
        }
        // 尾事件在 200ms 后到达（仍在 500ms 窗口内）：到达前定时冲刷必须已发生。
        frames.push((Duration::from_millis(200), delta_json("T").into_bytes()));
        frames.push((Duration::ZERO, b"data: [DONE]\n\n".to_vec()));
        let outcome = run_pump(frames, opts, Duration::from_secs(10)).await;
        match outcome.0 {
            StreamOutcome::Completed { text } => assert_eq!(text, "01234567T"),
            other => panic!("应正常完成，实际 {other:?}"),
        }
        assert_eq!(
            outcome.1,
            vec![
                "0".to_string(),
                "1".to_string(),
                "2".to_string(),
                "34567".to_string(),
                "T".to_string()
            ],
            "定时冲刷（16ms）必须在流中发生且不丢块"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn pump_slow_rate_streams_immediately() {
        // 低频（速率低于阈值）：即时逐块推送，不合并。
        let opts = BatchOptions {
            window: Duration::from_millis(500),
            rate: 3,
            interval: Duration::from_millis(16),
        };
        let mut frames = Vec::new();
        for (i, delay) in [(0, Duration::ZERO), (1, Duration::from_millis(50))] {
            frames.push((delay, delta_json(&i.to_string()).into_bytes()));
        }
        frames.push((Duration::ZERO, b"data: [DONE]\n\n".to_vec()));
        let outcome = run_pump(frames, opts, Duration::from_secs(10)).await;
        match outcome.0 {
            StreamOutcome::Completed { .. } => {}
            other => panic!("应正常完成，实际 {other:?}"),
        }
        assert_eq!(
            outcome.1,
            vec!["0".to_string(), "1".to_string()],
            "低频应即时逐块推送"
        );
    }
}
