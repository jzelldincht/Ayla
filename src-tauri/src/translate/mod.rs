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
