//! sse_integration：SSE 流式客户端与连接验证的集成测试（T8，spec 验收标准 2）。
//!
//! 覆盖（tasks.md T8 Verification）：
//! - EX-1/2/3/4/5/14 全分支（mock/夹具驱动的真实 HTTP 往返，非纯函数单测）；
//! - 分包/粘包/[DONE]/流内 error（eventsource 经真实字节流重组）；
//! - EX-5 保留 partial（流中相邻 chunk 间隔超时）；
//! - 取消后服务端活跃连接数归零（A7 幽灵连接防护）；
//! - 重试不叠加请求（失败后新请求恰好一条）；
//! - `verify_connection` 两形态（D6：GET /models 直通；404/405 回退 max_tokens=1 最小 chat）。
//!
//! 夹具说明：
//! - `wiremock` 承担绝大多数形态（状态码/延时响应/请求体契约校验）；
//! - 尾段 EX-5（流中停顿）与取消收敛需要「真正按时间顺序写入字节、可观测连接生命周期」的
//!   服务端——wiremock 0.6.5 无 set_body_stream，故用向本地端口写入 HTTP/1.1 响应的原始 TCP
//!   夹具补足（仅测试代码存在，不属生产网络出入口，不违反 INV-2）。
//!
//! 凭据纪律（G5/K3）：全部测试使用哑值 `sk-test-dummy`；无任何真实凭据。

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use alya::events::VerifyConnectionCommand;
use alya::translate::sse::{self, ChatRequest, StreamOutcome, Timeouts};
use alya::translate::verify::verify_connection;
use tokio_util::sync::CancellationToken;
use wiremock::http::Method;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 测试哑 Key（INV-1/G5：无真实凭据）。
const TEST_KEY: &str = "sk-test-dummy";
/// 测试模型名（无真实凭据）。
const TEST_MODEL: &str = "deepseek-chat";

/// 无 unwrap/expect 的取值辅助（lint 门禁覆盖集成测试目标；panic 仅用于断言失败）。
fn ok_of<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(err) => panic!("期望成功，实际错误: {err}"),
    }
}

// ---------------------------------------------------------------------------
// 原始 TCP SSE 夹具（批量写字节 + 连接生命周期计量）
// ---------------------------------------------------------------------------

