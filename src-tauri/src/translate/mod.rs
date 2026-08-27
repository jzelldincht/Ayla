//! translate：翻译会话与 SSE 流式客户端（T7/T8 实现，spec 5.7.7）。
//!
//! 模块划分：
//! - `prompt.rs`：场景→提示词模板构造（requirements.md v0.4 FR-2.5/FR-2.6 逐字权威源 + 快照测试）；
//! - `service.rs`：翻译会话表（HashMap<Source, Round> + 全局 generation）与 EX-13 顶替、分档判定纯函数；
//! - `sse.rs`：SSE 流式客户端（5.7.7 请求形态/两级超时/解析/微批；INV-2 网络出站入口之一）；
//! - `verify.rs`：连接验证（D6：先 GET /models，404/405 回退最小 chat；INV-2 网络出站入口之二）。
//!
//! 依赖方向：translate → config/events/error（禁止反向）；跨模块共享可变状态仅 config 锁与
//! 本模块会话表锁，锁序为**先 config 后 sessions**（config/mod.rs 模块头声明，本模块严守）。

pub mod prompt;
pub mod service;
pub mod sse;
pub mod verify;

use crate::error::AppError;

/// `set_panel_target_lang` 命令（T12 注册；FR-2.7 面板临时目标语言，易失槽位在 prompt.rs）：
/// 面板定向语言下拉变更时调用——仅写内存临时值（prompt::set_panel_temp_target，
/// Q5 校验），**不回写 config 默认值**，重启后自动回落到设置页默认值。
/// 不消费 AppHandle（纯内存态）；参数来自 6.1 契约最小扩展（说明见 events.rs）。
#[tauri::command]
pub fn set_panel_target_lang(lang: Option<String>) -> Result<(), AppError> {
    crate::translate::prompt::set_panel_temp_target(lang)
}
