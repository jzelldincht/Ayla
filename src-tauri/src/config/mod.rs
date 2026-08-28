//! config：Settings 结构体（spec 6.3）、tmp+rename 原子持久化与 schemaVersion 迁移桩（T2）。
//!
//! 职责：
//! - `Settings` 逐字段对齐 spec 6.3 示例 JSON（序列化键一律 camelCase），出厂默认值＝FR-13 全表
//!   （本文件是默认值唯一权威源，T13 设置页不得另立默认；面板尺寸 460×340 取 spec v0.2 对齐值）。
//! - 持久化：`tmp 文件 + rename` 原子写（`save_to`）；读入容忍未知字段并以 `extra` 保留透传，
//!   前向兼容旧版本/未来版本（未知键不丢弃，保存时原样回写）。
//! - `schemaVersion: 1` 迁移桩：低于当前版本（含缺失字段的旧文件）载入时经 `migrate` 合并缺省字段、
//!   升版并回写；高于当前版本不降级、不重写（未来版本容忍）。
//! - Q5 九种定向语言常量表（中文名文案与 UI/前端共用一份，spec 6.3）。
//! - 全局 `RwLock<Settings>` 访问器与锁序声明（spec 5.4：**先 config 后 sessions**）。
//!
//! 锁序声明：跨模块共享可变状态共两处（本模块 config 锁、translate/service 会话表锁），
//! 固定顺序为 **先取 config 锁，再取 sessions 锁**；任何模块在持有 config 锁期间禁止反向
//! 嵌套获取 sessions 锁（反之亦然），破坏锁序即构成死锁风险，评审门禁按此核对。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

use serde::{Deserialize, Serialize};

use crate::error::AppError;

pub mod crypto;

/// 当前配置 schema 版本（spec 6.3：`schemaVersion: 1`）。
pub const SCHEMA_VERSION: u32 = 1;

/// 配置文件名与原子写临时文件名（同目录，rename 保证同卷原子替换）。
pub const CONFIG_FILE: &str = "config.json";
pub const TMP_FILE: &str = "config.json.tmp";

/// 定向目标语言出厂默认值（FR-13「翻译｜定向目标语言」，Q5 文案：英文）。
pub const DEFAULT_TARGET_LANG: &str = "英文";
/// AI 服务接口地址出厂默认（FR-13「AI 服务｜接口地址」，OpenAI 兼容协议）。
pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
/// 模型名出厂默认（FR-13「AI 服务｜模型名」）。
pub const DEFAULT_MODEL: &str = "deepseek-chat";

// ---------------------------------------------------------------------------
// Q5 定向语言常量表（首批九种；名称文案与 UI 共用一份，spec 6.3）
// ---------------------------------------------------------------------------

/// 一种定向目标语言：`label` 为提示词 `{{target}}` 替换值（中文名文案），
/// `code` 为 BCP-47 语言标识（供 OCR 语种/未来扩展对齐用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetLang {
    pub code: &'static str,
    pub label: &'static str,
}

/// Q5 裁决：首批九种定向语言（不改增删，改经需求变更）。
pub const DIRECTED_TARGET_LANGS: &[TargetLang] = &[
    TargetLang {
        code: "zh-CN",
        label: "中文(简体)",
    },
    TargetLang {
        code: "zh-TW",
        label: "中文(繁体)",
    },
    TargetLang {
        code: "en",
        label: "英文",
    },
    TargetLang {
        code: "ja",
        label: "日文",
    },
    TargetLang {
        code: "ko",
        label: "韩文",
    },
    TargetLang {
        code: "fr",
        label: "法文",
    },
    TargetLang {
        code: "de",
        label: "德文",
    },
    TargetLang {
        code: "es",
        label: "西班牙文",
    },
    TargetLang {
        code: "ru",
        label: "俄文",
    },
];

/// 按中文文案查语言；不在清单内返回 `None`（设置/面板存值须先在清单内校验）。
pub fn target_lang_by_label(label: &str) -> Option<&'static TargetLang> {
    DIRECTED_TARGET_LANGS
        .iter()
        .find(|lang| lang.label == label)
}

/// 中文文案是否为合法定向目标语言（Q5 清单内）。
pub fn is_valid_target_lang(label: &str) -> bool {
    target_lang_by_label(label).is_some()
}

// ---------------------------------------------------------------------------
// Settings 结构体（spec 6.3 逐字段；出厂默认值 FR-13 全表）
// ---------------------------------------------------------------------------