/// 按时间顺序向客户端写 HTTP/1.1 响应的原始服务端：
/// - `active`：当前尚未收到客户端断开（EOF）的连接数；
/// - `req_seen`：已读完请求头的连接数（`wait_request` 轮询此计数）；
/// - 脚本发完后持续等待客户端关闭（取消/超时 → reqwest drop 流 → EOF），据此计量收敛。
struct RawSseServer {
    addr: SocketAddr,
    accepted: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    req_seen: Arc<Mutex<usize>>,
    shutdown: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl RawSseServer {
    fn start(script: Vec<(Duration, Vec<u8>)>) -> RawSseServer {
        let listener = match TcpListener::bind(("127.0.0.1", 0)) {
            Ok(listener) => listener,
            Err(err) => panic!("测试夹具绑定端口失败: {err}"),
        };
        match listener.set_nonblocking(true) {
            Ok(()) => {}
            Err(err) => panic!("测试夹具设置非阻塞失败: {err}"),
        }
        let addr = match listener.local_addr() {
            Ok(addr) => addr,
            Err(err) => panic!("测试夹具读取端口失败: {err}"),
        };
        let accepted = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let req_seen = Arc::new(Mutex::new(0usize));
        let shutdown = Arc::new(AtomicBool::new(false));

        let accept_handle = {
            let accepted = accepted.clone();
            let active = active.clone();
            let req_seen = req_seen.clone();
            let shutdown = shutdown.clone();
            thread::spawn(move || {
                accept_loop(listener, script, accepted, active, req_seen, shutdown);
            })
        };

        RawSseServer {
            addr,
            accepted,
            active,
            req_seen,
            shutdown,
            handle: Some(accept_handle),
        }
    }

    fn uri(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn active(&self) -> usize {
        self.active.load(Ordering::SeqCst)
    }

    fn accepted(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }

    /// 等待首个连接完成请求头读取（最大 `timeout`）。
    /// 轮询式（每 25ms 让步）：不能在单线程 tokio runtime 上阻塞——否则等待期间
    /// 被 `tokio::spawn` 的客户端任务将永远得不到调度（测试自行验证过该陷阱）。
    async fn wait_request(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            let seen = match self.req_seen.lock() {
                Ok(guard) => *guard,
                Err(_) => return false,
            };
            if seen >= 1 {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

impl Drop for RawSseServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            // accept 循环在下次非阻塞 accept 轮询或等待中退出；join 不阻塞过久（循环带休眠）。
            let _ = handle.join();
        }
    }
}

fn accept_loop(
    listener: TcpListener,
    script: Vec<(Duration, Vec<u8>)>,
    accepted: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    req_seen: Arc<Mutex<usize>>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _addr)) => {
                let script = script.clone();
                let accepted = accepted.clone();
                let active = active.clone();
                let req_seen = req_seen.clone();
                thread::spawn(move || {
                    handle_connection(stream, script, &accepted, &active, &req_seen);
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break,
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    script: Vec<(Duration, Vec<u8>)>,
    accepted: &AtomicUsize,
    active: &AtomicUsize,
    req_seen: &Mutex<usize>,
) {
    accepted.fetch_add(1, Ordering::SeqCst);
    active.fetch_add(1, Ordering::SeqCst);
    // 本次会话仅处理一个连接：先读请求头再按脚本写响应。
    let _ = read_request_headers(&mut stream);
    if let Ok(mut seen) = req_seen.lock() {
        *seen += 1;
    }
    for (delay, bytes) in &script {
        thread::sleep(*delay);
        if stream.write_all(bytes).is_err() {
            break; // 客户端已断开（取消/超时），剩下的脚本无需再发。
        }
    }
    // 等待客户端关闭：取消链路 = reqwest 流 drop → TCP 关闭 → 服务端读到 EOF（A7 收敛计量）。
    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
    let mut buf = [0u8; 2048];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break, // EOF：客户端已关闭连接。
            Ok(_) => continue,
            Err(err) => match err.kind() {
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => continue,
                _ => break,
            },
        }
    }
    active.fetch_sub(1, Ordering::SeqCst);
}

fn read_request_headers(stream: &mut TcpStream) -> std::io::Result<()> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let mut buffered: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 512];
    loop {
        let n = match stream.read(&mut chunk) {
            Ok(n) => n,
            Err(err) if err.kind() == std::io::ErrorKind::TimedOut => return Ok(()),
            Err(err) => return Err(err),
        };
        if n == 0 {
            return Ok(());
        }
        buffered.extend_from_slice(&chunk[..n]);
        if buffered.windows(4).any(|w| w == b"\r\n\r\n") {
            return Ok(());
        }
    }
}

/// HTTP/1.1 200 OK + SSE 响应头（无 Content-Length：正文长度由连接关闭界定，事件流增量到达）。
fn sse_ok_headers() -> Vec<u8> {
    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n".to_vec()
}

/// 一个 delta 增量帧（`choices[0].delta.content`）。
fn delta_frame(delta: &str) -> Vec<u8> {
    let json = serde_json::json!({ "choices": [ { "delta": { "content": delta } } ] });
    format!("data: {json}\n\n").into_bytes()
}

/// `[DONE]` 终止帧。
fn done_frame() -> Vec<u8> {
    b"data: [DONE]\n\n".to_vec()
}

/// 发起一次流式请求（默认哑 Key）并收集增量推送。
async fn run_stream(base: &str, content: &str, timeouts: Timeouts) -> (StreamOutcome, Vec<String>) {
    let cancel = CancellationToken::new();
    let request = ChatRequest {
        base_url: base,
        model: TEST_MODEL,
        api_key: TEST_KEY,
    };
    let mut deltas: Vec<String> = Vec::new();
    let outcome = sse::stream_chat(
        &request,
        content.to_string(),
        cancel,
        timeouts,
        &mut |delta| deltas.push(delta),
    )
    .await;
    (outcome, deltas)
}

/// 短超时组合（EX-4/EX-5 分支需亚秒级验证窗口，不实际等待 30s）。
fn fast_timeouts() -> Timeouts {
    Timeouts {
        first_byte: Duration::from_secs(5),
        chunk_gap: Duration::from_millis(200),
    }
}

