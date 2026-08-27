//! ocr：Windows.Media.Ocr 封装（T10 实现，spec 5.7.6 / 5.8 EX-6 / 假设 A6）。
//!
//! 数据流：PNG 内存字节 → `InMemoryRandomAccessStream` → `BitmapDecoder` →
//! `GetSoftwareBitmapAsync`（统一转 Bgra8 保证 OcrEngine 支持）→ `SoftwareBitmap` →
//! `OcrEngine::RecognizeAsync`。解码与识别为阻塞段，统一在 `recognize` 的 `spawn_blocking`
//! 内执行（不卡异步运行时）；全程内存（INV-3：无文件路径、无磁盘痕迹）。
//!
//! 引擎创建（EX-6 两形态）：
//! - `Auto` → `TryCreateFromUserProfileLanguages`（系统首选语言口径，A6）；失败 → "自动模式无
//!   可用 OCR 语言包"；
//! - `Tag` → `Language::CreateLanguage` + `TryCreateFromLanguage`；失败 → "未安装『XX』语言包"。
//!
//! 两种形态均附微软官方语言包指引链接；`recognize` 失败即截图翻译停止、不发翻译请求（EX-6）。
//! 二次校验：设置中存的固定语种在识别时同样经 `TryCreateFromLanguage` 校验（语言包可能事后被卸载）。
//!
//! 识别成功但空文本 → `Ok("")`（由上游 FR-7.2 分路提示"未识别到文字"，非错误）。
//! `list_ocr_languages_blocking` 枚举已装语言包（设置语种下拉数据源，从源头减少 EX-6）；
//! 枚举失败 → 空列表（退化为自由文本输入，运行期再报 EX-6，spec 5.7.6）。
//!
//! 错误纪律：仅引擎创建失败归 `AppError::OcrLangPack`（EX-6，5.8 检测点"TryCreate* 失败"）；
//! 解码/识别运行期失败归内部 `AppError::Ocr`（ex_code="ocr"，INV-6 分类但不冒充语言包缺失）。

use std::sync::mpsc;

use windows::core::HSTRING;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapDecoder, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};
use windows_future::{AsyncOperationCompletedHandler, AsyncStatus, IAsyncOperation};

use crate::error::AppError;
use crate::events::OcrLanguage;

/// 微软官方语言包安装指引（EX-6 呈现附带的链接；避免提示"缺某语种包"而无处安装）。
pub const MICROSOFT_LANGUAGE_PACK_GUIDE_URL: &str =
    "https://support.microsoft.com/windows/language-packs-for-windows-a5094319-a92d-18de-5b53-1cfc697cfca8";

/// OCR 语种口径（spec 6.3 `ocr.language`：`"auto"` 或 BCP-47 tag）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrLang {
    /// 跟随系统用户语言偏好（A6 口径）。
    Auto,
    /// 固定语种（BCP-47 tag，如 `zh-CN`、`en-US`）。
    Tag(String),
}

impl OcrLang {
    /// 由配置字符串解析（`"auto"` 大小写不敏感；其余视为 tag——可用性由识别时二次校验）。
    pub fn from_config(value: &str) -> Self {
        if value.eq_ignore_ascii_case("auto") {
            OcrLang::Auto
        } else {
            OcrLang::Tag(value.to_string())
        }
    }
}

/// EX-6 分类纯函数（spec 5.8：TryCreate* 失败/null → EX-6）。`hresult` 仅供日志/QA 溯源，
/// 决定分类的是 lang 两形态；`hresult` 以技术附注附加（不改变 EX 呈现主体文案与 retriable 口径）。
pub fn classify_engine_failure(lang: &OcrLang, hresult: i32) -> AppError {
    let guide = MICROSOFT_LANGUAGE_PACK_GUIDE_URL;
    let detail = format!("（系统信息 0x{hresult:08X}）");
    match lang {
        OcrLang::Auto => AppError::OcrLangPack(format!(
            "缺少可用的 OCR 语言包，无法识别文字。请在 Windows 设置中安装语言包后重试。查看微软官方指引：{guide}{detail}"
        )),
        OcrLang::Tag(tag) => AppError::OcrLangPack(format!(
            "未安装“{tag}”语言包，无法识别该语种。请在 Windows 设置中安装语言包后重试。查看微软官方指引：{guide}{detail}"
        )),
    }
}

/// 识别结果归一化：空文本按 FR-7.2 口径返回 `Ok("")`（上游分路"未识别到文字"，不视为错误）。
pub fn finalize_ocr_text(raw: String) -> Result<String, AppError> {
    Ok(raw)
}