/// 应用配置（单一事实源，INV-8）。序列化键为 spec 6.3 示例 JSON 的 camelCase 形式。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// 前向迁移桩（spec 6.3）：旧版文件缺省视为 v0，由 `migrate` 升 v1 并回写。
    #[serde(default = "schema_version_legacy_default")]
    pub schema_version: u32,
    #[serde(default)]
    pub ai: AiSettings,
    #[serde(default)]
    pub translation: TranslationSettings,
    #[serde(default)]
    pub panel: PanelSettings,
    /// 剪贴板监听开关（FR-5.3，出厂默认关）；与面板/托盘/设置三方同步（INV-8）。
    #[serde(default)]
    pub clipboard_monitor: bool,
    /// 活动场景（FR-2.3，持久化；出厂默认互译）。
    #[serde(default)]
    pub scene: SceneSettings,
    /// 五项可改热键（FR-14；出厂值 FR-14.1）。
    #[serde(default)]
    pub hotkeys: HotkeySettings,
    #[serde(default)]
    pub ocr: OcrSettings,
    #[serde(default)]
    pub general: GeneralSettings,
    #[serde(default)]
    pub meta: MetaSettings,
    /// 未知字段保留透传（前向兼容：旧/新版本多余字段读取后原样回写，不丢不改）。
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// AI 服务（FR-13「AI 服务」全组）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiSettings {
    #[serde(default = "base_url_default")]
    pub base_url: String,
    #[serde(default = "model_default")]
    pub model: String,
    /// DPAPI 密文（base64）；`None` ＝ 未配置 Key。明文绝不驻留本结构（INV-1）。
    #[serde(default)]
    pub api_key_cipher: Option<String>,
    /// 未知字段保留透传。
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// 翻译默认（FR-13「翻译」全组）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TranslationSettings {
    /// 定向目标语言默认值（FR-13：英文）。
    #[serde(default = "default_target_lang_value")]
    pub default_target_lang: String,
    /// 译文呈现（FR-10.1：全部流式 / 分档呈现）。
    #[serde(default)]
    pub presentation: PresentationMode,
    /// 长文整段阈值（FR-10.2：1000 字符，含空白与换行 Q10）；仅分档呈现生效。
    #[serde(default = "long_text_threshold_default")]
    pub long_text_threshold_chars: u32,
    /// 原文变化自动翻译（真机修复轮 12；用户需求：默认关，回车翻译）。
    #[serde(default)]
    pub auto_translate: AutoTranslateSettings,
    /// 未知字段保留透传。
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// 原文变化自动翻译（轮 12）：`enabled` 出厂默认关（按回车翻译）；
/// `delay_ms` = 打字停顿/续输后等待触发的防抖时长（档位 300~2000，出厂 700）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoTranslateSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "auto_translate_delay_default")]
    pub delay_ms: u32,
    /// 未知字段保留透传。
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for AutoTranslateSettings {
    fn default() -> Self {
        AutoTranslateSettings {
            enabled: false,
            delay_ms: auto_translate_delay_default(),
            extra: serde_json::Map::new(),
        }
    }
}

/// 译文呈现模式（Q4 裁决：两档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationMode {
    /// 全部流式（FR-10.1：所有译文一律流式增量输出；出厂默认）。
    #[default]
    StreamAll,
    /// 分档呈现（FR-10.1：短文流式 + 长文整段）。
    Staged,
}

/// 面板（FR-9.2/9.3；尺寸为**逻辑像素**持久化，INV-4）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PanelSettings {
    /// 失焦即隐（FR-9.2，出厂默认开）。
    #[serde(default = "hide_on_blur_default")]
    pub hide_on_blur: bool,
    /// 面板逻辑宽度（spec v0.2 对齐 design-spec 460）。
    #[serde(default = "panel_width_default")]
    pub width_logical: u32,
    /// 面板逻辑高度（spec v0.2 对齐 design-spec 340）。
    #[serde(default = "panel_height_default")]
    pub height_logical: u32,
    /// 未知字段保留透传。
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// 活动场景（FR-2.3：互译 / 定向，全局唯一、持久化、出厂默认互译）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SceneSettings {
    #[serde(default)]
    pub active: SceneMode,
    /// 未知字段保留透传。
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneMode {
    #[default]
    Mutual,
    Directed,
}

