//! prompt：场景→提示词构造（T7 实现）。
//!
//! 权威源声明（spec §4 / tasks 0.1）：提示词正文以 `requirements.md` v0.4 的 FR-2.5（互译）与
//! FR-2.6（定向）为**唯一权威源**，本文件将两段模板**逐字收录**为常量（含 FR-2.5 v0.4 新增的
//! 「防乱码」规则行），代码与文档不得另存变体副本。快照测试把两段模板文本同时硬编码于测试中
//! 逐字比对——任何文本差异（含空白/换行）触发重新评审。
//!
//! 占位符约定：`{{text}}`（待译文本）与 `{{target}}`（定向目标语言名，Q5 文案）为**字面替换**：
//! 仅替换模板自身的首个占位符，不做转义、不做模型专属编码——用户文本中的同名子串原样保留。
//!
//! FR-2.7 目标语言取值：定向场景下 `{{target}}` 取**面板临时目标语言**，否则回落设置页默认值
//! （本模块提供易失的临时值槽位 `set_panel_temp_target`；默认值解析在 service 层读取 config）。

use std::sync::{OnceLock, RwLock};

use crate::config::{self, SceneMode};
use crate::error::AppError;

/// 互译模板（FR-2.5，requirements.md v0.4 原文逐字收录：含防乱码规则行）。
/// 常量以 `r#"..."#` 原样保留换行；调用时仅做 `{{text}}` 字面替换。
pub(crate) const MUTUAL_TEMPLATE: &str = r#"你是无情的翻译机器。无论输入是单词、短语还是句子，你都必须直接输出翻译结果，绝不做任何解释、评价或反问。

翻译规则：
- 如果输入文本是中文 → 翻译为英文
- 如果输入文本不是中文（包括英文、代码、混合内容等） → 翻译为中文
- 若无法绝对确定语种，默认视为非中文，直接翻译为中文
- 若输入不含可识别的自然语言内容（如乱码、无意义符号），原样输出输入内容，不做猜测性翻译

现在，立即翻译以下文本，只回复译文：
{{text}}"#;

/// 定向模板（FR-2.6，requirements.md v0.4 原文逐字收录：Q1 优化稿 v2）。
/// 常量以 `r#"..."#` 原样保留换行；调用时做 `{{target}}`、`{{text}}` 字面替换。
pub(crate) const DIRECTED_TEMPLATE: &str = r#"你是无情的翻译机器。无论输入是单词、短语还是句子，你都必须直接输出翻译结果，绝不做任何解释、评价或反问。

翻译规则：
- 将以下文本翻译为{{target}}
- 若输入不含可识别的自然语言内容（如乱码、无意义符号），原样输出输入内容，不做猜测性翻译

现在，立即翻译以下文本，只回复译文：
{{text}}"#;

// ---------------------------------------------------------------------------
// 面板临时目标语言（FR-2.7：易失态，仅当前运行期间有效；重启回落设置页默认值）
// ---------------------------------------------------------------------------

static PANEL_TEMP_TARGET: OnceLock<RwLock<Option<String>>> = OnceLock::new();

/// 读取面板临时目标语言（未设置 → None＝回落设置页默认值；锁中毒记错误并回落 None）。
pub fn panel_temp_target() -> Option<String> {
    match PANEL_TEMP_TARGET.get() {
        Some(lock) => match lock.read() {
            Ok(guard) => guard.clone(),
            Err(err) => {
                log::error!("prompt: 面板临时目标语言读锁中毒: {err}");
                None
            }
        },
        None => None,
    }
}

/// 设置面板临时目标语言（`None`＝清除，回落默认值；`Some` 必须在 Q5 九语言清单内）。
/// 面板下拉变更时调用（T12 接线；仅内存态，不写 config——FR-2.7 不回写设置页默认值）。
pub fn set_panel_temp_target(value: Option<String>) -> Result<(), AppError> {
    if let Some(ref label) = value {
        validate_target(label)?;
    }
    let lock = PANEL_TEMP_TARGET.get_or_init(|| RwLock::new(None));
    let mut guard = lock
        .write()
        .map_err(|_| AppError::Config("prompt: 面板临时目标语言写锁中毒".to_string()))?;
    *guard = value;
    Ok(())
}

/// 目标语言合法性校验（Q5 清单：九种，见 config::DIRECTED_TARGET_LANGS）。
pub fn validate_target(label: &str) -> Result<(), AppError> {
    if config::is_valid_target_lang(label) {
        Ok(())
    } else {
        Err(AppError::Config(format!("目标语言不在支持清单内: {label}")))
    }
}