/// 阻塞等待 IAsyncOperation 完成（仅限 spawn_blocking 阻塞上下文调用）。
/// 说明：windows-future 0.3 的阻塞原语 `Async::join` 为 crate 私有（不可导入），本函数以
/// 完成回调 + `std::sync::mpsc` 通道等效实现——完成回调在 WinRT 线程池线程上发回结果，
/// 本线程阻塞等待；挂接前已完成的走 Status 快速路径。`Cancelled` 以 win32 1223
/// （ERROR_CANCELLED，经 HRESULT::from_win32 转为 HRESULT 语义）表达。
fn wait_async<T>(operation: IAsyncOperation<T>) -> Result<T, windows::core::Error>
where
    T: windows::core::RuntimeType + Send + 'static,
{
    match operation.Status()? {
        AsyncStatus::Completed => return operation.GetResults(),
        AsyncStatus::Error => {
            return Err(windows::core::Error::from_hresult(operation.ErrorCode()?))
        }
        _ => {}
    }
    let (tx, rx) = mpsc::channel::<Result<T, windows::core::Error>>();
    operation.SetCompleted(&AsyncOperationCompletedHandler::new(
        move |sender, status| {
            // sender: windows_core::Ref<IAsyncOperation<T>>，经 ok() 取其引用（回调保证非空）
            let op = sender.ok()?;
            let result = match status {
                AsyncStatus::Completed => op.GetResults(),
                AsyncStatus::Error => match op.ErrorCode() {
                    Ok(code) => Err(windows::core::Error::from_hresult(code)),
                    Err(e) => Err(e),
                },
                _ => Err(windows::core::Error::from_hresult(
                    windows::core::HRESULT::from_win32(1223), // ERROR_CANCELLED
                )),
            };
            let _ = tx.send(result);
            Ok(())
        },
    ))?;
    rx.recv().map_err(|_| {
        windows::core::Error::from_hresult(windows::core::HRESULT::from_win32(8001))
        // 通道持端丢失
    })?
}

/// 解码 PNG 内存字节 → SoftwareBitmap（非 Bgra8 时 Convert 归一——OcrEngine 按 Bgra8 校验输入，
/// 避免把格式兼容问题误报为语言包问题）。阻塞——spawn_blocking 内调用。
fn decode_png_blocking(png_bytes: &[u8]) -> Result<SoftwareBitmap, AppError> {
    if png_bytes.is_empty() {
        return Err(AppError::Ocr("PNG 数据为空".into()));
    }
    let stream = InMemoryRandomAccessStream::new()
        .map_err(|e| AppError::Ocr(format!("创建内存流失败: 0x{:08X}", e.code().0)))?;
    let writer = DataWriter::CreateDataWriter(&stream)
        .map_err(|e| AppError::Ocr(format!("创建数据写入器失败: 0x{:08X}", e.code().0)))?;
    writer
        .WriteBytes(png_bytes)
        .map_err(|e| AppError::Ocr(format!("写入内存流失败: 0x{:08X}", e.code().0)))?;
    wait_async(
        writer
            .StoreAsync()
            .map_err(|e| AppError::Ocr(format!("提交内存流失败: 0x{:08X}", e.code().0)))?,
    )
    .map_err(|e| AppError::Ocr(format!("提交内存流异常: 0x{:08X}", e.code().0)))?;
    let _ = writer.DetachStream(); // 分离写流，避免其制约解码器读取位置

    let decoder = wait_async(
        BitmapDecoder::CreateAsync(&stream)
            .map_err(|e| AppError::Ocr(format!("创建 PNG 解码器失败: 0x{:08X}", e.code().0)))?,
    )
    .map_err(|e| AppError::Ocr(format!("PNG 解码失败: 0x{:08X}", e.code().0)))?;
    let bitmap = wait_async(
        decoder
            .GetSoftwareBitmapAsync()
            .map_err(|e| AppError::Ocr(format!("获取位图失败: 0x{:08X}", e.code().0)))?,
    )
    .map_err(|e| AppError::Ocr(format!("位图解码失败: 0x{:08X}", e.code().0)))?;
    let decoded_format = bitmap
        .BitmapPixelFormat()
        .map_err(|e| AppError::Ocr(format!("读取位图格式失败: 0x{:08X}", e.code().0)))?;
    if decoded_format == BitmapPixelFormat::Bgra8 {
        Ok(bitmap)
    } else {
        SoftwareBitmap::Convert(&bitmap, BitmapPixelFormat::Bgra8)
            .map_err(|e| AppError::Ocr(format!("位图转 Bgra8 失败: 0x{:08X}", e.code().0)))
    }
}