/// 轮询等待条件满足（取消收敛等异步事实）。
async fn wait_until(condition: impl Fn() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    condition()
}

/// 已接收请求中某路径的计数（重试不叠加断言用）。
fn count_requests(requests: &[wiremock::Request], url_part: &str, http_method: Method) -> usize {
    requests
        .iter()
        .filter(|request| request.method == http_method && request.url.as_str().contains(url_part))
        .count()
}

// ---------------------------------------------------------------------------
// 集成测试：EX 分类全分支
// ---------------------------------------------------------------------------

/// EX-1：连接类错误（端口无监听 → ECONNREFUSED）。
#[tokio::test]
async fn ex1_connection_error_maps_network() {
    let listener = match TcpListener::bind(("127.0.0.1", 0)) {
        Ok(listener) => listener,
        Err(err) => panic!("绑定临时端口失败: {err}"),
    };
    let addr = match listener.local_addr() {
        Ok(addr) => addr,
        Err(err) => panic!("读取临时端口失败: {err}"),
    };
    drop(listener); // 立即释放 → 连接被拒（确定性更高，不依赖端口 1 之类惯例）。

    let (outcome, deltas) =
        run_stream(&format!("http://{addr}"), "原文", Timeouts::default()).await;
    match outcome {
        StreamOutcome::Failed { error, partial } => {
            assert_eq!(error.ex_code(), "EX-1", "连接失败应归为 EX-1");
            assert!(partial.is_empty(), "EX-1 无 partial 承诺");
            assert!(deltas.is_empty());
        }
        other => panic!("应归为 EX-1，实际 {other:?}"),
    }
}

/// EX-2：HTTP 401/403 → 鉴权失败。
#[tokio::test]
async fn ex2_401_and_403_map_auth() {
    for status in [401u16, 403u16] {
        let server = MockServer::start().await;
        server
            .register(
                Mock::given(method("POST"))
                    .and(path("/chat/completions"))
                    .respond_with(ResponseTemplate::new(status)),
            )
            .await;
        let (outcome, deltas) = run_stream(&server.uri(), "原文", Timeouts::default()).await;
        match outcome {
            StreamOutcome::Failed { error, partial } => {
                assert_eq!(error.ex_code(), "EX-2", "HTTP {status} 应归为 EX-2");
                assert!(partial.is_empty());
                assert!(deltas.is_empty());
            }
            other => panic!("HTTP {status} 应归为 EX-2，实际 {other:?}"),
        }
    }
}

/// EX-3：HTTP 402 → 配额/余额不足。
#[tokio::test]
async fn ex3_http_402_maps_quota() {
    let server = MockServer::start().await;
    server
        .register(
            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .respond_with(ResponseTemplate::new(402)),
        )
        .await;
    let (outcome, deltas) = run_stream(&server.uri(), "原文", Timeouts::default()).await;
    match outcome {
        StreamOutcome::Failed { error, partial } => {
            assert_eq!(error.ex_code(), "EX-3", "402 应归为 EX-3（配额）");
            assert!(partial.is_empty());
            assert!(deltas.is_empty());
        }
        other => panic!("402 应归为 EX-3，实际 {other:?}"),
    }
}

/// EX-4：连接→响应头超时（D1 第一级）。
#[tokio::test]
async fn ex4_first_byte_timeout() {
    let server = MockServer::start().await;
    server
        .register(
            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(1))),
        )
        .await;
    let timeouts = Timeouts {
        first_byte: Duration::from_millis(150),
        chunk_gap: Duration::from_secs(30),
    };
    let (outcome, deltas) = run_stream(&server.uri(), "原文", timeouts).await;
    match outcome {
        StreamOutcome::Failed { error, partial } => {
            assert_eq!(error.ex_code(), "EX-4", "响应头前超时应归为 EX-4");
            assert!(partial.is_empty());
            assert!(deltas.is_empty());
        }
        other => panic!("应归为 EX-4，实际 {other:?}"),
    }
}

