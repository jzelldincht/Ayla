//! verify：连接验证（T8 实现；spec D6；FR-11.1(c)；INV-2 网络出站入口之二）。
//!
//! D6 请求形态：先 `GET {base}/models`（免计费、验 Key）；若端点不支持（404/405）回退发送
//! `max_tokens=1` 的最小 chat 请求。模型名不在验证期强校验——错误模型名在首次翻译时以 EX-14
//! 呈现。缺省参数（base?/model?/key?）＝用配置已存值；key 参数仅经 IPC 入内存（INV-1）。
//!
//! 分类与 sse.rs 同一口径：连接类→EX-1；401/403→EX-2；402→EX-3；超时（30s 响应头）→EX-4；
//! 5xx/其他→EX-14。
//!
//! 日志纪律（INV-7）：只记 host 与结果归类，绝不记录 baseUrl 全量 / Key / Authorization。

use std::time::Duration;

use crate::config;
use crate::error::AppError;
use crate::events::VerifyConnectionCommand;

/// 验证超时（响应头前；与 sse.rs 第一级超时同口径 30s）。
pub const VERIFY_TIMEOUT: Duration = Duration::from_secs(30);

/// 连接验证入口（6.1 `verify_connection`）。
pub async fn verify_connection(command: &VerifyConnectionCommand) -> Result<(), AppError> {
    let (base, model, key) = resolve_inputs(command)?;
    log::debug!("verify_connection: 开始验证（host={}）", host_of(&base));

    let client = crate::translate::sse::client()?;
    let models_url = format!("{}/models", base.trim_end_matches('/'));
    let request = client
        .get(&models_url)
        .bearer_auth(&key)
        .build()
        .map_err(|err| AppError::Config(format!("接口地址非法: {err}")))?;
    let response = match tokio::time::timeout(VERIFY_TIMEOUT, client.execute(request)).await {
        Ok(Ok(response)) => response,
        Ok(Err(err)) => {
            log::debug!(
                "verify_connection: GET /models 连接失败（host={}）",
                host_of(&base)
            );
            return Err(AppError::Network(format!("{err}")));
        }
        Err(_elapsed) => {
            return Err(AppError::TimeoutFirstByte(format!(
                "{VERIFY_TIMEOUT:?} 内未收到响应头"
            )));
        }
    };

    let status = response.status().as_u16();
    match status {
        // 2xx：Key 与端点有效（D6：GET /models 免计费验证即成功）。
        200..=299 => {
            log::debug!(
                "verify_connection: GET /models 成功（host={}）",
                host_of(&base)
            );
            Ok(())
        }
        // D6：端点不支持 /models → 回退最小 chat 请求。
        404 | 405 => {
            log::debug!(
                "verify_connection: GET /models 返回 404/405，回退最小 chat 请求（host={}）",
                host_of(&base)
            );
            verify_chat_minimal(client, &base, &model, &key).await
        }
        401 | 403 => Err(AppError::Auth(format!("GET /models HTTP {status}"))),
        402 => Err(AppError::Quota(format!("GET /models HTTP {status}"))),
        _ => Err(AppError::ServerResponse(format!(
            "GET /models HTTP {status}"
        ))),
    }
}

/// D6 回退：最小 chat 请求——`POST {base}/chat/completions`，体仅
/// `{model, max_tokens:1, messages:[user]}`（不附加其它参数；成功仅验明 Key/端点有效）。
async fn verify_chat_minimal(
    client: &reqwest::Client,
    base: &str,
    model: &str,
    key: &str,
) -> Result<(), AppError> {
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "max_tokens": 1,
        "messages": [ { "role": "user", "content": "ping" } ],
    });
    let request = client
        .post(&url)
        .bearer_auth(key)
        .json(&body)
        .build()
        .map_err(|err| AppError::Config(format!("接口地址非法: {err}")))?;
    let response = match tokio::time::timeout(VERIFY_TIMEOUT, client.execute(request)).await {
        Ok(Ok(response)) => response,
        Ok(Err(err)) => {
            log::debug!(
                "verify_connection: 最小 chat 请求连接失败（host={}）",
                host_of(base)
            );
            return Err(AppError::Network(format!("{err}")));
        }
        Err(_elapsed) => {
            return Err(AppError::TimeoutFirstByte(format!(
                "{VERIFY_TIMEOUT:?} 内未收到响应头"
            )));
        }
    };
    let status = response.status().as_u16();
    match status {
        200..=299 => {
            log::debug!(
                "verify_connection: 最小 chat 请求成功（host={}）",
                host_of(base)
            );
            Ok(())
        }
        401 | 403 => Err(AppError::Auth(format!("HTTP {status}"))),
        402 => Err(AppError::Quota(format!("HTTP {status}"))),
        _ => Err(AppError::ServerResponse(format!("HTTP {status}"))),
    }
}