/// 引擎创建（EX-6 检测点；语言包缺失/未装 → `OcrLangPack`，附官方指引）。
fn create_engine(lang: &OcrLang) -> Result<OcrEngine, AppError> {
    match lang {
        OcrLang::Auto => OcrEngine::TryCreateFromUserProfileLanguages().map_err(|e| {
            log::warn!("OCR 自动引擎创建失败：0x{:08X}", e.code().0);
            classify_engine_failure(lang, e.code().0)
        }),
        OcrLang::Tag(tag) => {
            let language = Language::CreateLanguage(&HSTRING::from(tag)).map_err(|e| {
                log::warn!("OCR 语种构造失败 {tag}：0x{:08X}", e.code().0);
                classify_engine_failure(lang, e.code().0)
            })?;
            OcrEngine::TryCreateFromLanguage(&language).map_err(|e| {
                log::warn!("OCR 固定语种引擎创建失败 {tag}：0x{:08X}", e.code().0);
                classify_engine_failure(lang, e.code().0)
            })
        }
    }
}

/// 识别阻塞主流程（PNG 内存字节 → 文本；空文本 → Ok("")；上游 FR-7.2 分路）。
pub fn recognize_blocking(png_bytes: &[u8], lang: OcrLang) -> Result<String, AppError> {
    let bitmap = decode_png_blocking(png_bytes)?;
    let engine = create_engine(&lang)?;
    let operation = engine
        .RecognizeAsync(&bitmap)
        .map_err(|e| AppError::Ocr(format!("发起识别失败: 0x{:08X}", e.code().0)))?;
    let result = wait_async(operation)
        .map_err(|e| AppError::Ocr(format!("识别执行失败: 0x{:08X}", e.code().0)))?;
    let text = result
        .Text()
        .map_err(|e| AppError::Ocr(format!("读取识别结果失败: 0x{:08X}", e.code().0)))?;
    finalize_ocr_text(text.to_string())
}

/// `recognize`（任务书签名；spawn_blocking 包裹阻塞段，不卡异步运行时）。
pub async fn recognize(png_bytes: Vec<u8>, lang: OcrLang) -> Result<String, AppError> {
    tauri::async_runtime::spawn_blocking(move || recognize_blocking(&png_bytes, lang))
        .await
        .map_err(|e| AppError::Ocr(format!("识别任务被中断: {e}")))?
}

