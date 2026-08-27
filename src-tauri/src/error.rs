//! error：AppError 枚举，携带 ex_code——EX-1~EX-14 分类映射（T3 实现，spec 5.8；INV-6）。
//!
//! 规则（spec 5.8 总原则）：一切异常经本类型分类并映射到 EX 呈现；禁止静默失败、禁止以
//! 空译文或滞留等待动效冒充成功。各变体的判定依据与 spec 5.8 检测点一一对应（见各变体注释）。
//!
//! 额外变体 `Config`（ex_code=`"config"`）：承载本地配置读写/反序列化失败等内部错误——
//! spec 5.8 未为其定义用户呈现（配置系统异常属程序面问题而非 EX 场景），属内部编码，
//! 不进入前端的 EX 呈现表（T16 装配层决定向上如何呈现）。

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// EX-1：无网络 / 网络不可达（连接类：DNS/TCP/TLS；检测点 sse.rs 请求建立段）。
    #[error("网络连接失败: {0}")]
    Network(String),
    /// EX-2：API Key 无效（HTTP 401/403）。
    #[error("鉴权失败，请检查 API Key: {0}")]
    Auth(String),
    /// EX-3：配额 / 余额不足（HTTP 402）。
    #[error("配额或余额不足，请前往服务商控制台处理: {0}")]
    Quota(String),
    /// EX-4：请求超时——连接至响应头超过 30 秒（D1 第一级；检测点 sse.rs timeout）。
    #[error("请求超时（30 秒未收到响应头）: {0}")]
    TimeoutFirstByte(String),
    /// EX-5：流式传输中断——流中相邻 chunk 间隔 >30 秒或流 Err（D1 第二级；保留 partial）。
    #[error("流式传输中断: {0}")]
    StreamIdle(String),
    /// EX-6：OCR 语言包缺失——TryCreate* 失败/null（含"自动无可用包"与"固定语种未装"两形态，
    /// 呈现附微软官方语言包指引链接；截图翻译就此停止不误发翻译请求）。
    #[error("OCR 语言包缺失: {0}")]
    OcrLangPack(String),
    /// EX-7：快捷键注册冲突——register Err（占用）或改键查重拒绝，通知指明功能名。
    #[error("快捷键注册冲突: {0}")]
    HotkeyConflict(String),
    /// EX-8：剪贴板非文本内容——监听 tick 读文本返回 None；**静默忽略**（无呈现动作）。
    #[error("剪贴板内容为非文本（静默忽略）")]
    ClipboardNonText,
    /// EX-9：空文本触发——划词空选区 / 输入空串 / OCR 空文本（按 FR-3.2/FR-4.3/FR-7.2 分路呈现）。
    #[error("文本为空，已阻止无效请求: {0}")]
    EmptyText(String),
    /// EX-10：取词失败——模拟复制无返回或超时（220ms 无剪贴板变化）。
    #[error("取词失败: {0}")]
    SelectionFailed(String),
    /// EX-11：系统安全桌面——UAC 等安全桌面前台时取词/截屏受限属预期；无特殊处理。
    #[error("系统安全桌面，操作受限（预期内，无需处理）: {0}")]
    SecureDesktop(String),
    /// EX-12：重复启动应用——互斥体已存在，唤起既有实例。
    #[error("应用已在运行（唤起既有实例）: {0}")]
    AlreadyRunning(String),
    /// EX-13：翻译中再次触发——同来源新请求顶替（内部终止旧行，仅呈现最新一轮）。
    #[error("翻译被新请求顶替（中止旧任务）: {0}")]
    Superseded(String),
    /// EX-14：服务端异常响应——HTTP 5xx / body 非 JSON / 流内 error 事件。
    #[error("翻译服务返回异常: {0}")]
    ServerResponse(String),
    /// 内部错误：本地配置读写/序列化失败等（非 EX 场景；不前端呈现，见模块头）。
    #[error("本地配置错误: {0}")]
    Config(String),
    /// 内部错误：抓屏/截图会话失败（捕获、裁剪、PNG 编码、会话缺失等）。spec 5.8 无对应 EX 条目
    /// （EX-11 专指"安全桌面属预期"，与真实截屏失败不得混同）——非 EX 场景，T16 决定呈现。
    #[error("截图失败: {0}")]
    Capture(String),
    /// 内部错误：OCR 运行期失败（PNG 解码、识别执行、读取结果）。EX-6 **仅**指引擎创建失败即
    /// 语言包缺失；解码失败提示装语言包是误导，故二者分类不得混淆——非 EX 场景。
    #[error("OCR 失败: {0}")]
    Ocr(String),
    /// 内部错误：遮罩窗池/窗口生命周期失败（创建、显示、定位、销毁）——非 EX 场景。
    #[error("窗口操作失败: {0}")]
    Window(String),
}