/// 解析验证输入：命令参数缺省＝用已存配置值；key 参数为空则用已存密文（INV-1/7）。
/// 纯函数形态（测试喂固定 Settings，不依赖全局锁——配置模块测试会并发改写全局值）。
fn resolve_inputs_from(
    settings: &config::Settings,
    command: &VerifyConnectionCommand,
) -> Result<(String, String, String), AppError> {
    let base = command
        .base
        .clone()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| settings.ai.base_url.clone());
    let model = command
        .model
        .clone()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| settings.ai.model.clone());
    let key = match &command.key {
        Some(key) if !key.trim().is_empty() => key.trim().to_string(),
        _ => match &settings.ai.api_key_cipher {
            Some(cipher) => config::crypto::unprotect_str(cipher).map_err(|err| {
                AppError::Config(format!("API Key 无法解出（视同未配置）: {}", err.class()))
            })?,
            None => return Err(AppError::Config("未配置 API Key，无法验证连接".to_string())),
        },
    };
    Ok((base, model, key))
}

/// 生产包装：读全局配置（读锁在块尾释放，不跨 await 持锁——先 config 后 sessions 锁序）。
fn resolve_inputs(command: &VerifyConnectionCommand) -> Result<(String, String, String), AppError> {
    let settings = config::read_global()?;
    resolve_inputs_from(&settings, command)
}

/// 仅取 host 供日志（INV-7：日志不得含 baseUrl 全量）。
fn host_of(base: &str) -> String {
    match reqwest::Url::parse(base) {
        Ok(url) => url
            .host_str()
            .map(|host| host.to_string())
            .unwrap_or_else(|| "<无法解析>".to_string()),
        Err(_) => "<非法 URL>".to_string(),
    }
}

// ---------------------------------------------------------------------------
// 单元测试：输入解析（缺省回退/参数优先/Key 优先级）+ host 日志裁剪
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_of<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(err) => panic!("期望成功，实际错误: {err}"),
        }
    }

    #[test]
    fn host_of_keeps_only_host_for_logging() {
        // INV-7：日志只记 host，不含 path/query/凭据形态的 base。
        assert_eq!(
            host_of("https://api.deepseek.com/some/path?token=secret"),
            "api.deepseek.com"
        );
        assert_eq!(host_of("http://127.0.0.1:8080/v1"), "127.0.0.1");
        assert_eq!(host_of("not-a-url"), "<非法 URL>");
    }

    /// 解析语义：参数优先；空白参数视同缺省；缺 key 且无密文 → 明确错误。
    /// 喂固定 `Settings::default()`——全局配置会被配置模块的并发测试改写，测试必须自包含。
    #[test]
    fn resolve_inputs_fallback_and_override_logic() {
        let defaults = config::Settings::default();

        // 全部缺省（出厂配置无 Key）→ 报未配置 Key。
        let err = match resolve_inputs_from(&defaults, &VerifyConnectionCommand::default()) {
            Ok(_) => panic!("无 Key 应报错"),
            Err(err) => err,
        };
        assert_eq!(err.ex_code(), "config");

        // key 参数显式提供 → 直接使用（其余走默认）。
        let (base, model, key) = ok_of(resolve_inputs_from(
            &defaults,
            &VerifyConnectionCommand {
                key: Some("sk-test-dummy".to_string()),
                ..Default::default()
            },
        ));
        assert_eq!(base, "https://api.deepseek.com");
        assert_eq!(model, "deepseek-chat");
        assert_eq!(key, "sk-test-dummy");

        // 空白 base/model 视同缺省（回落已存值）；空白 key 视同未提供 → 无密文则明确错误。
        let (base, model, key) = ok_of(resolve_inputs_from(
            &defaults,
            &VerifyConnectionCommand {
                base: Some("   ".to_string()),
                model: Some("".to_string()),
                key: Some("sk-test-dummy".to_string()),
            },
        ));
        assert_eq!(base, "https://api.deepseek.com");
        assert_eq!(model, "deepseek-chat");
        assert_eq!(key, "sk-test-dummy");

        // 非空白参数优先于已存值。
        let (base, model, key) = ok_of(resolve_inputs_from(
            &defaults,
            &VerifyConnectionCommand {
                base: Some("https://example.com/v1".to_string()),
                model: Some("custom-model".to_string()),
                key: Some("sk-custom".to_string()),
            },
        ));
        assert_eq!(base, "https://example.com/v1");
        assert_eq!(model, "custom-model");
        assert_eq!(key, "sk-custom");
    }
}