// ---------------------------------------------------------------------------
// 场景→提示词构造
// ---------------------------------------------------------------------------

/// 按活动场景构造完整提示词（spec 5.6.1⑥）：
/// - 互译：FR-2.5 模板 + `{{text}}` 字面替换（target 参数被忽略）；
/// - 定向：FR-2.6 模板 + `{{target}}` 与 `{{text}}` 字面替换。
///
/// `target`：定向场景所需的目标语言名（调用方经 `panel_temp_target()` 回落 config 默认值后传入）；
/// `None`（定向缺目标）→ 内部错误（配置缺失属程序面问题，不当用户 EX 呈现）。
pub fn build_prompt(
    scene: SceneMode,
    target: Option<&str>,
    text: &str,
) -> Result<String, AppError> {
    match scene {
        SceneMode::Mutual => Ok(MUTUAL_TEMPLATE.replacen("{{text}}", text, 1)),
        SceneMode::Directed => {
            let target = target.ok_or_else(|| {
                AppError::Config(
                    "定向场景缺少目标语言（面板临时值未设且设置默认值缺失）".to_string(),
                )
            })?;
            validate_target(target)?;
            let target_replaced = DIRECTED_TEMPLATE.replacen("{{target}}", target, 1);
            Ok(target_replaced.replacen("{{text}}", text, 1))
        }
    }
}

// ---------------------------------------------------------------------------
// 单元测试：快照（逐字对照 requirements.md v0.4）+ 占位符替换（特殊字符/超长文本）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 快照基线（逐字= requirements.md v0.4 FR-2.5 代码块内容）。
    /// 源文件：`D:\WorkSpace\PROJECT\_ai\alya\.specs\alya-v1\requirements.md`（v0.4，2026-08-26）。
    /// 若本常量与上面模板常量不一致 → 模板漂移，触发重新评审（spec §4 权威源纪律）。
    const EXPECTED_MUTUAL_FROM_REQUIREMENTS: &str = r#"你是无情的翻译机器。无论输入是单词、短语还是句子，你都必须直接输出翻译结果，绝不做任何解释、评价或反问。

翻译规则：
- 如果输入文本是中文 → 翻译为英文
- 如果输入文本不是中文（包括英文、代码、混合内容等） → 翻译为中文
- 若无法绝对确定语种，默认视为非中文，直接翻译为中文
- 若输入不含可识别的自然语言内容（如乱码、无意义符号），原样输出输入内容，不做猜测性翻译

现在，立即翻译以下文本，只回复译文：
{{text}}"#;

    /// 快照基线（逐字= requirements.md v0.4 FR-2.6 代码块内容）。
    /// 源文件：`D:\WorkSpace\PROJECT\_ai\alya\.specs\alya-v1\requirements.md`（v0.4，2026-08-26）。
    const EXPECTED_DIRECTED_FROM_REQUIREMENTS: &str = r#"你是无情的翻译机器。无论输入是单词、短语还是句子，你都必须直接输出翻译结果，绝不做任何解释、评价或反问。

翻译规则：
- 将以下文本翻译为{{target}}
- 若输入不含可识别的自然语言内容（如乱码、无意义符号），原样输出输入内容，不做猜测性翻译

