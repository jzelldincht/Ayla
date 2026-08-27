//! capture：截图会话——多屏物理分辨率快照入内存、选区裁剪与 PNG 内存编码（T9 实现）。
//!
//! 对齐 spec 5.7.4/5.7.5/5.6.2 ②④：
//! - 抓帧时机在遮罩窗显示**之前**（遮罩永不被截入，D4）；
//! - 会话数据形态 `{sessionId, frames: HashMap<MonitorIndex, RgbaImage>, crops}`：框选期持各屏
//!   物理分辨率帧（INV-4：截屏坐标一律物理像素）；`crop_to_png` 用 `image::imageops::crop_imm`
//!   裁剪并以 PNG 编码入内存，随后**丢弃全部帧**（仅留裁剪块，存活至面板关闭/下次触发，spec 5.7.5）；
//! - **零落盘**（INV-3：截图位图不写任何临时文件；与 pot 落盘缓存刻意不同——参考库仅借鉴机制，
//!   无实质性同构代码）；位图本体不经任何 IPC 边界送往前端（数据流见 windows/mask.rs 的
//!   `get_mask_snapshot` 说明与 implementation-report T9 节）。
//!
//! 几何约定（INV-4）：`MonitorInfo` 为 tauri `Monitor`/screenshots `Screen` 的纯数据镜像——
//! origin/尺寸一律**物理像素**；显示器编号 = 按 (x, y) 排序后的位置（tauri 与 screenshots
//! 枚举序均不可依赖，用同一排序规则保证两侧编号一致）；选区矩形为该屏**局部**物理 px，
//! 全局 = 显示器 origin + 局部（负坐标副屏自然成立）。
//!
//! 错误纪律：本模块错误全部经 `AppError::Capture`（内部编码 "capture"，spec 5.8 无 EX 条目；
//! EX-11 专指安全桌面预期，与真实截屏失败不得混同），INV-6 成立。

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use image::{imageops, ImageFormat, RgbaImage};

use crate::error::AppError;
use crate::events::SelectionRect;

// ---------------------------------------------------------------------------
// Q11/FR-8.5 无效判定阈值（物理 px；spec 5.6.2 ③：位移 <4px 或 面积 <64px² 视同取消）
// ---------------------------------------------------------------------------

/// 无效判定阈值：拖动位移下限（物理 px）。释放时位移 < 此值 → 视同取消（FR-8.5/Q11）。
pub const MIN_DRAG_DISPLACEMENT_PX: u32 = 4;
/// 无效判定阈值：选区面积下限（物理 px²）。释放时面积 < 此值 → 视同取消（FR-8.5/Q11）。
pub const MIN_SELECTION_AREA_PX2: u64 = 64;

/// 无效判定纯函数（spec 5.6.2 ③；Q11 双条件 OR——位移 <4 物理px **或** 面积 <64 物理px²）。
/// 位移按矩形对角线度量（|dx|,|dy| 归一化后即 w/h 幅值，与拖拽方向无关）。
/// 面积/位移用 u128 防大矩形计算溢出（u32² ≈ 1.8e19 超出 u64 精度上限）；两条件独立判定。
pub fn is_valid_selection(rect: &SelectionRect) -> bool {
    let w = rect.width as u128;
    let h = rect.height as u128;
    let displacement_sq = w * w + h * h;
    let min_disp_sq = (MIN_DRAG_DISPLACEMENT_PX as u128) * (MIN_DRAG_DISPLACEMENT_PX as u128);
    let area = w * h;
    displacement_sq >= min_disp_sq && area >= MIN_SELECTION_AREA_PX2 as u128
}

// ---------------------------------------------------------------------------
// 显示器信息（INV-4：物理像素纯数据镜像，模块不依赖 tauri——可单测）
// ---------------------------------------------------------------------------

/// 一台显示器的几何事实源：origin + 尺寸 + 缩放，均为物理像素/系数（tauri Monitor 镜像）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorInfo {
    /// 按 (origin.x, origin.y) 排序后的编号（与 mask 窗 label `mask-{i}` 同源）。
    pub index: u32,
    /// 虚拟屏全局物理原点（副屏 x 可为负）。
    pub x: i32,
    pub y: i32,
    /// 物理分辨率宽。
    pub width: u32,
    /// 物理分辨率高。
    pub height: u32,
    /// 该屏缩放系数（>1 表示高 DPI）。
    pub scale_factor: f64,
}