/// IPC 序列化（tauri 命令返回契约，T4/T5 起注册以 `Result<_, AppError>` 返回的命令要求
/// `AppError: Serialize`）：序列化为展示消息字符串（INV-6——前端对错误仅有展示义务，EX 编码
/// 与 retriable 等结构化语义经 `translation://error` 事件载荷传递，见 events.rs）。
impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl AppError {
    /// EX 编码（对 spec 5.8 表逐条归类；QA 与前端按此定位呈现分支）。
    /// 单独变体 `Config` 返回内部编码 `"config"`（无 EX 条目）。
    pub fn ex_code(&self) -> &'static str {
        match self {
            AppError::Network(_) => "EX-1",
            AppError::Auth(_) => "EX-2",
            AppError::Quota(_) => "EX-3",
            AppError::TimeoutFirstByte(_) => "EX-4",
            AppError::StreamIdle(_) => "EX-5",
            AppError::OcrLangPack(_) => "EX-6",
            AppError::HotkeyConflict(_) => "EX-7",
            AppError::ClipboardNonText => "EX-8",
            AppError::EmptyText(_) => "EX-9",
            AppError::SelectionFailed(_) => "EX-10",
            AppError::SecureDesktop(_) => "EX-11",
            AppError::AlreadyRunning(_) => "EX-12",
            AppError::Superseded(_) => "EX-13",
            AppError::ServerResponse(_) => "EX-14",
            AppError::Config(_) => "config",
            AppError::Capture(_) => "capture",
            AppError::Ocr(_) => "ocr",
            AppError::Window(_) => "window",
        }
    }

    /// 面板错误态是否提供「重试」动作（spec 5.8 呈现层口径）：
    /// - EX-1/EX-4/EX-5/EX-14 明示提供重试；
    /// - EX-2（检查 API Key）/EX-3（服务商控制台）/EX-6/EX-7/EX-9/EX-10 为引导性或单向动作；
    /// - EX-8/EX-11/EX-12/EX-13 无错误态呈现（或非错误）。
    pub fn retriable(&self) -> bool {
        matches!(
            self,
            AppError::Network(_)
                | AppError::TimeoutFirstByte(_)
                | AppError::StreamIdle(_)
                | AppError::ServerResponse(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EX-1~EX-14 编码完备性：每个 EX 编码都有对应变体，且编码值严格等于表 5.8 编号。
    #[test]
    fn ex_codes_cover_ex_1_to_14_completely() {
        let cases: [(&str, AppError); 14] = [
            ("EX-1", AppError::Network("DNS 解析失败".into())),
            ("EX-2", AppError::Auth("403 Forbidden".into())),
            ("EX-3", AppError::Quota("402 Payment Required".into())),
            ("EX-4", AppError::TimeoutFirstByte("30s 无响应头".into())),
            ("EX-5", AppError::StreamIdle("chunk 间隔超时".into())),
            ("EX-6", AppError::OcrLangPack("缺少 zh-Hans 语言包".into())),
            ("EX-7", AppError::HotkeyConflict("Alt+Q 已被占用".into())),
            ("EX-8", AppError::ClipboardNonText),
            ("EX-9", AppError::EmptyText("输入为空".into())),
            (
                "EX-10",
                AppError::SelectionFailed("220ms 无剪贴板变化".into()),
            ),
            ("EX-11", AppError::SecureDesktop("UAC 安全桌面".into())),
            (
                "EX-12",
                AppError::AlreadyRunning("instance.pid 存活".into()),
            ),
            ("EX-13", AppError::Superseded("同来源新请求".into())),
            (
                "EX-14",
                AppError::ServerResponse("500 Internal Server Error".into()),
            ),
        ];
        for (expected, err) in cases {
            assert_eq!(err.ex_code(), expected, "EX 编码与 5.8 表应逐条一致");
        }
    }

    /// EX-1~EX-14 编码值唯一性（14 个互不相同，无重复归类）。
    #[test]
    fn ex_codes_are_unique() {
        let errs = [
            AppError::Network("n".into()),
            AppError::Auth("a".into()),
            AppError::Quota("q".into()),
            AppError::TimeoutFirstByte("t".into()),
            AppError::StreamIdle("s".into()),
            AppError::OcrLangPack("o".into()),
            AppError::HotkeyConflict("h".into()),
            AppError::ClipboardNonText,
            AppError::EmptyText("e".into()),
            AppError::SelectionFailed("p".into()),
            AppError::SecureDesktop("d".into()),
            AppError::AlreadyRunning("r".into()),
            AppError::Superseded("u".into()),
            AppError::ServerResponse("v".into()),
        ];
        let mut codes: Vec<&str> = errs.iter().map(|err| err.ex_code()).collect();
        codes.sort_unstable();
        let before = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), before, "14 个 EX 编码不得重复");
        assert_eq!(codes.len(), 14);
    }

    /// 重试口径（spec 5.8 呈现层）：仅 EX-1/4/5/14 提供重试。
    #[test]
    fn retriable_matches_spec58() {
        assert!(AppError::Network("x".into()).retriable());
        assert!(AppError::TimeoutFirstByte("x".into()).retriable());
        assert!(AppError::StreamIdle("x".into()).retriable());
        assert!(AppError::ServerResponse("x".into()).retriable());
        assert!(!AppError::Auth("x".into()).retriable());
        assert!(!AppError::Quota("x".into()).retriable());
        assert!(!AppError::OcrLangPack("x".into()).retriable());
        assert!(!AppError::HotkeyConflict("x".into()).retriable());
        assert!(!AppError::EmptyText("x".into()).retriable());
        assert!(!AppError::SelectionFailed("x".into()).retriable());
        assert!(!AppError::Config("x".into()).retriable());
        assert!(!AppError::Capture("x".into()).retriable());
        assert!(!AppError::Ocr("x".into()).retriable());
        assert!(!AppError::Window("x".into()).retriable());
    }

    /// 内部编码纪律：capture/ocr/window 属内部错误（非 EX 条目），编码值不与 "EX-*" 冲突且互不相同。
    #[test]
    fn internal_variants_have_internal_codes() {
        let internal = [
            AppError::Capture("抓屏失败".into()),
            AppError::Ocr("解码失败".into()),
            AppError::Window("窗创建失败".into()),
        ];
        for err in &internal {
            let code = err.ex_code();
            assert!(
                !code.starts_with("EX-"),
                "内部变体不得伪装成 EX 条目：{code}"
            );
            assert!(!err.retriable(), "内部错误不提供重试动作（无 EX 呈现口径）");
        }
        assert_ne!(
            AppError::Capture("a".into()).ex_code(),
            AppError::Ocr("a".into()).ex_code()
        );
        assert_ne!(
            AppError::Ocr("a".into()).ex_code(),
            AppError::Window("a".into()).ex_code()
        );
        assert_ne!(
            AppError::Window("a".into()).ex_code(),
            AppError::Config("a".into()).ex_code()
        );
    }

    /// 错误信息可直接作为面板错误态 message（无独立格式化层级）。
    #[test]
    fn display_message_is_user_facing() {
        let err = AppError::Auth("回显原因".into());
        let msg = err.to_string();
        assert!(msg.contains("鉴权失败"), "EX-2 文案应含用户可见定位词");
        assert!(msg.contains("回显原因"));
    }

    /// IPC 命令错误序列化契约：序列化为展示消息字符串（前端仅有展示义务；EX 结构化语义走事件）。
    #[test]
    fn serialize_as_message_string() {
        let err = AppError::HotkeyConflict("快捷键「Alt+Q」绑定失败".into());
        let json = serde_json::to_string(&err).unwrap_or_else(|e| panic!("{e}"));
        let expected = serde_json::to_string(&err.to_string()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(json, expected, "命令错误应序列化为字符串（＝Display 消息）");
        let msg = err.to_string();
        assert!(msg.contains("快捷键「Alt+Q」绑定失败"));
        assert_eq!(err.ex_code(), "EX-7");
    }
}