现在，立即翻译以下文本，只回复译文：
{{text}}"#;

    fn ok_of<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(err) => panic!("期望成功，实际错误: {err}"),
        }
    }

    /// 快照：模板常量逐字等于 requirements.md v0.4 原文（FR-2.5/FR-2.6）。
    #[test]
    fn templates_match_requirements_v04_verbatim() {
        assert_eq!(
            MUTUAL_TEMPLATE, EXPECTED_MUTUAL_FROM_REQUIREMENTS,
            "互译模板与 requirements.md v0.4 FR-2.5 逐字不一致——须重新评审"
        );
        assert_eq!(
            DIRECTED_TEMPLATE, EXPECTED_DIRECTED_FROM_REQUIREMENTS,
            "定向模板与 requirements.md v0.4 FR-2.6 逐字不一致——须重新评审"
        );
    }

    /// 防乱码规则行存在性（FR-2.5 v0.4 用户批准追加的规则，v0.4 变更点专项快照）。
    #[test]
    fn mutual_template_contains_anti_gibberish_rule_line() {
        assert!(
            MUTUAL_TEMPLATE.contains(
                "- 若输入不含可识别的自然语言内容（如乱码、无意义符号），原样输出输入内容，不做猜测性翻译"
            ),
            "FR-2.5 v0.4 防乱码规则行必须原样保留"
        );
    }

    /// 互译构造：{{text}} 字面替换，其余逐字保留。
    #[test]
    fn mutual_prompt_replaces_text_only() {
        let prompt = ok_of(build_prompt(SceneMode::Mutual, None, "hello world"));
        assert!(prompt.ends_with("\n\n现在，立即翻译以下文本，只回复译文：\nhello world"));
        assert!(prompt.starts_with("你是无情的翻译机器。"));
        assert!(prompt.contains("→ 翻译为英文"));
        assert!(!prompt.contains("{{text}}"), "模板占位符必须被替换");
        assert!(
            !prompt.contains("{{target}}"),
            "互译模板不应出现 target 占位符"
        );
    }

    /// 定向构造：{{target}} 与 {{text}} 均字面替换；target 非清单值 → 内部错误。
    #[test]
    fn directed_prompt_replaces_target_and_text() {
        let prompt = ok_of(build_prompt(
            SceneMode::Directed,
            Some("日文"),
            "今日は良い天気です。",
        ));
        assert!(
            prompt.contains("- 将以下文本翻译为日文"),
            "{{target}} 应被替换为语言名"
        );
        assert!(prompt.ends_with("只回复译文：\n今日は良い天気です。"));
        assert!(!prompt.contains("{{text}}"));
        assert!(!prompt.contains("{{target}}"));

        // 清单校验：Finnish 不在 Q5 九种内
        let err = match build_prompt(SceneMode::Directed, Some("芬兰文"), "x") {
            Ok(_) => panic!("非法目标语言应拒绝"),
            Err(err) => err,
        };
        assert_eq!(err.ex_code(), "config");

        // 定向缺 target（既无面板临时值也无可回落默认值）→ 内部错误（程序面问题）
        let err = match build_prompt(SceneMode::Directed, None, "x") {
            Ok(_) => panic!("缺目标语言应拒绝"),
            Err(err) => err,
        };
        assert_eq!(err.ex_code(), "config");
    }

    /// 特殊字符与占位符同名子串：仅模板占位符被替换，用户内容原样保留（字面替换、不做转义）。
    #[test]
    fn special_characters_preserved_and_no_escape() {
        let tricky = r#"含{{text}}与{{target}}子串 \n <script> & "引号" '$' 制表	符"#;
        let prompt = ok_of(build_prompt(SceneMode::Mutual, None, tricky));
        assert!(prompt.ends_with(&format!(
            "\n\n现在，立即翻译以下文本，只回复译文：\n{tricky}"
        )));
        // 用户内容中原有的 {{text}} 子串只出现在内容位置（模板占位符恰被替换一次 → 不残留）
        let occurrences = prompt.matches("{{text}}").count();
        assert_eq!(occurrences, 1, "仅用户内容中的 {{text}} 子串应保留一次");
        // 定向：target=英文 时用户内容含 {{target}} 保留
        let prompt = ok_of(build_prompt(SceneMode::Directed, Some("英文"), tricky));
        assert_eq!(prompt.matches("{{target}}").count(), 1);
        assert_eq!(prompt.matches("{{text}}").count(), 1);
    }

    /// 超长文本（10 万字符）替换性能与完整性。
    #[test]
    fn long_text_prompt_complete() {
        let long = "长".repeat(100_000);
        let prompt = ok_of(build_prompt(SceneMode::Mutual, None, &long));
        assert!(prompt.ends_with(&long));
        assert_eq!(
            prompt.chars().count(),
            MUTUAL_TEMPLATE.chars().count() - "{{text}}".chars().count() + long.chars().count()
        );
    }

    /// 面板临时目标语言槽位：设置/读取/清除/非法值拒绝；未设置回落 None（由上层取 config 默认）。
    #[test]
    fn panel_temp_target_slot_roundtrip_and_validation() {
        // 未设置 → None（回落默认值的信号）
        assert_eq!(panel_temp_target(), None);
        ok_of(set_panel_temp_target(Some("韩文".to_string())));
        assert_eq!(panel_temp_target().as_deref(), Some("韩文"));
        ok_of(set_panel_temp_target(None));
        assert_eq!(panel_temp_target(), None);
        // 非法值拒绝且不改变当前值
        ok_of(set_panel_temp_target(Some("法文".to_string())));
        let err = match set_panel_temp_target(Some("芬兰文".to_string())) {
            Ok(()) => panic!("非法语言应拒绝"),
            Err(err) => err,
        };
        assert_eq!(err.ex_code(), "config");
        assert_eq!(
            panel_temp_target().as_deref(),
            Some("法文"),
            "非法值不得覆盖当前值"
        );
    }
}