/// 局部物理坐标 → 虚拟屏全局物理坐标（INV-4；origin 为显示器 origin，负坐标副屏自然成立）。
pub fn local_to_global(origin: (i32, i32), x: i32, y: i32) -> (i32, i32) {
    (origin.0 + x, origin.1 + y)
}

/// 虚拟屏全局物理坐标 → 局部物理坐标（`local_to_global` 逆运算）。
pub fn global_to_local(origin: (i32, i32), gx: i32, gy: i32) -> (i32, i32) {
    (gx - origin.0, gy - origin.1)
}

/// 选区矩形（局部物理 px，u32）左上角换算为全局物理坐标；宽高不变（INV-4）。
pub fn selection_rect_to_global(origin: (i32, i32), rect: &SelectionRect) -> (i32, i32, u32, u32) {
    let (gx, gy) = local_to_global(origin, rect.x as i32, rect.y as i32);
    (gx, gy, rect.width, rect.height)
}

// ---------------------------------------------------------------------------
// 抓帧抽象（trait 注入以便单测；生产实现走 screenshots crate）
// ---------------------------------------------------------------------------

/// 单显示器帧来源（生产 `ScreenshotsFrameSource`；测试注入假图）。
pub trait FrameSource {
    /// 抓取该显示器整屏物理分辨率 RGBA 帧（原始像素，未落盘）。
    fn capture_monitor(&self, info: &MonitorInfo) -> Result<RgbaImage, AppError>;
}

/// 生产实现：screenshots 0.8（底层 image 0.24 ↔ 本仓 image 0.25 经 raw 字节桥接——RGBA8
/// 内存布局一致，仅做尺寸+字节迁移，无解码/重编码）。
pub struct ScreenshotsFrameSource;

impl FrameSource for ScreenshotsFrameSource {
    fn capture_monitor(&self, info: &MonitorInfo) -> Result<RgbaImage, AppError> {
        // screenshots::Screen::all() 枚举序不可依赖：按 origin 匹配 tauri MonitorInfo。
        let screens = screenshots::Screen::all()
            .map_err(|e| AppError::Capture(format!("枚举显示器失败: {e}")))?;
        let screen = screens
            .iter()
            .find(|s| s.display_info.x == info.x && s.display_info.y == info.y)
            .ok_or_else(|| {
                AppError::Capture(format!(
                    "未找到 origin=({}, {}) 的显示器对应快照源",
                    info.x, info.y
                ))
            })?;
        let frame24 = screen
            .capture()
            .map_err(|e| AppError::Capture(format!("抓取屏幕失败: {e}")))?;
        // image 0.24 RgbaImage → image 0.25 RgbaImage（RGBA8 布局一致，纯字节桥接）
        let (w, h) = frame24.dimensions();
        let raw = frame24.into_raw();
        RgbaImage::from_raw(w, h, raw).ok_or_else(|| AppError::Capture("抓帧数据格式无效".into()))
    }
}

/// 依次抓取全部显示器帧（`monitors` 必须已按 (x,y) 排序且 index==位置）。
/// 任一屏失败即整体失败（不支持部分成功——会话要求各屏帧齐全）。
pub fn capture_all<S: FrameSource>(
    source: &S,
    monitors: &[MonitorInfo],
) -> Result<Vec<MonitorFrame>, AppError> {
    let mut frames = Vec::with_capacity(monitors.len());
    for info in monitors {
        let image = source.capture_monitor(info)?;
        frames.push(MonitorFrame { info: *info, image });
    }
    Ok(frames)
}

/// 一屏的抓帧结果（info 为几何事实源，image 为物理分辨率 RGBA 帧）。
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorFrame {
    pub info: MonitorInfo,
    pub image: RgbaImage,
}

// ---------------------------------------------------------------------------
// 会话生命周期（单活动会话：下次触发替换旧会话，spec 6.3 易失态）
// ---------------------------------------------------------------------------

/// 裁剪块（会话内存态）：PNG 编码字节 + 物理尺寸；reocr（FR-6.2）/送翻前 OCR 直接消费，
/// 不落盘（INV-3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CropBlock {
    pub png_bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// 活动截图会话（内部态）：框选期持各屏帧（无裁剪），确认后仅持裁剪块（帧已全部释放）。
struct ActiveSession {
    id: String,
    frames: HashMap<u32, MonitorFrame>,
    crops: HashMap<u32, CropBlock>,
}