/// 五项可改热键（FR-14.2；组合字串形如 "Alt+Q"；出厂值 FR-14.1）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HotkeySettings {
    #[serde(default = "hotkey_selection_default")]
    pub selection: String,
    #[serde(default = "hotkey_input_default")]
    pub input: String,
    #[serde(default = "hotkey_ocr_recognize_default")]
    pub ocr_recognize: String,
    #[serde(default = "hotkey_ocr_translate_default")]
    pub ocr_translate: String,
    #[serde(default = "hotkey_toggle_scene_default")]
    pub toggle_scene: String,
    /// 未知字段保留透传。
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// 截图 OCR（FR-6.4）：`auto` ＝ 跟随系统首选语言（A6），或固定为已装语言包 tag。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OcrSettings {
    #[serde(default = "ocr_language_default")]
    pub language: String,
    /// 未知字段保留透传。
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// 通用（FR-1.2/1.5）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GeneralSettings {
    /// 开机自启（FR-1.5，出厂默认关）。
    #[serde(default)]
    pub autostart: bool,
    /// 主题（FR-1.2：亮 / 暗 / 跟随系统，出厂默认跟随系统）。
    #[serde(default)]
    pub theme: ThemeMode,
    /// 未知字段保留透传。
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    System,
}

/// 首次运行引导状态（FR-11.1/11.5）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MetaSettings {
    #[serde(default)]
    pub onboarding_completed: bool,
    /// 未知字段保留透传。
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// 出厂默认值（FR-13 全表；唯一权威源）
// ---------------------------------------------------------------------------

/// 旧版文件（无 `schemaVersion` 键）视为 v0，交由迁移桩升 v1（出厂新配置恒显式写 v1）。
fn schema_version_legacy_default() -> u32 {
    0
}
fn base_url_default() -> String {
    DEFAULT_BASE_URL.to_string()
}
fn model_default() -> String {
    DEFAULT_MODEL.to_string()
}
fn default_target_lang_value() -> String {
    DEFAULT_TARGET_LANG.to_string()
}
fn long_text_threshold_default() -> u32 {
    1000
}
fn auto_translate_delay_default() -> u32 {
    700
}
fn hide_on_blur_default() -> bool {
    true
}
fn panel_width_default() -> u32 {
    460
}
fn panel_height_default() -> u32 {
    340
}
fn hotkey_selection_default() -> String {
    "Alt+Q".to_string()
}
fn hotkey_input_default() -> String {
    "Alt+E".to_string()
}
fn hotkey_ocr_recognize_default() -> String {
    "Alt+S".to_string()
}
fn hotkey_ocr_translate_default() -> String {
    "Alt+D".to_string()
}
fn hotkey_toggle_scene_default() -> String {
    "Alt+W".to_string()
}
fn ocr_language_default() -> String {
    "auto".to_string()
}

/// 各子结构手动实现 Default：serde 的 `#[serde(default)]`（缺字段合并）与 `Settings::default()`
/// 共用同一来源，保证"缺字段 = 出厂默认值"恒成立（FR-13 唯一权威）。
impl Default for AiSettings {
    fn default() -> Self {
        AiSettings {
            base_url: DEFAULT_BASE_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            api_key_cipher: None,
            extra: serde_json::Map::new(),
        }
    }
}

impl Default for TranslationSettings {
    fn default() -> Self {
        TranslationSettings {
            default_target_lang: DEFAULT_TARGET_LANG.to_string(),
            presentation: PresentationMode::StreamAll,
            long_text_threshold_chars: 1000,
            auto_translate: AutoTranslateSettings::default(),
            extra: serde_json::Map::new(),
        }
    }
}

impl Default for PanelSettings {
    fn default() -> Self {
        PanelSettings {
            hide_on_blur: true,
            width_logical: 460,
            height_logical: 340,
            extra: serde_json::Map::new(),
        }
    }
}

impl Default for SceneSettings {
    fn default() -> Self {
        SceneSettings {
            active: SceneMode::Mutual,
            extra: serde_json::Map::new(),
        }
    }
}

impl Default for HotkeySettings {
    fn default() -> Self {
        HotkeySettings {
            selection: "Alt+Q".to_string(),
            input: "Alt+E".to_string(),
            ocr_recognize: "Alt+S".to_string(),
            ocr_translate: "Alt+D".to_string(),
            toggle_scene: "Alt+W".to_string(),
            extra: serde_json::Map::new(),
        }
    }
}