/// 枚举已装 OCR 语言包（设置页语种下拉数据源；失败 → 空列表，spec 5.7.6）。
pub fn list_ocr_languages_blocking() -> Vec<OcrLanguage> {
    let langs = match OcrEngine::AvailableRecognizerLanguages() {
        Ok(langs) => langs,
        Err(e) => {
            log::warn!("枚举 OCR 语言包失败（退化为空列表）：0x{:08X}", e.code().0);
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for language in langs {
        // 单项枚举失败仅跳过该条（语言包可能在枚举间隙被卸载），列表整体仍有价值
        if let (Ok(tag), Ok(label)) = (language.LanguageTag(), language.DisplayName()) {
            out.push(OcrLanguage {
                tag: tag.to_string(),
                label: label.to_string(),
            });
        }
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

/// `list_ocr_languages` 命令（6.1：→ [{tag, label}]；枚举阻塞段放 spawn_blocking）。
#[tauri::command]
pub async fn list_ocr_languages() -> Result<Vec<OcrLanguage>, AppError> {
    tauri::async_runtime::spawn_blocking(list_ocr_languages_blocking)
        .await
        .map_err(|e| AppError::Ocr(format!("语言包枚举任务被中断: {e}")))
}

// ---------------------------------------------------------------------------
// 单元测试（错误分类映射纯逻辑、空文本口径、语言解析）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试断言辅助：Ok 解包（失败给出上下文 panic；避免裸 unwrap/expect——clippy unwrap_used=deny）。
    fn t_ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
        result.unwrap_or_else(|err| panic!("{err:?}"))
    }

    /// 测试断言辅助：Err 解包（成功按测试语义失败）。
    fn t_err<T, E: std::fmt::Debug>(result: Result<T, E>) -> E {
        match result {
            Err(err) => err,
            Ok(_) => panic!("期望 Err，实际 Ok"),
        }
    }

    #[test]
    fn classify_auto_failure_is_ex6_auto_form() {
        // 典型 HRESULT：0x80070032（无可用识别器类错误）仍归 EX-6 自动形态
        let err = classify_engine_failure(&OcrLang::Auto, 0x80070032u32 as i32);
        assert_eq!(err.ex_code(), "EX-6");
        assert!(err.to_string().contains("缺少可用的 OCR 语言包"));
        assert!(err.to_string().contains(MICROSOFT_LANGUAGE_PACK_GUIDE_URL));
        assert!(err.to_string().contains("0x80070032"), "HRESULT 附注可溯源");
        assert!(!err.retriable());
    }

    #[test]
    fn classify_tag_failure_is_ex6_tag_form() {
        // E_INVALIDARG（0x80070057，CreateLanguage/TryCreateFromLanguage 失败典型值）归固定语种形态
        let err = classify_engine_failure(&OcrLang::Tag("zh-CN".into()), 0x80070057u32 as i32);
        assert_eq!(err.ex_code(), "EX-6");
        let msg = err.to_string();
        assert!(msg.contains("zh-CN"), "固定语种形态须点名语种 tag");
        assert!(msg.contains("未安装"));
        assert!(msg.contains(MICROSOFT_LANGUAGE_PACK_GUIDE_URL));
        // 两形态互异（自动形态不点名 tag）
        let auto = classify_engine_failure(&OcrLang::Auto, 0x80070057u32 as i32).to_string();
        assert_ne!(auto, msg);
    }

    #[test]
    fn empty_text_is_ok_for_upstream_fr72() {
        // 识别成功但无字符：Ok("")——上游按 FR-7.2 提示"未识别到文字"，不得伪装成错误/成功文本
        assert_eq!(t_ok(finalize_ocr_text(String::new())), "");
        // 非空文本原样透传（不改写、不裁剪——行组结构由 OcrResult::Text 组织）
        assert_eq!(t_ok(finalize_ocr_text("你好\n世界".into())), "你好\n世界");
    }

    #[test]
    fn ocr_lang_from_config_parses() {
        assert_eq!(OcrLang::from_config("auto"), OcrLang::Auto);
        assert_eq!(OcrLang::from_config("AUTO"), OcrLang::Auto, "大小写不敏感");
        assert_eq!(
            OcrLang::from_config("zh-Hans"),
            OcrLang::Tag("zh-Hans".into())
        );
        assert_eq!(
            OcrLang::from_config(""),
            OcrLang::Tag(String::new()),
            "空串视为 tag（识别时报 EX-6）"
        );
    }

    #[test]
    fn decode_failure_is_internal_ocr_not_ex6() {
        // 解码/运行期失败不得冒充 EX-6（误导用户装语言包）：空数据 → 内部 ocr 编码
        let err = t_err(decode_png_blocking(&[]));
        assert_eq!(err.ex_code(), "ocr");
        assert!(
            !err.to_string().contains("语言包"),
            "解码失败不得提示装语言包"
        );
    }

    // -----------------------------------------------------------------------
    // 真机冒烟（默认忽略；属 checklist 真机矩阵 A6/交叉验证项——依赖实际显示内容与语言包，
    // 由人工 `cargo test -- --ignored -p alya ocr` 执行；不落盘：全程内存）
    // -----------------------------------------------------------------------

    /// 抓当前主屏左上角区域 → PNG 内存编码 → Auto 引擎识别 → 断言 Ok。
    /// 文本可能为空（纯桌面背景）——空文本为合法识别结果（FR-7.2 分支），本测试不断言非空。
    #[test]
    #[ignore = "真机专项：依赖实际显示内容与已装 OCR 语言包（清单 A6），人工执行"]
    fn smoke_recognize_captured_region_in_memory() {
        let screens = t_ok(screenshots::Screen::all());
        assert!(!screens.is_empty(), "应有至少一台显示器");
        let primary = screens
            .iter()
            .find(|s| s.display_info.is_primary)
            .unwrap_or(&screens[0]);
        let frame = t_ok(primary.capture_area(0, 0, 800, 300));
        let (w, h) = frame.dimensions();
        let rgba = image::RgbaImage::from_raw(w, h, frame.into_raw())
            .unwrap_or_else(|| panic!("RGBA 帧转换失败"));
        let mut png = std::io::Cursor::new(Vec::new());
        t_ok(rgba.write_to(&mut png, image::ImageFormat::Png));
        let text = t_ok(recognize_blocking(png.get_ref(), OcrLang::Auto));
        println!("OCR 冒烟识别字符数: {}", text.chars().count());
    }

    /// 枚举本机已装 OCR 语言包非空（语言包缺失时 EX-6 场景另测）。
    #[test]
    #[ignore = "真机专项：依赖已装 OCR 语言包（清单 A6），人工执行"]
    fn smoke_list_languages_non_empty() {
        let langs = list_ocr_languages_blocking();
        assert!(!langs.is_empty(), "本机应已装至少一个 OCR 语言包");
        let sample = &langs[0];
        assert!(!sample.tag.is_empty(), "tag 不得为空");
        assert!(!sample.label.is_empty(), "label 不得为空");
        println!(
            "已装 OCR 语言包（前五项）: {:?}",
            &langs[..langs.len().min(5)]
        );
    }
}