/// 会话编号计数器（进程内单调，重启即重编——会话属易失态，spec 6.3）。
static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);
/// 全局单活动会话槽（一次框选＝一个会话；新触发替换旧的，spec 5.7.5）。
static ACTIVE_SESSION: OnceLock<Mutex<Option<ActiveSession>>> = OnceLock::new();

fn active_slot() -> &'static Mutex<Option<ActiveSession>> {
    ACTIVE_SESSION.get_or_init(|| Mutex::new(None))
}

/// 建立会话（替换旧会话——旧裁剪块随之释放，spec"存活至下一次截图触发"）。
/// 返回会话 id（前端后续经 complete/cancel/reocr 以 id 引用）。
pub fn begin_session(frames: Vec<MonitorFrame>) -> String {
    let id = format!("s{}", NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed));
    let mut by_index = HashMap::with_capacity(frames.len());
    for frame in frames {
        by_index.insert(frame.info.index, frame);
    }
    let mut slot = active_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *slot = Some(ActiveSession {
        id: id.clone(),
        frames: by_index,
        crops: HashMap::new(),
    });
    id
}

/// 活动会话 id（无活动会话 → None）。
pub fn active_session_id() -> Option<String> {
    let slot = active_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    slot.as_ref().map(|s| s.id.clone())
}

/// 指定 id 是否为当前活动会话（命令侧判断"过期请求"——已被新触发替换则忽略）。
pub fn session_is_current(session_id: &str) -> bool {
    let slot = active_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    slot.as_ref().is_some_and(|s| s.id == session_id)
}

/// 取活动会话中某屏的**整屏帧**并 PNG 编码为内存字节（供遮罩页 data: URL；一次性拉取，
/// 位图本体后续不经 IPC）。返回 `(session_id, png_bytes)`。
pub fn frame_png_bytes(monitor_index: u32) -> Result<(String, Vec<u8>), AppError> {
    let slot = active_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = slot
        .as_ref()
        .ok_or_else(|| AppError::Capture("没有活动的截图会话".into()))?;
    let frame = session
        .frames
        .get(&monitor_index)
        .ok_or_else(|| AppError::Capture(format!("会话内无第 {monitor_index} 屏帧")))?;
    let png = encode_png_in_memory(&frame.image)?;
    Ok((session.id.clone(), png))
}

/// 按选区矩形从会话帧裁剪 → PNG 内存编码 → 存入会话 `crops`，并**释放全部屏帧**
/// （spec 5.7.5：仅留裁剪块）。rect 为该屏局部物理 px（INV-4），越界自动钳制到帧边界。
pub fn crop_to_png(
    session_id: &str,
    monitor_index: u32,
    rect: &SelectionRect,
) -> Result<CropBlock, AppError> {
    let mut slot = active_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = slot
        .as_mut()
        .filter(|s| s.id == session_id)
        .ok_or_else(|| AppError::Capture("截图会话不存在或已被替换".into()))?;
    let frame = session
        .frames
        .get(&monitor_index)
        .ok_or_else(|| AppError::Capture(format!("会话内无第 {monitor_index} 屏帧")))?;
    let (img_w, img_h) = frame.image.dimensions();
    let x = rect.x.min(img_w);
    let y = rect.y.min(img_h);
    let w = rect.width.min(img_w - x);
    let h = rect.height.min(img_h - y);
    if w == 0 || h == 0 {
        return Err(AppError::Capture("裁剪区域为空（选区越界或零尺寸）".into()));
    }
    let cropped = imageops::crop_imm(&frame.image, x, y, w, h).to_image();
    let png_bytes = encode_png_in_memory(&cropped)?;
    let block = CropBlock {
        png_bytes,
        width: w,
        height: h,
    };
    // 仅保留裁剪块：全部屏帧释放（含本屏——裁剪已完成）
    session.frames.clear();
    session.crops.insert(monitor_index, block.clone());
    Ok(block)
}

/// 取消/关闭会话（Esc、无效选区、面板关闭）。返回是否确实移除了会话（id 不匹配＝已被新会话
/// 替换 → false，调用方不得再隐藏遮罩，见 mask.rs 命令语义）。
pub fn cancel_session(session_id: &str) -> bool {
    let mut slot = active_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match slot.as_ref() {
        Some(s) if s.id == session_id => {
            *slot = None;
            true
        }
        _ => false,
    }
}