/// EX-5：流中相邻 chunk 间隔超时 → 保留 partial（D1 第二级）。
#[tokio::test]
async fn ex5_mid_stream_gap_keeps_partial() {
    let server = RawSseServer::start(vec![
        (Duration::ZERO, sse_ok_headers()),
        (Duration::ZERO, delta_frame("部分译文")),
        // 第二块在 600ms 后才到达——客户端 chunk_gap=200ms 判定中断，[DONE] 永不消费。
        // 注：客户端中断即断开，服务端 write_all 失败后进入 EOF 等待，连接自然收敛。
        (Duration::from_millis(600), done_frame()),
    ]);
    let (outcome, deltas) = run_stream(&server.uri(), "原文", fast_timeouts()).await;
    match outcome {
        StreamOutcome::Failed { error, partial } => {
            assert_eq!(error.ex_code(), "EX-5", "块间隔超时应归为 EX-5");
            assert_eq!(partial, "部分译文", "EX-5 必须保留已收增量");
            assert_eq!(deltas, vec!["部分译文".to_string()]);
        }
        other => panic!("应归为 EX-5 并保留 partial，实际 {other:?}"),
    }
    // 客户端断开后服务端连接应收敛（A7：不残留幽灵连接）。
    assert!(
        wait_until(|| server.active() == 0, Duration::from_secs(3)).await,
        "EX-5 中断后服务端活跃连接应归零（实际 {}）",
        server.active()
    );
}