impl Default for OcrSettings {
    fn default() -> Self {
        OcrSettings {
            language: "auto".to_string(),
            extra: serde_json::Map::new(),
        }
    }
}

impl Default for GeneralSettings {
    fn default() -> Self {
        GeneralSettings {
            autostart: false,
            theme: ThemeMode::System,
            extra: serde_json::Map::new(),
        }
    }
}

impl Default for MetaSettings {
    fn default() -> Self {
        MetaSettings {
            onboarding_completed: false,
            extra: serde_json::Map::new(),
        }
    }
}

impl Default for Settings {
    /// 全新出厂默认（FR-13 全表；首次启动写入前即此形态）。
    fn default() -> Self {
        Settings {
            schema_version: SCHEMA_VERSION,
            ai: AiSettings::default(),
            translation: TranslationSettings::default(),
            panel: PanelSettings::default(),
            clipboard_monitor: false,
            scene: SceneSettings::default(),
            hotkeys: HotkeySettings::default(),
            ocr: OcrSettings::default(),
            general: GeneralSettings::default(),
            meta: MetaSettings::default(),
            extra: serde_json::Map::new(),
        }
    }
}

impl Settings {
    /// 迁移桩：将非当前版本配置合并到当前 schema。
    /// - 低于 `SCHEMA_VERSION`（含缺失字段的旧文件）：serde 已按字段默认值合并缺省字段，
    ///   此处只负责版本号提升（v0→v1 无字段搬迁，spec 8.3 全新应用无历史数据）。
    /// - 高于 `SCHEMA_VERSION`：未来版本，容忍保留，不降级不重写。
    pub fn migrate(&mut self) {
        if self.schema_version < SCHEMA_VERSION {
            log::info!(
                "config: schemaVersion {} -> {}（迁移桩：缺省字段已按默认值合并，无字段搬迁）",
                self.schema_version,
                SCHEMA_VERSION
            );
            self.schema_version = SCHEMA_VERSION;
        } else if self.schema_version > SCHEMA_VERSION {
            log::info!(
                "config: 未来版本 schemaVersion={}，当前程序按 v{} 容忍读取（不降级不重写）",
                self.schema_version,
                SCHEMA_VERSION
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 持久化：tmp 文件 + rename 原子写（spec 6.3）
// ---------------------------------------------------------------------------

/// 配置文件完整路径（`dir` = app_config_dir，Q7：安装版/便携版共用）。
pub fn config_path(dir: &Path) -> PathBuf {
    dir.join(CONFIG_FILE)
}

/// 配置文件是否存在（FR-11.1 首启判定：无任何已存配置则进入引导）。
pub fn config_file_exists(dir: &Path) -> bool {
    config_path(dir).exists()
}

/// 从目录加载配置：文件不存在 → 出厂默认（首次启动）；存在 → 解析（未知字段保留透传）。
/// 低于当前版本时先迁移并回写磁盘（迁移桩持久化）。
pub fn load_from(dir: &Path) -> Result<Settings, AppError> {
    let path = config_path(dir);
    if !path.exists() {
        return Ok(Settings::default());
    }
    let raw = fs::read_to_string(&path)
        .map_err(|err| AppError::Config(format!("读取配置失败 {}: {err}", path.display())))?;
    let mut settings: Settings = serde_json::from_str(&raw)
        .map_err(|err| AppError::Config(format!("配置 JSON 解析失败: {err}")))?;
    if settings.schema_version != SCHEMA_VERSION {
        let before = settings.schema_version;
        settings.migrate();
        if before != settings.schema_version {
            // 迁移桩回写：让磁盘上的版本号与新字段即刻生效（重写仅为版本升级，幂等）。
            save_to(dir, &settings)?;
        }
    }
    Ok(settings)
}

/// 原子保存：写入 `config.json.tmp` →（sync_all 落盘）→ `rename` 原子替换。
/// Windows 上 `std::fs::rename` 经 `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` 语义替换目标；
/// 若仍失败（目标被占用/杀软拦截），删除旧文件后重试一次，保持磁盘上至多一份有效配置。
pub fn save_to(dir: &Path, settings: &Settings) -> Result<(), AppError> {
    fs::create_dir_all(dir)
        .map_err(|err| AppError::Config(format!("创建配置目录失败 {}: {err}", dir.display())))?;

    let tmp = dir.join(TMP_FILE);
    let final_path = config_path(dir);
    let json = serde_json::to_string_pretty(settings)
        .map_err(|err| AppError::Config(format!("配置序列化失败: {err}")))?;

    {
        use std::io::Write;
        let mut file = fs::File::create(&tmp).map_err(|err| {
            AppError::Config(format!("创建临时文件失败 {}: {err}", tmp.display()))
        })?;
        file.write_all(json.as_bytes()).map_err(|err| {
            AppError::Config(format!("写入临时文件失败 {}: {err}", tmp.display()))
        })?;
        // 数据先落盘再 rename，防止断电后出现完整原文件被替换为半截新文件。
        file.sync_all().map_err(|err| {
            AppError::Config(format!("临时文件落盘失败 {}: {err}", tmp.display()))
        })?;
    }

    if let Err(first_err) = fs::rename(&tmp, &final_path) {
        // 兜底：目标存在或暂被占用。先清旧文件再重试一次；仍失败则清理 tmp 并上抛（不吞异常）。
        let _ = fs::remove_file(&final_path);
        if let Err(second_err) = fs::rename(&tmp, &final_path) {
            let _ = fs::remove_file(&tmp);
            return Err(AppError::Config(format!(
                "配置原子替换失败 {}: {first_err}; 重试后仍失败: {second_err}",
                final_path.display()
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// API Key 可用性（spec 5.7.8 降级口径）
// ---------------------------------------------------------------------------

/// 设置是否具备可用 API Key（INV-1：前端只能得到该布尔）。
/// - 密文缺失 → `false`（未配置）；
/// - 解密成功 → `true`；
/// - 解密失败（密码重置/配置跨用户拷贝等）→ 视同未配置（`false`），不崩溃、不弹系统错误，
///   仅记一条**无值**日志（不落 Key/密文/熵等任何值，INV-7）。
pub fn has_key(settings: &Settings) -> bool {
    match &settings.ai.api_key_cipher {
        None => false,
        Some(cipher) => match crypto::unprotect(cipher) {
            Ok(_key_plain) => true,
            Err(err) => {
                log::warn!(
                    "config: apiKeyCipher 无法解出，视同未配置（{}-类别日志，无任何值）",
                    err.class()
                );
                false
            }
        },
    }
}

// ---------------------------------------------------------------------------
// 全局 RwLock<Settings> 访问器（单一事实源，INV-8）
// ---------------------------------------------------------------------------

/// 全局配置状态：进程内单份。启动序列（配置加载完成后）以 `init_global` 安装；
/// 之后所有模块经 `global`/`read_global`/`write_global` 访问，禁止持有其他克隆副本。
///
/// 锁序：先 config（本锁）后 sessions（translate/service）——禁止嵌套反向（模块头声明）。
static CONFIG: OnceLock<RwLock<Settings>> = OnceLock::new();

/// 安装全局配置（仅一次；重复安装返回错误而非覆盖，防多源写入）。
pub fn init_global(settings: Settings) -> Result<(), AppError> {
    CONFIG
        .set(RwLock::new(settings))
        .map_err(|_| AppError::Config("config: init_global 重复调用，全局状态尚未替换".to_string()))
}

/// 访问全局配置锁（未初始化 → 错误；调用方按装配时序保证先 init_global）。
pub fn global() -> Result<&'static RwLock<Settings>, AppError> {
    CONFIG
        .get()
        .ok_or_else(|| AppError::Config("config: 全局配置尚未初始化（先 init_global）".to_string()))
}

/// 读锁守卫（先 config 后 sessions 的锁序起点）。
pub fn read_global() -> Result<RwLockReadGuard<'static, Settings>, AppError> {
    let lock = global()?;
    let guard = lock
        .read()
        .map_err(|_| AppError::Config("config: 全局配置读锁中毒".to_string()))?;
    Ok(guard)
}

/// 写锁守卫。写后需显式 `save_to`（本模块不隐式落盘，写策略由调用方决定）。
pub fn write_global() -> Result<RwLockWriteGuard<'static, Settings>, AppError> {
    let lock = global()?;
    let guard = lock
        .write()
        .map_err(|_| AppError::Config("config: 全局配置写锁中毒".to_string()))?;
    Ok(guard)
}

/// 默认配置目录（Tauri app_config_dir，即 `%APPDATA%\com.alya.app`；安装版/便携版共用 Q7）。
pub fn app_config_dir(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
    use tauri::Manager;
    app.path()
        .app_config_dir()
        .map_err(|err| AppError::Config(format!("解析 app_config_dir 失败: {err}")))
}

// ---------------------------------------------------------------------------
// 单元测试：读写往返 / 默认值 / 原子写 / 未知字段容忍 / 迁移桩 / 语言清单 / 降级分支
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicU32, Ordering};

    /// 无 unwrap/expect 的取值辅助（INV-6：clippy deny 覆盖测试目标；panic 仅用于测试断言失败）。
    fn ok_of<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(err) => panic!("期望成功值，实际错误: {err}"),
        }
    }

    /// 独立临时目录（路径注入，绝不触碰真实 %APPDATA%）。
    fn test_dir(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "alya-config-test-{tag}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        assert!(
            fs::create_dir_all(&dir).is_ok(),
            "创建测试临时目录失败: {}",
            dir.display()
        );
        dir
    }

    fn cleanup(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn defaults_match_fr13_full_table() {
        let s = Settings::default();
        assert_eq!(s.schema_version, SCHEMA_VERSION);
        // AI 服务（FR-13）
        assert_eq!(s.ai.base_url, "https://api.deepseek.com");
        assert_eq!(s.ai.model, "deepseek-chat");
        assert_eq!(s.ai.api_key_cipher, None);
        // 翻译（FR-13）
        assert_eq!(s.translation.default_target_lang, "英文");
        assert_eq!(s.translation.presentation, PresentationMode::StreamAll);
        assert_eq!(s.translation.long_text_threshold_chars, 1000);
        // 面板（FR-9.2/9.3；尺寸 spec v0.2 对齐 design-spec）
        assert!(s.panel.hide_on_blur);
        assert_eq!(s.panel.width_logical, 460);
        assert_eq!(s.panel.height_logical, 340);
        // 剪贴板监听（FR-5.3）
        assert!(!s.clipboard_monitor);
        // 活动场景（FR-2.3）
        assert_eq!(s.scene.active, SceneMode::Mutual);
        // 快捷键（FR-14.1）
        assert_eq!(s.hotkeys.selection, "Alt+Q");
        assert_eq!(s.hotkeys.input, "Alt+E");
        assert_eq!(s.hotkeys.ocr_recognize, "Alt+S");
        assert_eq!(s.hotkeys.ocr_translate, "Alt+D");
        assert_eq!(s.hotkeys.toggle_scene, "Alt+W");
        // OCR（FR-13：自动）
        assert_eq!(s.ocr.language, "auto");
        // 通用（FR-1.5/1.2）
        assert!(!s.general.autostart);
        assert_eq!(s.general.theme, ThemeMode::System);
        // meta（FR-11.1）
        assert!(!s.meta.onboarding_completed);
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = test_dir("roundtrip");
        let mut s = Settings::default();
        s.ai.model = "deepseek-reasoner".to_string();
        s.translation.default_target_lang = "日文".to_string();
        s.translation.presentation = PresentationMode::Staged;
        s.translation.long_text_threshold_chars = 2000;
        s.clipboard_monitor = true;
        s.scene.active = SceneMode::Directed;
        s.hotkeys.selection = "Ctrl+Shift+Q".to_string();
        s.general.theme = ThemeMode::Dark;
        s.general.autostart = true;
        s.ocr.language = "ja".to_string();
        s.panel.hide_on_blur = false;
        s.meta.onboarding_completed = true;

        ok_of(save_to(&dir, &s));
        let loaded = ok_of(load_from(&dir));
        assert_eq!(loaded, s);
        cleanup(&dir);
    }

    #[test]
    fn atomic_write_overwrites_without_tmp_leftover() {
        let dir = test_dir("atomic");
        let mut s = Settings::default();
        ok_of(save_to(&dir, &s));
        // 再次保存（覆盖既有文件）：rename 语义替换目标，不残留 tmp。
        s.ai.model = "deepseek-chat-next".to_string();
        ok_of(save_to(&dir, &s));
        let loaded = ok_of(load_from(&dir));
        assert_eq!(loaded.ai.model, "deepseek-chat-next");
        assert!(!dir.join(TMP_FILE).exists(), "原子写后不应残留 tmp 文件");
        assert!(dir.join(CONFIG_FILE).exists());
        cleanup(&dir);
    }

    #[test]
    fn load_missing_file_returns_factory_defaults() {
        let dir = test_dir("missing");
        let s = ok_of(load_from(&dir));
        assert_eq!(s, Settings::default());
        assert!(!config_file_exists(&dir));
        cleanup(&dir);
    }

    #[test]
    fn unknown_fields_tolerated_and_preserved() {
        let dir = test_dir("unknown");
        let raw = r#"{
          "schemaVersion": 1,
          "ai": { "baseUrl": "https://api.deepseek.com", "model": "deepseek-chat", "apiVersion": "2024-01" },
          "translation": { "defaultTargetLang": "英文", "newerField": [1, 2] },
          "futureTopLevel": { "enabled": true }
        }"#;
        let path = config_path(&dir);
        ok_of(fs::write(&path, raw));

        let s = ok_of(load_from(&dir));
        // 未声明字段读入后保留
        assert_eq!(
            s.ai.extra.get("apiVersion").and_then(|v| v.as_str()),
            Some("2024-01")
        );
        assert!(s.translation.extra.contains_key("newerField"));
        assert!(s.extra.contains_key("futureTopLevel"));
        // 缺省字段按出厂默认合并
        assert_eq!(s.general.theme, ThemeMode::System);
        assert_eq!(s.hotkeys.selection, "Alt+Q");

        // 保存后透传字段原样回写
        ok_of(save_to(&dir, &s));
        let text = ok_of(fs::read_to_string(&path));
        assert!(text.contains("\"apiVersion\""), "未知字段应透传回写");
        assert!(text.contains("\"newerField\""));
        assert!(text.contains("\"futureTopLevel\""));

        let s2 = ok_of(load_from(&dir));
        assert_eq!(s, s2, "往返一致（含透传字段）");
        cleanup(&dir);
    }

    #[test]
    fn migration_stub_upgrades_v0_file_and_persists() {
        let dir = test_dir("migrate");
        // v0 存档：无 schemaVersion，仅一个已知字段（主题 dark）
        let raw = r#"{ "general": { "theme": "dark" } }"#;
        ok_of(fs::write(config_path(&dir), raw));

        let s = ok_of(load_from(&dir));
        assert_eq!(s.schema_version, SCHEMA_VERSION);
        assert_eq!(s.general.theme, ThemeMode::Dark);
        assert_eq!(s.ai.model, "deepseek-chat", "缺省字段按出厂默认合并");

        // 迁移桩回写：磁盘已是 v1
        let disk = ok_of(fs::read_to_string(config_path(&dir)));
        assert!(disk.contains("\"schemaVersion\": 1"), "迁移桩应回写版本号");
        cleanup(&dir);
    }

    #[test]
    fn future_schema_version_tolerated_without_rewrite() {
        let dir = test_dir("future");
        let raw = r#"{ "schemaVersion": 999, "ai": { "baseUrl": "https://example.invalid" } }"#;
        ok_of(fs::write(config_path(&dir), raw));

        let s = ok_of(load_from(&dir));
        assert_eq!(s.schema_version, 999, "未来版本不降级");
        assert_eq!(s.ai.base_url, "https://example.invalid");

        // 不重写：磁盘原文保留
        let disk = ok_of(fs::read_to_string(config_path(&dir)));
        assert_eq!(disk, raw, "未来版本不应被改写");
        cleanup(&dir);
    }

    #[test]
    fn invalid_json_returns_error_not_panic() {
        let dir = test_dir("invalid");
        ok_of(fs::write(config_path(&dir), "{ not json"));
        let err = match load_from(&dir) {
            Ok(_) => panic!("损坏配置应返回错误而非成功"),
            Err(err) => err,
        };
        assert_eq!(
            err.ex_code(),
            "config",
            "配置类错误走内部 config 编码（非 EX 表）"
        );
        cleanup(&dir);
    }

    #[test]
    fn serialization_alignment_with_spec63_example() {
        // spec 6.3 示例（出厂默认值版本）字段集合逐键对齐
        let s = Settings::default();
        let json = ok_of(serde_json::to_value(&s));
        let obj = match json.as_object() {
            Some(obj) => obj,
            None => panic!("配置应序列化为 JSON 对象"),
        };
        assert_eq!(obj.len(), 10, "spec 6.3 顶层恰为十个字段");
        for key in [
            "schemaVersion",
            "ai",
            "translation",
            "panel",
            "clipboardMonitor",
            "scene",
            "hotkeys",
            "ocr",
            "general",
            "meta",
        ] {
            assert!(obj.contains_key(key), "缺少 spec 6.3 顶层字段: {key}");
        }
        let ai = match obj.get("ai").and_then(|v| v.as_object()) {
            Some(ai) => ai,
            None => panic!("ai 应为对象"),
        };
        assert_eq!(
            ai.get("baseUrl").and_then(|v| v.as_str()),
            Some("https://api.deepseek.com")
        );
        assert_eq!(
            ai.get("model").and_then(|v| v.as_str()),
            Some("deepseek-chat")
        );
        assert!(
            ai.contains_key("apiKeyCipher"),
            "apiKeyCipher 键存在（值 null 或密文）"
        );
        let scene = match obj.get("scene").and_then(|v| v.as_object()) {
            Some(scene) => scene,
            None => panic!("scene 应为对象"),
        };
        assert_eq!(scene.get("active").and_then(|v| v.as_str()), Some("mutual"));
    }

    #[test]
    fn directed_target_langs_match_q5() {
        let labels: Vec<&str> = DIRECTED_TARGET_LANGS
            .iter()
            .map(|lang| lang.label)
            .collect();
        assert_eq!(
            labels,
            vec![
                "中文(简体)",
                "中文(繁体)",
                "英文",
                "日文",
                "韩文",
                "法文",
                "德文",
                "西班牙文",
                "俄文"
            ],
            "Q5 九种语言文案逐一确认"
        );
        let mut dedup = labels.clone();
        dedup.sort_unstable();
        dedup.dedup();
        assert_eq!(dedup.len(), labels.len(), "语言文案不得重复");
        // code 唯一性
        let mut codes: Vec<&str> = DIRECTED_TARGET_LANGS.iter().map(|lang| lang.code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(
            codes.len(),
            DIRECTED_TARGET_LANGS.len(),
            "语言 code 不得重复"
        );
    }

    #[test]
    fn target_lang_lookup_and_validation() {
        assert_eq!(
            target_lang_by_label("日文").map(|lang| lang.code),
            Some("ja")
        );
        assert!(is_valid_target_lang("中文(简体)"));
        assert!(is_valid_target_lang("西班牙文"));
        assert!(!is_valid_target_lang("芬兰文"), "九种清单外不得判有效");
        assert!(target_lang_by_label("芬兰文").is_none());
    }

    #[test]
    fn has_key_degrade_branches() {
        // 未配置：密文缺失 → false
        assert!(!has_key(&Settings::default()));

        // 正常路径：protect → has_key true（Windows 真机直接跑真实 DPAPI 函数）
        let cipher = ok_of(crypto::protect(b"sk-fake-test-key-000"));
        let with_cipher = Settings {
            ai: AiSettings {
                api_key_cipher: Some(cipher),
                ..Settings::default().ai
            },
            ..Settings::default()
        };
        assert!(has_key(&with_cipher));

        // 降级：密文损坏（base64 非法）→ 视同未配置，不崩溃不抛错
        let tampered = Settings {
            ai: AiSettings {
                api_key_cipher: Some("!!!not-base64!!!".to_string()),
                ..Settings::default().ai
            },
            ..Settings::default()
        };
        assert!(!has_key(&tampered), "解密失败应降级为未配置");

        // 降级：base64 合法但内容被篡改（DPAPI 解密失败）→ 同样降级
        let cipher = ok_of(crypto::protect(b"sk-fake-test-key-000"));
        let mut chars: Vec<char> = cipher.chars().collect();
        // 翻转首个编码字符（避开 padding），得到合法 base64 但内容不同的密文
        if let Some(first) = chars.get_mut(0) {
            *first = if *first == 'A' { 'B' } else { 'A' };
            let broken = Settings {
                ai: AiSettings {
                    api_key_cipher: Some(chars.into_iter().collect()),
                    ..Settings::default().ai
                },
                ..Settings::default()
            };
            assert!(!has_key(&broken), "篡改密文应降级为未配置");
        }
    }

    #[test]
    fn global_accessor_requires_init_then_reads() {
        // 未初始化：报错而非 panic
        assert!(global().is_err());

        ok_of(init_global(Settings::default()));
        {
            let guard = ok_of(read_global());
            assert_eq!(*guard, Settings::default());
        }

        // 重复 init：拒绝（防多源写入）
        assert!(init_global(Settings::default()).is_err());

        {
            let mut guard = ok_of(write_global());
            guard.ai.model = "m".to_string();
        }
        let guard = ok_of(read_global());
        assert_eq!(guard.ai.model, "m");
    }
}