/// 取会话裁剪块（供 reocr/send_ocr_to_translate，FR-6.2；内存克隆，块仍然保留在会话内）。
pub fn get_crop(session_id: &str) -> Result<CropBlock, AppError> {
    let slot = active_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = slot
        .as_ref()
        .filter(|s| s.id == session_id)
        .ok_or_else(|| AppError::Capture("截图会话不存在或已被替换".into()))?;
    session
        .crops
        .iter()
        .next()
        .map(|(_, block)| block.clone())
        .ok_or_else(|| AppError::Capture("截图会话尚无裁剪块（尚未确认选区）".into()))
}

/// PNG 内存编码（零落盘；write_to 直接写 Cursor<Vec<u8>>）。
fn encode_png_in_memory(image: &RgbaImage) -> Result<Vec<u8>, AppError> {
    let mut cursor = Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| AppError::Capture(format!("PNG 内存编码失败: {e}")))?;
    Ok(cursor.into_inner())
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用帧源（可编程尺寸与像素填充）。
    struct FakeFrameSource;

    impl FrameSource for FakeFrameSource {
        fn capture_monitor(&self, info: &MonitorInfo) -> Result<RgbaImage, AppError> {
            Ok(test_image(info.width, info.height))
        }
    }

    /// 生成可校验图案的测试图：像素 (x,y) = (x, y, 0, 255)（R=x, G=y, B=0, A=255）。
    fn test_image(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([(x & 0xFF) as u8, (y & 0xFF) as u8, 0, 255])
        })
    }

    fn rect(x: u32, y: u32, w: u32, h: u32) -> SelectionRect {
        SelectionRect {
            x,
            y,
            width: w,
            height: h,
        }
    }

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

    /// 全局单活动会话槽为进程级共享态，cargo 并行测试会相互覆盖——涉会话的用例统一串行化
    /// （守卫借出至用例结束；纯数学用例不取）。
    static SESSION_TEST_LOCK: Mutex<()> = Mutex::new(());
    fn session_guard() -> std::sync::MutexGuard<'static, ()> {
        SESSION_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    // -----------------------------------------------------------------------
    // INV-4：局部 ↔ 全局换算（含负坐标副屏）
    // -----------------------------------------------------------------------

    #[test]
    fn local_to_global_primary_monitor() {
        let origin = (0, 0);
        assert_eq!(local_to_global(origin, 100, 50), (100, 50));
        assert_eq!(global_to_local(origin, 100, 50), (100, 50));
    }

    #[test]
    fn local_to_global_negative_origin_secondary_monitor() {
        // 副屏在主屏左侧：origin.x = -1920
        let origin = (-1920, 0);
        assert_eq!(local_to_global(origin, 100, 50), (-1820, 50));
        assert_eq!(local_to_global(origin, 0, 0), (-1920, 0));
        // 逆运算
        assert_eq!(global_to_local(origin, -1820, 50), (100, 50));
        assert_eq!(global_to_local(origin, -1920, 0), (0, 0));
        // 负坐标副屏幕内任意点往返一致
        for (lx, ly) in [(0, 0), (32, 1080), (1919, 55)] {
            let (gx, gy) = local_to_global(origin, lx, ly);
            assert_eq!(global_to_local(origin, gx, gy), (lx, ly), "往返应恒等");
        }
    }

    #[test]
    fn local_to_global_vertical_stack_negative_y() {
        // 副屏在主屏上方：origin.y = -1080
        let origin = (0, -1080);
        assert_eq!(local_to_global(origin, 321, 0), (321, -1080));
        // 屏内点：全局 y = -1080 + 10 → 局部 y = 10
        assert_eq!(global_to_local(origin, 321, -1080 + 10), (321, 10));
        // 上缘外 10px：局部为负（几何数学如实表达，不做越界钳制——越界由裁剪/前端处理）
        assert_eq!(global_to_local(origin, 321, -1080 - 10), (321, -10));
    }

    #[test]
    fn selection_rect_to_global_keeps_local_origin_math() {
        // 负坐标副屏：矩形局部 (5, 8, 640, 480) → 全局左上 (-1915, 8)，尺寸不变
        let (gx, gy, w, h) = selection_rect_to_global((-1920, 0), &rect(5, 8, 640, 480));
        assert_eq!((gx, gy, w, h), (-1915, 8, 640, 480));
    }

    // -----------------------------------------------------------------------
    // Q11 无效判定阈值边界
    // -----------------------------------------------------------------------

    #[test]
    fn invalid_selection_zero_or_tiny() {
        assert!(!is_valid_selection(&rect(0, 0, 0, 0)), "零尺寸无效");
        assert!(!is_valid_selection(&rect(0, 0, 1, 1)));
        // 位移 <4px：2x2（位移²=8 <16）且面积 4 <64
        assert!(!is_valid_selection(&rect(0, 0, 2, 2)));
        // 位移 =3px（3x1：位移²=10<16）
        assert!(!is_valid_selection(&rect(0, 0, 3, 1)));
    }

    #[test]
    fn invalid_selection_min_area_boundary() {
        // 面积下边界：7x9=63 <64 → 无效（位移充足）
        assert!(!is_valid_selection(&rect(0, 0, 7, 9)));
        // 64px² 恰为下界含：8x8=64 → 有效（位移²=128 ≥16）
        assert!(is_valid_selection(&rect(0, 0, 8, 8)));
        // 1x64：面积=64 且位移²=4097 ≥16 → 有效（细长条不算无效——Q11 只判面积与位移）
        assert!(is_valid_selection(&rect(0, 0, 1, 64)));
    }

    #[test]
    fn invalid_selection_tiny_drag_columnar() {
        // 位移足够但面积 0（单列/单行拖拽）→ 面积条件判失效
        assert!(!is_valid_selection(&rect(0, 0, 0, 100)));
        assert!(!is_valid_selection(&rect(0, 0, 100, 0)));
        // 正常矩形（位移、面积双满足）
        assert!(is_valid_selection(&rect(0, 0, 100, 100)));
        // u64 抗溢出：超大矩形数学不溢出
        assert!(is_valid_selection(&SelectionRect {
            x: 0,
            y: 0,
            width: u32::MAX,
            height: u32::MAX
        }));
    }

    // -----------------------------------------------------------------------
    // 裁剪正确性
    // -----------------------------------------------------------------------

    #[test]
    fn crop_to_png_dimensions_and_pixels() {
        let _guard = session_guard();
        let frame = MonitorFrame {
            info: MonitorInfo {
                index: 0,
                x: 0,
                y: 0,
                width: 100,
                height: 60,
                scale_factor: 1.0,
            },
            image: test_image(100, 60),
        };
        let id = begin_session(vec![frame]);
        let block = t_ok(crop_to_png(&id, 0, &rect(10, 20, 30, 15)));
        assert_eq!((block.width, block.height), (30, 15));

        // 解码回位图校验像素（用本仓 image 解码内存字节，零落盘校验）
        let decoded = t_ok(image::load_from_memory(&block.png_bytes));
        let rgba = decoded.to_rgba8();
        assert_eq!(rgba.dimensions(), (30, 15));
        // 局部 (0,0) 对应原图 (10,20)：R=10, G=20
        let p = rgba.get_pixel(0, 0);
        assert_eq!(p.0, [10, 20, 0, 255]);
        // 局部 (29,14) 对应原图 (39,34)
        let p = rgba.get_pixel(29, 14);
        assert_eq!(p.0, [39, 34, 0, 255]);
    }

    #[test]
    fn crop_clamped_at_frame_edge() {
        let _guard = session_guard();
        let frame = MonitorFrame {
            info: MonitorInfo {
                index: 0,
                x: 0,
                y: 0,
                width: 100,
                height: 60,
                scale_factor: 1.0,
            },
            image: test_image(100, 60),
        };
        let id = begin_session(vec![frame.clone()]);
        // 越界：x=90 w=20 → 钳制到 w=10；y=55 h=20 → 钳制到 h=5
        let block = t_ok(crop_to_png(&id, 0, &rect(90, 55, 20, 20)));
        assert_eq!((block.width, block.height), (10, 5));
        // 完全越界（x>=宽）→ 显式错误（不返回空块）。首次裁剪已释放帧——用新会话验证
        let id2 = begin_session(vec![frame]);
        let err = t_err(crop_to_png(&id2, 0, &rect(100, 0, 10, 10)));
        assert!(err.to_string().contains("裁剪区域为空"));
    }

    #[test]
    fn crop_releases_frames_keeps_crop() {
        let _guard = session_guard();
        let frame = MonitorFrame {
            info: MonitorInfo {
                index: 0,
                x: 0,
                y: 0,
                width: 10,
                height: 10,
                scale_factor: 1.0,
            },
            image: test_image(10, 10),
        };
        let id = begin_session(vec![frame]);
        // 确认选区（裁剪）；随后整帧不应再可取（帧已释放，仅留裁剪块）
        let block = t_ok(crop_to_png(&id, 0, &rect(0, 0, 10, 10)));
        assert_eq!((block.width, block.height), (10, 10));
        assert!(frame_png_bytes(0).is_err(), "帧应已释放（仅留裁剪块）");
        // 裁剪块可取
        let crop = t_ok(get_crop(&id));
        assert_eq!((crop.width, crop.height), (10, 10));
        assert!(!crop.png_bytes.is_empty());
    }

    #[test]
    fn session_lifecycle_begin_crop_cancel_replace() {
        let _guard = session_guard();
        let frame = MonitorFrame {
            info: MonitorInfo {
                index: 0,
                x: 0,
                y: 0,
                width: 10,
                height: 10,
                scale_factor: 1.0,
            },
            image: test_image(10, 10),
        };
        let id = begin_session(vec![frame]);
        assert_eq!(active_session_id().as_deref(), Some(id.as_str()));
        assert!(session_is_current(&id), "活动会话应判定为当前");
        // 取消 → 会话清空
        assert!(cancel_session(&id));
        assert_eq!(active_session_id(), None);
        assert!(!session_is_current(&id), "取消后不再是当前会话");
        assert!(
            !cancel_session(&id),
            "重复取消幂等（返回 false 表示无匹配）"
        );

        // 新会话替换旧会话（id 不同；旧 id 不能再取消）
        let f2 = MonitorFrame {
            info: MonitorInfo {
                index: 1,
                x: 1920,
                y: 0,
                width: 10,
                height: 10,
                scale_factor: 1.0,
            },
            image: test_image(10, 10),
        };
        let id2 = begin_session(vec![f2]);
        assert_ne!(id, id2);
        assert_eq!(active_session_id().as_deref(), Some(id2.as_str()));
        assert!(
            !cancel_session(&id),
            "旧会话已被替换——取消请求应不使新会话受影响"
        );
        assert_eq!(active_session_id().as_deref(), Some(id2.as_str()));
    }

    #[test]
    fn session_errors_when_no_active() {
        let _guard = session_guard();
        // 起点自清洁：无论此前是否有并行用例遗留会话，先按当前活动 id 取消（幂等），
        // 保证无活动会话断言不依赖其他用例的清理行为（测试隔离，主 Agent 反馈项）。
        if let Some(current) = active_session_id() {
            cancel_session(&current);
        }
        assert!(frame_png_bytes(0).is_err());
        let err = t_err(crop_to_png("s-missing", 0, &rect(0, 0, 10, 10)));
        assert_eq!(err.ex_code(), "capture", "会话缺失为内部 capture 错误");
        assert!(get_crop("s-missing").is_err());
    }

    #[test]
    fn crop_for_wrong_monitor_index_fails() {
        let _guard = session_guard();
        let frame = MonitorFrame {
            info: MonitorInfo {
                index: 2,
                x: 0,
                y: 0,
                width: 10,
                height: 10,
                scale_factor: 1.0,
            },
            image: test_image(10, 10),
        };
        let id = begin_session(vec![frame]);
        let err = t_err(crop_to_png(&id, 5, &rect(0, 0, 10, 10)));
        assert!(err.to_string().contains("第 5 屏"));
    }

    // -----------------------------------------------------------------------
    // capture_all 走虚假帧源（不触屏）
    // -----------------------------------------------------------------------

    #[test]
    fn capture_all_maps_indices_in_order() {
        let monitors = [
            MonitorInfo {
                index: 0,
                x: -1920,
                y: 0,
                width: 8,
                height: 8,
                scale_factor: 1.0,
            },
            MonitorInfo {
                index: 1,
                x: 0,
                y: 0,
                width: 4,
                height: 4,
                scale_factor: 1.5,
            },
        ];
        let frames = t_ok(capture_all(&FakeFrameSource, &monitors));
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].info.index, 0);
        assert_eq!(frames[1].info.index, 1);
        assert_eq!(frames[0].image.dimensions(), (8, 8));
        assert_eq!(frames[1].image.dimensions(), (4, 4));
    }
}