/// EX-14：HTTP 5xx / 非 JSON 数据帧 / 流内 error 对象。
#[tokio::test]
async fn ex14_5xx_non_json_and_stream_error() {
    // 5xx
    let server = MockServer::start().await;
    server
        .register(
            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .respond_with(ResponseTemplate::new(503)),
        )
        .await;
    let (outcome, _) = run_stream(&server.uri(), "原文", Timeouts::default()).await;
    match outcome {
        StreamOutcome::Failed { error, partial } => {
            assert_eq!(error.ex_code(), "EX-14", "5xx 应归为 EX-14");
            assert!(partial.is_empty());
        }
        other => panic!("5xx 应归为 EX-14，实际 {other:?}"),
    }

    // 非 JSON 数据帧（data: 内容无法解析为 JSON）
    let server = MockServer::start().await;
    server
        .register(
            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .respond_with(ResponseTemplate::new(200).set_body_string("data: not-json\n\n")),
        )
        .await;
    let (outcome, _) = run_stream(&server.uri(), "原文", Timeouts::default()).await;
    match outcome {
        StreamOutcome::Failed { error, partial } => {
            assert_eq!(error.ex_code(), "EX-14", "非 JSON 数据帧应归为 EX-14");
            assert!(partial.is_empty());
        }
        other => panic!("非 JSON 数据帧应归为 EX-14，实际 {other:?}"),
    }

    // 流内 error 对象
    let server = MockServer::start().await;
    server
        .register(
            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_string("data: {\"error\":{\"message\":\"internal\"}}\n\n"),
                ),
        )
        .await;
    let (outcome, _) = run_stream(&server.uri(), "原文", Timeouts::default()).await;
    match outcome {
        StreamOutcome::Failed { error, partial } => {
            assert_eq!(error.ex_code(), "EX-14", "流内 error 应归为 EX-14");
            assert!(partial.is_empty());
        }
        other => panic!("流内 error 应归为 EX-14，实际 {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 集成测试：正路径契约与解析（请求形态/粘包/[DONE]）
// ---------------------------------------------------------------------------

/// 请求形态契约：体**仅** {model, stream:true, messages:[user]} + Bearer 头；响应粘包/空 delta/[DONE]
/// 正常消费为增量推送。
#[tokio::test]
async fn happy_path_exact_request_shape_and_completed() {
    let server = MockServer::start().await;
    let body = format!(
        "{}{}{}{}",
        String::from_utf8(delta_frame("A")).unwrap_or_default(),
        // 空 delta（role 首块）应被静默跳过，不中断流
        "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n",
        String::from_utf8(delta_frame("B")).unwrap_or_default(),
        String::from_utf8(done_frame()).unwrap_or_default(),
    );
    // 契约型 mock：请求头/体逐字节校验，命中才回 200；expect(1) 由 verify 复核。
    server
        .register(
            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .and(header("Authorization", format!("Bearer {TEST_KEY}")))
                .and(header("Content-Type", "application/json"))
                .and(body_json(serde_json::json!({
                    "model": TEST_MODEL,
                    "stream": true,
                    "messages": [ { "role": "user", "content": "你好" } ],
                })))
                .respond_with(ResponseTemplate::new(200).set_body_string(body))
                .expect(1),
        )
        .await;

    let (outcome, deltas) = run_stream(&server.uri(), "你好", Timeouts::default()).await;
    match outcome {
        StreamOutcome::Completed { text } => {
            assert_eq!(text, "AB");
            assert_eq!(deltas, vec!["A".to_string(), "B".to_string()]);
        }
        other => panic!("应正常完成，实际 {other:?}"),
    }
    server.verify().await; // expect(1)：严格命中一次（体/头任一不符则 mock 未命中 → verify 失败）

    // 同一响应体经原始字节流（粘包、无分包）在 wiremock 整体下发时也能按事件边界解析。
    // 分包（一个 UTF-8 data 帧字节被拆两段）已在 pump_events 单测覆盖；此处补协议级验证。
    let server = MockServer::start().await;
    server
        .register(
            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_string(
                        String::from_utf8(delta_frame("你好")).unwrap_or_default(),
                    ),
                ),
        )
        .await;
    let (outcome, _) = run_stream(&server.uri(), "你好", Timeouts::default()).await;
    match outcome {
        StreamOutcome::Failed { error, partial } => {
            // 无 [DONE] 的 EOF：不伪装完整成功（EX-5 口径），但增量已到达。
            assert_eq!(error.ex_code(), "EX-5");
            assert_eq!(partial, "你好");
        }
        StreamOutcome::Completed { .. } => panic!("无 [DONE] 不得视为完成"),
        other => panic!("预期 EX-5 中断，实际 {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 集成测试：取消链路（A7）与重试纪律
// ---------------------------------------------------------------------------

/// 取消 = token 触发 + reqwest 流 drop → 服务端感知断连；活跃连接数归零。
#[tokio::test]
async fn cancel_drops_connection_and_active_returns_to_zero() {
    let server = RawSseServer::start(vec![
        (Duration::ZERO, sse_ok_headers()),
        (Duration::ZERO, delta_frame("前半")),
        // 之后不再发送：客户端请求挂起为活动连接。
    ]);
    let uri = server.uri();
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    let task = tokio::spawn(async move {
        let request = ChatRequest {
            base_url: uri.as_str(),
            model: TEST_MODEL,
            api_key: TEST_KEY,
        };
        let mut deltas: Vec<String> = Vec::new();
        let outcome = sse::stream_chat(
            &request,
            "原文".to_string(),
            task_cancel,
            fast_timeouts(),
            &mut |delta| deltas.push(delta),
        )
        .await;
        (outcome, deltas)
    });

    // 等服务端确认请求已到且已回发首块（这一等待同时排除「取消早于请求发出」的时序）。
    assert!(
        server.wait_request(Duration::from_secs(5)).await,
        "夹具未收到请求（connect={}）",
        server.accepted()
    );
    assert!(
        wait_until(|| server.active() == 1, Duration::from_secs(3)).await,
        "请求建立后服务端应有 1 条活跃连接"
    );

    cancel.cancel();
    let (outcome, _deltas) = match task.await {
        Ok(pair) => pair,
        Err(err) => panic!("stream_chat 任务异常: {err}"),
    };
    assert!(
        matches!(outcome, StreamOutcome::Cancelled),
        "取消应返回 Cancelled，实际 {outcome:?}"
    );

    // A7 收敛：客户端 drop → TCP 关闭 → 服务端读到 EOF → 活跃连接归零。
    assert!(
        wait_until(|| server.active() == 0, Duration::from_secs(3)).await,
        "取消后服务端活跃连接应归零（实际 {}）",
        server.active()
    );
}

/// 重试不叠加请求：失败一轮 → 重试再次发起时恰好一条新请求（无既有的旧连接并行残留）。
/// 注：wiremock 0.6 同路径 mock 按注册顺序匹配（FIFO），不能靠后注册覆盖；本测试验证
/// 「每轮恰一请求 + 失败被正确分类」这一重试纪律，成功路径由 happy_path 覆盖。
#[tokio::test]
async fn retry_after_failure_issues_exactly_one_new_request() {
    let server = MockServer::start().await;
    server
        .register(
            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .respond_with(ResponseTemplate::new(500)),
        )
        .await;

    let (first, _) = run_stream(&server.uri(), "原文", Timeouts::default()).await;
    match first {
        StreamOutcome::Failed { error, .. } => assert_eq!(error.ex_code(), "EX-14"),
        other => panic!("第一轮应失败（EX-14），实际 {other:?}"),
    }
    let (second, _) = run_stream(&server.uri(), "原文", Timeouts::default()).await;
    match second {
        StreamOutcome::Failed { error, .. } => assert_eq!(error.ex_code(), "EX-14"),
        other => panic!("第二轮应收到服务端错误（EX-14），实际 {other:?}"),
    }

    // 恰好两条 POST /chat/completions——不叠加、不重发（每轮独立请求，失败后重试为全新一轮）。
    let requests = match server.received_requests().await {
        Some(requests) => requests,
        None => panic!("未收到任何请求"),
    };
    assert_eq!(
        count_requests(&requests, "/chat/completions", Method::POST),
        2,
        "两轮各恰好一条请求（重试不叠加）"
    );
}

// ---------------------------------------------------------------------------
// 集成测试：连接验证两形态（D6）
// ---------------------------------------------------------------------------

/// 验证命令构造（全部显式给定，不依赖全局配置值——配置会被其他测试并发改写）。
fn verify_command(base: &str) -> VerifyConnectionCommand {
    VerifyConnectionCommand {
        base: Some(base.to_string()),
        model: Some(TEST_MODEL.to_string()),
        key: Some(TEST_KEY.to_string()),
    }
}

/// 测试进程无应用启动装配：`verify_connection` 的输入解析仍会读全局配置（生产时序为启动时
/// init_global；此处先以缺省值初始化即可——验证参数已全部显式给定，全局值不影响任何断言）。
fn ensure_global_config() {
    // 初始化成功或被并发抢占（Err＝已初始化）：均视为就绪——参数全部显式给定，值不影响断言。
    let _ = alya::config::init_global(alya::config::Settings::default());
}

/// D6 形态一：GET {base}/models 直通（免计费验 Key）→ 成功且不触发任何 chat 请求。
#[tokio::test]
async fn verify_connection_models_direct_success() {
    ensure_global_config();
    let server = MockServer::start().await;
    server
        .register(
            Mock::given(method("GET"))
                .and(path("/models"))
                .and(header("Authorization", format!("Bearer {TEST_KEY}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "data": [{ "id": TEST_MODEL }]
                }))),
        )
        .await;
    ok_of(verify_connection(&verify_command(&server.uri())).await);

    let requests = match server.received_requests().await {
        Some(requests) => requests,
        None => panic!("未收到任何请求"),
    };
    assert_eq!(count_requests(&requests, "/models", Method::GET), 1);
    assert_eq!(
        count_requests(&requests, "/chat/completions", Method::POST),
        0,
        "GET /models 成功路径不得触发 chat 请求（免计费验证）"
    );
}

/// D6 形态二：GET /models 404/405 → 回退 max_tokens=1 最小 chat 请求（请求体契约校验）。
#[tokio::test]
async fn verify_connection_chat_minimal_fallback_on_404_and_405() {
    ensure_global_config();
    for models_status in [404u16, 405u16] {
        let server = MockServer::start().await;
        server
            .register(
                Mock::given(method("GET"))
                    .and(path("/models"))
                    .respond_with(ResponseTemplate::new(models_status)),
            )
            .await;
        server
            .register(
                Mock::given(method("POST"))
                    .and(path("/chat/completions"))
                    .and(header("Authorization", format!("Bearer {TEST_KEY}")))
                    .and(body_json(serde_json::json!({
                        "model": TEST_MODEL,
                        "max_tokens": 1,
                        "messages": [ { "role": "user", "content": "ping" } ],
                    })))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "id": "chatcmpl-test"
                    })))
                    .expect(1),
            )
            .await;
        ok_of(verify_connection(&verify_command(&server.uri())).await);
        server.verify().await;
    }
}
