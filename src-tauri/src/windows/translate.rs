//! translate 窗（label=translate）：翻译面板窗口生命周期与定位（T11 实现，spec 5.5/5.7.2/5.7.3）。
//!
//! 职责：
//! - **暖窗预创建**（D5/J1 前提）：启动序列末尾 `init()` 创建并隐藏（无边框、透明、阴影、
//!   skip_taskbar、初始不可见；C3：topmost 于 show 置位、隐藏后清除）；常驻复用，隐藏≠销毁。
//! - **统一显示路径** `show_panel`：place_remembered_rect（真机修复轮 13g 用户需求，取代
//!   spec 5.7.2 光标锚点定位）——记忆位置（`panel.posXLogical/posYLogical`，用户拖动/磁吸
//!   松手后防抖保存）优先并钳回所在屏工作区；**无记忆 → 默认贴屏幕右侧垂直居中** →
//!   show + set_focus → `panel://show` 统一发射 → 打点 `panel_visible`（7.2）。
//! - **失焦隐藏**（FR-9.2/5.7.3）：`Focused(false)` 且 `panel.hideOnBlur` 开且无压制条件
//!   （框选进行中=遮罩可见 / onboarding 可见；T16 的引导期压制同样经 onboarding 可见投影）→
//!   隐藏；隐藏**不取消**进行中翻译（D10）；隐藏幂等（重复 hide 无副作用）。
//! - **尺寸调整**（FR-9.3/A4）：`resizable(true)`（无边框窗在 Windows 上保留原生隐形调尺寸
//!   边框＝四边四角热区，约系统边框宽度）＋ `Resized` 事件防抖（500ms 松手后）持久化逻辑宽高
//!   （`panel.widthLogical/heightLogical`，INV-4 存逻辑值抗 DPI 变更），并在持久化时按当前
//!   显示器工作区上限与最小 380×300 钳制回写。
//! - **尺寸上限**（真机修复轮 13 起；design-spec 3.1 的"工作区 80%"废弃）：最小 380×300、
//!   宽度上限＝工作区宽 − 左右各 8px、高度上限＝工作区高 − 顶部 8px（**底部贴死工作区底边**，
//!   即磁吸到任务栏上沿）——show 前、resize 持久化、内容自适应（`panel_resize_for_content`）
//!   均经 `clamp_logical_size` 钳制（cap < 最小尺寸时 cap 优先，保证绝不出屏）。
//! - **译文内容自适应高度**（真机修复轮 13 用户需求）：前端测量译文不滚动所需窗口总高，
//!   经 `panel_resize_for_content` 应用＝max(用户偏好高度, 内容所需) 钳到 [最小, 所在位置上限
//!   （工作区底 − 窗顶，贴底无空带）]；窗口贴底时底边钉住、顶边上移（等效向上生长）。内容驱动
//!   的变更**不持久化**（用户拖拽的偏好尺寸不被长文撑大覆盖，每次 show 回到偏好值）。
//!
//! 依赖方向：windows/translate.rs → config/events/windows::mask（spec 5.4 禁止反向）。
//! 定位/钳制为纯函数（`place_panel`/`clamp_logical_size`），单测覆盖 spec 测试策略点名项：
//! 四角溢出翻转、负坐标副屏、屏小于面板、100%/150% 换算。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_notification::NotificationExt;

use crate::config;
use crate::error::AppError;
use crate::events::{PanelShowPayload, EVENT_PANEL_SHOW, WINDOW_TRANSLATE_LABEL};

// ---------------------------------------------------------------------------
// 规格常量（design-spec 3.1/3.4 + spec 5.7.2）
// ---------------------------------------------------------------------------

/// 面板最小尺寸（逻辑像素；真机修复轮 9 按用户反馈从 360×240 上调——
/// 360 宽下顶栏关闭按钮贴住窗口圆角被视觉裁切，380×300 留出呼吸空间）。
pub const PANEL_MIN_WIDTH_LOGICAL: u32 = 380;
pub const PANEL_MIN_HEIGHT_LOGICAL: u32 = 300;

/// 面板最大尺寸（真机修复轮 13 用户需求"拖到多大就是多大，上限即磁吸到 Windows 边界"；
/// design-spec 3.1 原定工作区 80%，自本轮起废弃）：
/// - 宽度上限＝工作区宽 − 左右各 `PANEL_MAX_EDGE_MARGIN_LOGICAL`；
/// - 高度上限＝工作区高 − 顶部 `PANEL_MAX_EDGE_MARGIN_LOGICAL`（**底部 0**——窗口底边贴死
///   工作区底边即任务栏上沿，消除"贴底拖大后卡片与任务栏之间的空带"）。
pub const PANEL_MAX_EDGE_MARGIN_LOGICAL: f64 = WORK_MARGIN_LOGICAL;

/// 锚点偏移（逻辑像素；design-spec 3.4 第 2/6 条：偏移量按逻辑像素定义，各屏按缩放换算；
/// spec 5.7.2 缺省值为 +16,+16——任务书明确按设计规范的逻辑 px 口径执行）。
pub const ANCHOR_OFFSET_LOGICAL: f64 = 16.0;

/// 工作区四周安全边距（逻辑像素，design-spec 3.4 第 4 条"四周保留 8px 安全边距"；
/// 与偏移同列为逻辑语义，保证 100%/150% 下视觉一致，INV-4）。
pub const WORK_MARGIN_LOGICAL: f64 = 8.0;

/// 尺寸持久化防抖：松手判定（Resized 静默后可认为拖拽结束）。
pub const RESIZE_PERSIST_DWELL: std::time::Duration = std::time::Duration::from_millis(500);

/// 出厂初始逻辑宽高（仅建窗时占位；每次 show 前都会按持久值重定位——默认值权威源见 config 模块）。
const DEFAULT_INNER_SIZE_LOGICAL: (f64, f64) = (460.0, 340.0);

// ---------------------------------------------------------------------------
// 纯几何类型（局部定义——与 tauri 的 Monitor 解耦，便于单测）
// ---------------------------------------------------------------------------

/// 显示器工作区（物理像素；不含任务栏，spec 5.7.2 第 4 条"以所在显示器工作区为界"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl WorkArea {
    pub fn right(&self) -> i32 {
        self.x + self.width as i32
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.height as i32
    }
    /// 以逻辑像素描述的工作区尺寸（缩放换算，INV-4）。
    fn logical_size(&self, scale: f64) -> (u32, u32) {
        (
            ((self.width as f64) / scale).round() as u32,
            ((self.height as f64) / scale).round() as u32,
        )
    }
}

/// 面板矩形（物理像素）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

// ---------------------------------------------------------------------------
// 纯算法：尺寸钳制 + 定位（spec 5.7.2 第 3~5 步 + design-spec 3.4）
// ---------------------------------------------------------------------------

/// 面板逻辑尺寸钳制（真机修复轮 13 起，design-spec 3.1 的 80% 上限废弃）：
/// - 最小 380×300（不足抬升到最小）；
/// - 宽度上限＝工作区宽 − 左右各 8px；**高度上限＝工作区高（顶/底零边距——
///   真机修复轮 13e：磁吸/拖拽到工作区整高时原样保留，不被 −8px 上限"回弹"出空带；
///   满高时 y 钳到 0 贴顶贴底）**；
/// - 上限 < 最小尺寸的极端窄屏：**上限优先**（绝不出屏的硬约束优先于最小尺寸）。
///
/// `work_w/work_h` 为工作区**逻辑像素**尺寸（调用方负责按所在屏 scale 换算）。
pub fn clamp_logical_size(width: u32, height: u32, work_w: u32, work_h: u32) -> (u32, u32) {
    let cap_w = cap_w_len(work_w);
    let w = std::cmp::min(std::cmp::max(width, PANEL_MIN_WIDTH_LOGICAL), cap_w);
    let h = std::cmp::min(std::cmp::max(height, PANEL_MIN_HEIGHT_LOGICAL), work_h);
    (w, h)
}

/// 工作区宽度 → 面板宽度上限（左右各减 `PANEL_MAX_EDGE_MARGIN_LOGICAL`；防下溢取 0 下界）。
fn cap_w_len(work_w: u32) -> u32 {
    ((work_w as f64) - 2.0 * PANEL_MAX_EDGE_MARGIN_LOGICAL).round().max(0.0) as u32
}

/// 持久化钳制（真机修复轮 13e）：与 `clamp_logical_size` 同构，但高度上限由调用方按窗口
/// 顶边位置推导（`工作区底 − 顶边`）——系统磁吸贴满工作区（顶=工作区顶）时上限=整工作区高，
/// 尺寸原样保留，不被压回（消除磁吸后"回弹出空带"）。极端小屏上限优先口径一致。
pub fn clamp_persist_size(width: u32, height: u32, work_w: u32, cap_h: u32) -> (u32, u32) {
    let w = std::cmp::min(
        std::cmp::max(width, PANEL_MIN_WIDTH_LOGICAL),
        cap_w_len(work_w),
    );
    let h = std::cmp::min(std::cmp::max(height, PANEL_MIN_HEIGHT_LOGICAL), cap_h);
    (w, h)
}

fn scale_len(logical: u32, scale: f64) -> u32 {
    ((logical as f64) * scale).round().max(1.0) as u32
}

fn clamp_i(value: i32, lo: i32, hi: i32) -> i32 {
    if hi < lo {
        lo
    } else {
        value.clamp(lo, hi)
    }
}

/// 面板定位纯函数（spec 5.7.2 统一算法 + design-spec 3.4）：
/// 1. 锚点（鼠标物理坐标）→ 期望位置 = 锚点 + (+16,+16) 逻辑偏移 × scale；
/// 2. 右溢出 → 面板右上角贴锚点左侧（偏移 −16），下溢出 → 面板底边在锚点上方（−16），
///    角部溢出 → 双轴同时翻转；C2 裁决：无系统选区矩形能力，选区避让退化为鼠标锚点规则
///    （即上述首选位本身，无额外禁区步）；
/// 3. 仍有溢出 → 工作区内钳制（顶部/左右 8px 逻辑边距；**底边贴死工作区底边**——
///    真机修复轮 13c，磁吸边界=任务栏上沿）；
/// 4. 归属显示器工作区固定由调用方传入（含负坐标副屏），矩形绝不越出该工作区。
pub fn place_panel(
    anchor_x: f64,
    anchor_y: f64,
    logical_width: u32,
    logical_height: u32,
    scale: f64,
    work: WorkArea,
) -> PanelRect {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let (logical_work_w, logical_work_h) = work.logical_size(scale);
    let (lw, lh) = clamp_logical_size(
        logical_width,
        logical_height,
        logical_work_w,
        logical_work_h,
    );

    let panel_w = scale_len(lw, scale) as i32;
    let panel_h = scale_len(lh, scale) as i32;
    let offset = (ANCHOR_OFFSET_LOGICAL * scale).round() as i32;
    let margin = (WORK_MARGIN_LOGICAL * scale).round() as i32;

    let ax = anchor_x.round() as i32;
    let ay = anchor_y.round() as i32;
    let right_limit = work.right() - margin; // 允许的右边缘上界（物理）
    // 底边贴死工作区底边（真机修复轮 13c：呼出路径与拖拽/内容自适应同口径——磁吸边界即
    // 任务栏上沿，消除"呼出后卡片与任务栏之间 8px 空带"；顶部仍留 8px 呼吸）。
    let bottom_limit = work.bottom();

    // 首选位：锚点向右下偏移（+16,+16）
    let mut x = ax + offset;
    let mut y = ay + offset;

    // 右溢出 → 面板整体移到锚点左侧（右上角对齐锚点左侧，偏移 −16）
    if x + panel_w > right_limit {
        x = ax - offset - panel_w;
    }
    // 下溢出 → 面板整体移到锚点上方（底边在锚点上方，偏移 −16）
    if y + panel_h > bottom_limit {
        y = ay - offset - panel_h;
    }
    // 钳制（8px 边距；翻转后仍溢出或触碰边缘时贴回工作区，绝不跨屏）。
    x = clamp_i(x, work.x + margin, right_limit - panel_w);
    // y 下界放至工作区顶（真机修复轮 13e）：满高面板（h=工作区高）时 y 必须=0 贴顶，
    // 旧下界 work.y+margin 会把满高窗口推下 8px 越过工作区底边（压住任务栏）。
    y = clamp_i(y, work.y, bottom_limit - panel_h);

    PanelRect {
        x,
        y,
        width: panel_w as u32,
        height: panel_h as u32,
    }
}

// ---------------------------------------------------------------------------
// 全局窗口访问与监听状态
// ---------------------------------------------------------------------------

/// resize 防抖代次：每收到一次 `Resized` 递增；持久化任务启动前记录代次，
/// 等 DWELL 后若代次未再变（＝松手），才执行一次持久化。
static RESIZE_GENERATION: AtomicU64 = AtomicU64::new(0);

/// 面板 WebView 页面加载完成标志（真机修复轮 1：防止内容未就绪时显示"透明黑窗口"）。
static PANEL_PAGE_LOADED: AtomicBool = AtomicBool::new(false);

/// 拖拽失焦宽限期（真机修复轮 5）。实测证明：`start_dragging()` **立即返回**（仅代理至主线程），
/// 其引发的 2 次虚假 `Focused(false)` 在返回**之后**异步到达（同秒内）——"拖拽期间置标志"
/// 无法覆盖（修复轮 4 无效的原因）。改为记录拖拽开始时刻，失焦处理时判断是否仍在宽限期内。
static PANEL_DRAG_UNTIL_MS: AtomicU64 = AtomicU64::new(0);

/// 拖拽宽限期长度：虚假失焦实测在 start_dragging 返回后 ~50ms 内到达，取 500ms 裕量。
/// 真实拖拽期间窗口持捕获不产生真实失焦；宽限期误压制真实失焦的代价仅为隐藏延迟半秒。
const DRAG_BLUR_GRACE_MS: u64 = 500;

/// 失焦复核延迟（真机修复轮 7）：模态循环虚假失焦在窗口仍持前台时到达，延迟后以
/// Win32 前台真值复核；真实失焦（点击其他窗口）同样延迟该时长后隐藏（体感无差）。
const BLUR_RECHECK_DELAY_MS: u64 = 150;

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 拖拽宽限期内？（拖拽开始后 DRAG_BLUR_GRACE_MS 毫秒内）。
fn drag_blur_in_grace() -> bool {
    let until = PANEL_DRAG_UNTIL_MS.load(Ordering::Relaxed);
    until != 0 && now_unix_ms() < until + DRAG_BLUR_GRACE_MS
}

/// 显示前等待页面就绪的上限（超时仍显示，避免极端加载失败下永久无窗）。
const PAGE_READY_WAIT_MAX: Duration = Duration::from_secs(3);

fn get_panel(app: &AppHandle) -> Result<WebviewWindow, AppError> {
    app.get_webview_window(WINDOW_TRANSLATE_LABEL)
        .ok_or_else(|| AppError::Window("translate 窗尚未创建（init 未执行）".to_string()))
}

// ---------------------------------------------------------------------------
// 启动预创建（D5 暖窗；lib.rs 启动序列第 6 步调用）
// ---------------------------------------------------------------------------

/// 启动序列末尾预创建 translate 窗并隐藏（D5 暖窗，J1 前提；幂等：已存在则跳过）。
pub fn init(app: &AppHandle) -> Result<(), AppError> {
    if app.get_webview_window(WINDOW_TRANSLATE_LABEL).is_some() {
        return Ok(());
    }
    create_window(app)?;
    log::debug!("translate: 暖窗已预创建并隐藏（label={WINDOW_TRANSLATE_LABEL}）");
    Ok(())
}

fn create_window(app: &AppHandle) -> Result<WebviewWindow, AppError> {
    let app_clone = app.clone();
    WebviewWindowBuilder::new(
        app,
        WINDOW_TRANSLATE_LABEL,
        WebviewUrl::App(PathBuf::from("src/translate/index.html")),
    )
    // i18n-1：建窗标题按当前语言（后续 show 时刷新，语言切换即时生效）。
    .title(crate::ui_text::resolve_global().panel_window_title())
    .inner_size(DEFAULT_INNER_SIZE_LOGICAL.0, DEFAULT_INNER_SIZE_LOGICAL.1)
    .min_inner_size(
        PANEL_MIN_WIDTH_LOGICAL as f64,
        PANEL_MIN_HEIGHT_LOGICAL as f64,
    )
    .decorations(false)
    .transparent(true)
    // 2026-08-28 用户反馈：transparent 圆角窗 + 系统 DWM 阴影会在圆角外透出方形阴影线；
    // 关闭系统阴影，阴影感由 CSS 宿主自行处理（真机修复轮 1）。
    .shadow(false)
    .skip_taskbar(true)
    // 无边框 + resizable：Windows 上保留系统隐形调尺寸边框（四边四角热区，FR-9.3）；
    // 松手后由 Resized 防抖持久化逻辑尺寸（本模块 on_window_event）。
    .resizable(true)
    // C3 裁决：topmost 于 show 置位、隐藏后清除；建窗期恒为 false。
    .always_on_top(false)
    // D5 暖窗：初始不可见，show 由统一显示路径执行。
    .visible(false)
    // 2026-08-28 用户反馈（真机修复轮 1）：面板曾在 WebView 内容未就绪时被显示，
    // 呈现"透明黑窗口"。on_page_load Finished 置位就绪标志；show_panel 显示前等待该
    // 标志（上限 3s，超时仍显示——内容随后到达，避免永久黑窗）。
    .on_page_load(|_, payload| {
        if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
            PANEL_PAGE_LOADED.store(true, std::sync::atomic::Ordering::Relaxed);
            log::debug!("translate: 页面加载完成（面板显示门控放行）");
        }
    })
    .build()
    .map_err(|e| AppError::Window(format!("translate 窗创建失败: {e}")))
    .inspect(|window| {
        // tauri 2：WebviewWindowBuilder 无 on_window_event——建窗成功后经窗口句柄订阅
        // （WindowEvent 监听为"不阻塞构建"的附加回调，失败仅记日志，不阻断启动）。
        window.on_window_event(move |event| match event {
            WindowEvent::Focused(false) => on_focus_lost(&app_clone),
            WindowEvent::Resized(size) => on_resized(&app_clone, *size),
            WindowEvent::Moved(_) => on_moved(&app_clone),
            _ => {}
        });
    })
}

// ---------------------------------------------------------------------------
// 统一显示路径（spec 5.7.3："显示路径统一走 place_at_cursor + show + set_focus"）
// ---------------------------------------------------------------------------

/// 统一显示：定位（place_at_cursor）→ 尺寸/位置落位 → topmost 置位（C3）→ show → set_focus →
/// 统一发射 `panel://show` → 打点 `panel_visible`（7.2）。
/// T16 action_router 的五个动作入口均须经本函数显示面板（禁止散落 show 调用）。
pub fn show_panel(app: &AppHandle, payload: PanelShowPayload) -> Result<(), AppError> {
    let window = get_panel(app)?;
    let rect = place_remembered_rect(app)?;

    // 真机修复轮 1：等待 WebView 首次加载完成再显示（冷启动即触发时避免"透明黑窗口"）。
    // 就绪标志一旦置位永久有效（暖窗常驻复用，仅首个页面加载）；超时兜底仍显示。
    if !PANEL_PAGE_LOADED.load(Ordering::Relaxed) {
        let started = Instant::now();
        while !PANEL_PAGE_LOADED.load(Ordering::Relaxed) {
            if started.elapsed() >= PAGE_READY_WAIT_MAX {
                log::warn!("translate: 页面加载等待超时（{PAGE_READY_WAIT_MAX:?}），强制显示面板");
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    // 真机修复轮 13g：定位走 tracked（Moved 事件消费豁免计数——复位到记忆位置不得被
    // 误当作"用户拖动"覆盖保存的位置）。
    if let Err(e) = set_position_tracked(&window, rect.x, rect.y) {
        return Err(AppError::Window(format!("translate 定位失败: {e}")));
    }
    window
        .set_size(PhysicalSize::new(rect.width, rect.height))
        .map_err(|e| AppError::Window(format!("translate 尺寸设置失败: {e}")))?;
    // i18n-1：标题按当前语言刷新（建窗后语言可能已切换）。
    let _ = window.set_title(crate::ui_text::resolve_global().panel_window_title());
    // C3：show 置位 topmost（弹出瞬间置顶一次，非常驻）。
    window
        .set_always_on_top(true)
        .map_err(|e| AppError::Window(format!("translate topmost 置位失败: {e}")))?;
    window
        .show()
        .map_err(|e| AppError::Window(format!("translate 显示失败: {e}")))?;
    // D7 前台握手拒绝时的降级路径（spec 5.7.9【A8】：面板置前但无焦点 + 通知 + 日志）。
    // AllowSetForegroundWindow 握手成功后此调用应成功（second instance 授权）；失败时
    // 面板仍已置前（topmost 置位 + show），仅无法获得输入焦点——通知引导用户点击面板。
    if let Err(err) = window.set_focus() {
        log::warn!("translate: 面板显示成功但未获得焦点（D7 握手持手失败/被系统拒绝）: {err}");
        if let Err(notify_err) = app
            .notification()
            .builder()
            .title("Alya")
            .body(crate::ui_text::resolve_global().focus_fallback_notify_body())
            .show()
        {
            log::error!("translate: 无焦点降级通知发送失败: {notify_err}");
        }
    }

    // 统一发射（6.2：{mode, text?, sessionId?}）；发射失败＝面板已不可用，收回并报错。
    if let Err(err) = app.emit_to(WINDOW_TRANSLATE_LABEL, EVENT_PANEL_SHOW, payload) {
        let _ = hide_panel(app);
        return Err(AppError::Window(format!("panel://show 发射失败: {err}")));
    }
    log::debug!(
        "panel_visible: 面板已显示（{}x{} 物理 @ {},{})",
        rect.width,
        rect.height,
        rect.x,
        rect.y
    );
    Ok(())
}

/// 面板定位（真机修复轮 13g 用户需求，取代 spec 5.7.2 光标锚点）：
/// 记忆位置（`panel.posXLogical/posYLogical`，用户上次拖动/磁吸后松手保存）优先，
/// 经目标屏工作区钳制复位；**无记忆（从未拖动过）→ 默认贴屏幕右侧垂直居中**。
/// 目标屏＝光标所在显示器（含负坐标副屏；获取失败回落主屏）。
pub fn place_remembered_rect(app: &AppHandle) -> Result<PanelRect, AppError> {
    let cursor = app.cursor_position().ok().map(|p| (p.x, p.y));
    let (_anchor_x, _anchor_y, work, scale) = resolve_anchor(app, cursor)?;
    let settings = config::read_global()?;
    let saved = match (
        settings.panel.pos_x_logical,
        settings.panel.pos_y_logical,
    ) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
    };
    Ok(place_remembered(
        saved,
        settings.panel.width_logical,
        settings.panel.height_logical,
        scale,
        work,
    ))
}

/// 记忆定位纯函数：尺寸经 `clamp_logical_size` 钳制；
/// - 有记忆：逻辑坐标 × 该屏 scale → 钳回工作区（左右 8px 边距；顶/底零边距贴边合法）；
/// - 无记忆：贴屏幕右侧（右缘留 8px）、垂直居中。
pub fn place_remembered(
    saved: Option<(i32, i32)>,
    logical_width: u32,
    logical_height: u32,
    scale: f64,
    work: WorkArea,
) -> PanelRect {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let (logical_work_w, logical_work_h) = work.logical_size(scale);
    let (lw, lh) = clamp_logical_size(
        logical_width,
        logical_height,
        logical_work_w,
        logical_work_h,
    );
    let panel_w = scale_len(lw, scale) as i32;
    let panel_h = scale_len(lh, scale) as i32;
    let margin = (WORK_MARGIN_LOGICAL * scale).round() as i32;

    let (x, y) = match saved {
        Some((sx, sy)) => {
            let px = (sx as f64 * scale).round() as i32;
            let py = (sy as f64 * scale).round() as i32;
            (
                clamp_i(px, work.x + margin, work.right() - margin - panel_w),
                clamp_i(py, work.y, work.bottom() - panel_h),
            )
        }
        None => {
            let x = work.right() - margin - panel_w;
            let y = work.y + ((work.height as i32 - panel_h) / 2).max(0);
            (x, y)
        }
    };
    PanelRect {
        x,
        y,
        width: panel_w as u32,
        height: panel_h as u32,
    }
}

/// 解析锚点所在显示器工作区与缩放：
/// 1. 光标位置存在且 `monitor_from_point` 命中（含负坐标副屏）→ 该屏工作区；
/// 2. 未命中 → 遍历 `available_monitors` 找包含该点的屏（兜底，覆盖 DPI 混合边界竞态）；
/// 3. 光标位置获取失败/仍无归属 → **主屏工作区中心**（spec 5.7.2 第 1 步回落）。
fn resolve_anchor(
    app: &AppHandle,
    cursor: Option<(f64, f64)>,
) -> Result<(f64, f64, WorkArea, f64), AppError> {
    if let Some((x, y)) = cursor {
        if let Ok(Some(monitor)) = app.monitor_from_point(x, y) {
            let work = work_area_of(&monitor);
            return Ok((x, y, work, monitor.scale_factor()));
        }
        let monitors = app
            .available_monitors()
            .map_err(|e| AppError::Window(format!("枚举显示器失败: {e}")))?;
        if let Some(monitor) = monitors
            .iter()
            .find(|m| monitor_contains(m, x, y))
            .or_else(|| monitors.first())
        {
            let work = work_area_of(monitor);
            return Ok((x, y, work, monitor.scale_factor()));
        }
    }
    // 回落主屏（无主屏则首台显示器；仍无 → 应用级错误）。
    let primary = app
        .primary_monitor()
        .map_err(|e| AppError::Window(format!("主显示器查询失败: {e}")))?
        .or_else(|| {
            app.available_monitors()
                .ok()
                .and_then(|mut list| (!list.is_empty()).then(|| list.remove(0)))
        })
        .ok_or_else(|| AppError::Window("无可用显示器".to_string()))?;
    let work = work_area_of(&primary);
    // 锚点=工作区中心（面板随后的翻转/钳制将把面板收在主屏工作区中心附近）。
    let center_x = work.x as f64 + work.width as f64 / 2.0;
    let center_y = work.y as f64 + work.height as f64 / 2.0;
    Ok((center_x, center_y, work, primary.scale_factor()))
}

fn monitor_contains(monitor: &tauri::Monitor, x: f64, y: f64) -> bool {
    let pos = monitor.position();
    let size = monitor.size();
    x >= pos.x as f64
        && x < (pos.x + size.width as i32) as f64
        && y >= pos.y as f64
        && y < (pos.y + size.height as i32) as f64
}

fn work_area_of(monitor: &tauri::Monitor) -> WorkArea {
    let wa = monitor.work_area();
    WorkArea {
        x: wa.position.x,
        y: wa.position.y,
        width: wa.size.width,
        height: wa.size.height,
    }
}

/// 隐藏面板（幂等；重复隐藏无副作用）并清除 topmost（C3：隐藏后清除，非常驻置顶）。
/// **不取消**进行中翻译（D10）。
pub fn hide_panel(app: &AppHandle) -> Result<(), AppError> {
    let window = get_panel(app)?;
    let visible = window.is_visible().unwrap_or(true);
    if visible {
        window
            .hide()
            .map_err(|e| AppError::Window(format!("translate 隐藏失败: {e}")))?;
    }
    // C3：清除 topmost（置位仅存在于 show 后、隐藏前窗口寿命段；重复清除无副作用）。
    window
        .set_always_on_top(false)
        .map_err(|e| AppError::Window(format!("translate topmost 清除失败: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 失焦隐藏（FR-9.2，spec 5.7.3 交汇矩阵）
// ---------------------------------------------------------------------------

/// 面板隐藏命令（6.1；FR-1.3 关闭=隐藏）。真机修复轮 2：前端直调 `getCurrentWindow().hide()`
/// 被 Tauri ACL 拒绝（core:window:default 不含 allow-hide）且 catch 静默，表现为"关闭无反应"；
/// 统一改走自定义命令（与 hide_panel 同一实现，语义不重复）。
#[tauri::command]
pub fn panel_hide(app: tauri::AppHandle) -> Result<(), String> {
    hide_panel(&app).map_err(|e| e.to_string())
}

/// 顶栏拖拽命令（真机修复轮 2：`startDragging()` 同被 ACL 拒绝，改走自定义命令）。
/// 真机修复轮 5：`start_dragging` 立即返回、虚假失焦异步后至——以"拖拽宽限期"取代
/// "拖拽期间标志"（见 PANEL_DRAG_UNTIL_MS 注释与诊断记录）。
#[tauri::command]
pub fn panel_start_drag(window: tauri::WebviewWindow) -> Result<(), String> {
    log::debug!("translate: panel_start_drag 进入（开启失焦宽限期）");
    PANEL_DRAG_UNTIL_MS.store(now_unix_ms(), Ordering::Relaxed);
    let result = window.start_dragging().map_err(|e| e.to_string());
    log::debug!("translate: start_dragging 返回（err={}）", result.is_err());
    result
}

// ---------------------------------------------------------------------------
// 译文内容自适应高度（真机修复轮 13 用户需求：译文区随内容自动展开，上限为
// "当前屏幕位置所能达到的最大窗口高"＝工作区底边 − 窗口顶边 − 8px 安全边距；
// 超出即回落为译文区内滚动）
// ---------------------------------------------------------------------------

/// 内容自适应逻辑高度的待定豁免计数：每次程序化 set_size 前自增，对应 `Resized` 事件到达时
/// 消费一枚并跳过持久化（内容驱动的临时生长不得覆盖用户拖拽的偏好尺寸——否则一次长文
/// 会把偏好高度永久撑大，短译文时窗口又空又占屏）。计数而非布尔的兜底：事件合并/乱序时
/// 至多豁免多一枚，下一次真实手动拖拽照常持久化（自愈）。
static AUTO_RESIZE_PENDING: AtomicU32 = AtomicU32::new(0);

// ---------------------------------------------------------------------------
// 位置记忆（真机修复轮 13g 用户需求：Alt+E 记住用户最后位置；未拖动过 → 默认贴屏幕右侧）
// ---------------------------------------------------------------------------

/// 位置持久化防抖代次：Moved 静默 400ms 视为拖动/磁吸结束，保存一次位置。
static MOVE_PERSIST_GENERATION: AtomicU64 = AtomicU64::new(0);
const MOVE_PERSIST_DWELL: Duration = Duration::from_millis(400);

/// 程序化 set_position 豁免计数（show 复位到记忆位置、内容自适应钳位等）：对应 `Moved`
/// 事件跳过位置持久化——复位不是用户拖动，不得覆盖已保存的位置。计数口径同 AUTO_RESIZE_PENDING。
static AUTO_MOVE_PENDING: AtomicU32 = AtomicU32::new(0);

/// 程序化改位置（豁免 Moved 持久化）；失败回滚计数。
fn set_position_tracked(
    window: &WebviewWindow,
    x: i32,
    y: i32,
) -> Result<(), tauri::Error> {
    AUTO_MOVE_PENDING.fetch_add(1, Ordering::Relaxed);
    let result = window.set_position(PhysicalPosition::new(x, y));
    if result.is_err() {
        AUTO_MOVE_PENDING.fetch_sub(1, Ordering::Relaxed);
    }
    result
}

/// `Moved`：程序化复位 → 消费豁免；用户拖动/磁吸 → 静默 400ms 后持久化逻辑位置。
fn on_moved(app: &AppHandle) {
    if AUTO_MOVE_PENDING
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            (n > 0).then(|| n - 1)
        })
        .is_ok()
    {
        return;
    }
    let generation = MOVE_PERSIST_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(MOVE_PERSIST_DWELL).await;
        if MOVE_PERSIST_GENERATION.load(Ordering::Relaxed) != generation {
            return; // 仍在移动
        }
        if let Err(err) = persist_panel_position(&app) {
            log::warn!("translate: 面板位置持久化失败: {err}");
        }
    });
}

/// 保存逻辑位置（÷ 所在屏 scale，INV-4）；值未变跳过落盘。
fn persist_panel_position(app: &AppHandle) -> Result<(), AppError> {
    let window = get_panel(app)?;
    let scale = window.scale_factor().unwrap_or(1.0);
    let pos = window
        .outer_position()
        .map_err(|e| AppError::Window(format!("translate 位置读取失败: {e}")))?;
    let x = ((pos.x as f64) / scale).round() as i32;
    let y = ((pos.y as f64) / scale).round() as i32;
    let dir = config::app_config_dir(app)?;
    {
        let mut guard = config::write_global()?;
        if guard.panel.pos_x_logical == Some(x) && guard.panel.pos_y_logical == Some(y) {
            return Ok(());
        }
        guard.panel.pos_x_logical = Some(x);
        guard.panel.pos_y_logical = Some(y);
    }
    let settings = config::read_global()?;
    config::save_to(&dir, &settings)?;
    log::debug!("translate: 面板位置已持久化（逻辑 {x},{y}）");
    Ok(())
}

/// 内容自适应应用高度（纯函数）：max(内容所需, 用户偏好, 最小) 再钳上限；上限更小时上限优先
/// （绝不出屏硬约束与 clamp_logical_size 同一口径）。
pub fn fit_height_logical(needed: u32, preferred: u32, cap: u32) -> u32 {
    let h = std::cmp::max(std::cmp::max(needed, preferred), PANEL_MIN_HEIGHT_LOGICAL);
    std::cmp::min(h, cap)
}

/// 生长后的纵坐标钳制（纯函数）：顶边有空间则保持（向下生长）；底边越界则底边**贴死工作区
/// 底边**（等效向上生长）。下界放至工作区顶（真机修复轮 13e）：满高窗口 y=0 贴顶，
/// 不得被旧下界（顶+8px 边距）推下越过工作区底边压住任务栏。
pub fn clamp_y_after_growth(y: i32, height_phys: u32, work: WorkArea) -> i32 {
    let max_y = work.bottom() - height_phys as i32;
    std::cmp::max(std::cmp::min(y, max_y), work.y)
}

/// `panel_resize_for_content`（真机修复轮 13 新增命令）：前端测量"译文不滚动所需的窗口总高"
/// （CSS 逻辑像素，含顶栏/原文区等常驻 chrome），应用窗口高度＝
/// `fit_height_logical(needed, panel.heightLogical, cap)`，其中 cap 由窗口当前位置与所在
/// 显示器工作区现场推导（工作区底 − 窗顶，底部贴死；窗口贴底时底边钉住、顶边上移）。
/// 仅改高度不改宽度；**不持久化**（AUTO_RESIZE_PENDING 豁免）；窗口不可见时直接忽略
/// （呼出时 show 会重置偏好尺寸）。
#[tauri::command]
pub fn panel_resize_for_content(
    window: tauri::WebviewWindow,
    content_height: f64,
) -> Result<(), String> {
    if !window.is_visible().unwrap_or(false) {
        return Ok(());
    }
    let needed = if content_height.is_finite() && content_height > 0.0 {
        content_height.round() as u32
    } else {
        return Ok(()); // 测量异常（0/负/NaN）：保持现状，下次内容变更再校准
    };
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let preferred = config::read_global()
        .map(|s| s.panel.height_logical)
        .unwrap_or(PANEL_MIN_HEIGHT_LOGICAL);

    let pos = window.outer_position().map_err(|e| e.to_string())?;
    let monitor = window.current_monitor().map_err(|e| e.to_string())?;
    // cap（逻辑）：工作区底边 − 窗口顶边（贴顶=整工作区高；底部贴死无空带——磁吸边界即
    // 任务栏上沿）；无显示器信息时不设上限（仅最小/偏好钳制）。
    let (applied_logical, work_opt) = match &monitor {
        Some(m) => {
            let work = work_area_of(m);
            let cap_logical =
                (((work.bottom() - pos.y) as f64) / scale).floor().max(0.0) as u32;
            (fit_height_logical(needed, preferred, cap_logical), Some(work))
        }
        None => (fit_height_logical(needed, preferred, u32::MAX), None),
    };

    let applied_phys = scale_len(applied_logical, scale);
    let current = window.outer_size().map_err(|e| e.to_string())?;
    if current.height != applied_phys {
        AUTO_RESIZE_PENDING.fetch_add(1, Ordering::Relaxed);
        if let Err(err) = window.set_size(PhysicalSize::new(current.width, applied_phys)) {
            AUTO_RESIZE_PENDING.fetch_sub(1, Ordering::Relaxed);
            return Err(err.to_string());
        }
        // 高度变化后位置复钳：向下放得下保持顶边；放不下底边钉住（等效向上生长）。
        // tracked：程序化复位不触发位置持久化（13g）。
        if let Some(work) = work_opt {
            let new_y = clamp_y_after_growth(pos.y, applied_phys, work);
            if new_y != pos.y {
                let _ = set_position_tracked(&window, pos.x, new_y);
            }
        }
        log::debug!(
            "translate: 内容自适应高度（逻辑 {applied_logical}，内容所需 {needed}，偏好 {preferred}）"
        );
    }
    Ok(())
}

/// `Focused(false)`：hideOnBlur 开 且 无压制条件 → 隐藏。
/// 真机修复轮 7：**延迟 + 前台核验**。实证（修复轮 5 诊断）表明拖拽/缩放的系统模态循环
/// 会发出虚假 `Focused(false)`，而窗口实际仍是前台活动窗口（用户实测：缩放面板时面板被
/// 误隐藏）。因此收到事件后不立即隐藏，延迟 BLUR_RECHECK_DELAY_MS 后以 Win32 前台窗口
/// 真值复核：仍在前台 → 不隐藏；确已失焦（用户点击了其他窗口）→ 隐藏。
fn on_focus_lost(app: &AppHandle) {
    let hide_on_blur = config::read_global()
        .map(|s| s.panel.hide_on_blur)
        .unwrap_or(true);
    if !hide_on_blur {
        return;
    }
    log::debug!("translate: Focused(false) 到达（{BLUR_RECHECK_DELAY_MS}ms 后前台核验）");
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(BLUR_RECHECK_DELAY_MS)).await;
        // 抑制条件与焦点状态均以延时后的实时值为准（拖拽/框选/引导可能在此期间开始）。
        if is_hide_suppressed(&app) {
            return;
        }
        let Ok(window) = get_panel(&app) else {
            return;
        };
        if !window.is_visible().unwrap_or(false) {
            return; // 已隐藏（幂等）
        }
        if panel_is_foreground(&window) {
            log::debug!("translate: 模态循环虚假失焦（仍为前台窗口）→ 不隐藏");
            return;
        }
        if let Err(err) = hide_panel(&app) {
            log::error!("translate: 失焦隐藏失败: {err}");
        }
    });
}

/// 面板是否为系统前台窗口（Win32 真值；查询失败按"仍在前台"处理——宁可延迟隐藏也不误隐藏）。
fn panel_is_foreground(window: &tauri::WebviewWindow) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    match window.hwnd() {
        Ok(hwnd) => {
            // SAFETY：GetForegroundWindow 无特殊约束（读取系统前台窗口句柄）。
            let fg = unsafe { GetForegroundWindow() };
            fg.0 as isize == hwnd.0 as isize
        }
        Err(_) => true,
    }
}

fn is_hide_suppressed(app: &AppHandle) -> bool {
    // 拖拽宽限期内（真机修复轮 5：start_dragging 的虚假 Focused(false) 异步后至）。
    if decide_hide_suppressed(false, drag_blur_in_grace()) {
        return true;
    }
    // 引导窗可见（T14 建窗后按可见性判定；引导期压制的面板本身也处于被隐藏态）。
    if let Some(onboarding) = app.get_webview_window("onboarding") {
        if onboarding.is_visible().unwrap_or(false) {
            return true;
        }
    }
    // 框选进行中（spec 5.7.3：面板已在触发时强制隐藏，此判定防御散落状态漂移）。
    crate::windows::mask::any_visible()
}

/// 压制决策纯函数（可单测）：`dragging` 任一为真即压制失焦隐藏。
fn decide_hide_suppressed(onboarding_visible: bool, dragging: bool) -> bool {
    onboarding_visible || dragging
}

// ---------------------------------------------------------------------------
// 尺寸调整持久化（FR-9.3/A4：四形态共用单一记忆值；松手持久化逻辑宽高）
// ---------------------------------------------------------------------------

fn on_resized(app: &AppHandle, size: PhysicalSize<u32>) {
    // 内容自适应的程序化 set_size 到达：消费一枚豁免计数，跳过持久化（不覆盖用户偏好尺寸）。
    if AUTO_RESIZE_PENDING
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| (n > 0).then(|| n - 1))
        .is_ok()
    {
        return;
    }
    let generation = RESIZE_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(RESIZE_PERSIST_DWELL).await;
        if RESIZE_GENERATION.load(Ordering::Relaxed) != generation {
            return; // 期间又有新 Resized（仍在拖拽）→ 等松手后的最后一代
        }
        if let Err(err) = persist_panel_size(&app, size) {
            log::warn!("translate: 面板尺寸持久化失败: {err}");
        }
    });
}

/// 松手持久化：物理 → 逻辑（÷ 所在屏 scale，INV-4）→ 钳制（最小 380×300；宽−左右 8px；
/// **高＝工作区底 − 窗口顶（位置感知，真机修复轮 13e）**——系统磁吸贴满工作区的整高
/// （顶=工作区顶）原样保留，不再被固定"−8px"上限压回出空带）→ 若被钳制先回写窗口 →
/// 写 config（值未变则跳过落盘）。内容自适应的变更不经过本函数
/// （AUTO_RESIZE_PENDING 豁免——临时生长不覆盖用户偏好尺寸）。
fn persist_panel_size(app: &AppHandle, physical: PhysicalSize<u32>) -> Result<(), AppError> {
    let window = get_panel(app)?;
    let scale = window.scale_factor().unwrap_or(1.0);
    let wa = match window.current_monitor() {
        Ok(Some(monitor)) => Some(work_area_of(&monitor)),
        Ok(None) => None,
        Err(_) => None,
    };
    let top_phys = window
        .outer_position()
        .map(|p| p.y)
        .unwrap_or_else(|_| wa.map(|w| w.y).unwrap_or(0));
    let logical_w = ((physical.width as f64) / scale).round() as u32;
    let logical_h = ((physical.height as f64) / scale).round() as u32;
    // 位置感知上限：窗口顶边在工作区内的可用高；顶边越低可用越高，贴顶=整工作区高。
    let (work_w_logical, cap_h) = match wa {
        Some(w) => {
            let top_clamped = top_phys.clamp(w.y, w.bottom());
            let cap = (((w.bottom() - top_clamped) as f64) / scale).floor() as u32;
            (w.logical_size(scale).0, cap)
        }
        None => (u32::MAX / 2, u32::MAX / 2), // 无显示器信息：仅最小尺寸钳制
    };
    let (lw, lh) = clamp_persist_size(logical_w, logical_h, work_w_logical, cap_h);

    // 上限钳制回写窗口（把超上限的窗口物理尺寸压回）
    let new_physical_w = scale_len(lw, scale);
    let new_physical_h = scale_len(lh, scale);
    if new_physical_w != physical.width || new_physical_h != physical.height {
        window
            .set_size(PhysicalSize::new(new_physical_w, new_physical_h))
            .map_err(|e| AppError::Window(format!("translate 尺寸钳制回写失败: {e}")))?;
    }

    let dir = config::app_config_dir(app)?;
    {
        let mut guard = config::write_global()?;
        if guard.panel.width_logical == lw && guard.panel.height_logical == lh {
            return Ok(()); // 值未变（含 DPI 变更前后等价）：无需落盘
        }
        guard.panel.width_logical = lw;
        guard.panel.height_logical = lh;
    }
    let settings = config::read_global()?;
    config::save_to(&dir, &settings)?;
    log::debug!(
        "translate: 面板尺寸已持久化（逻辑 {lw}x{lh}，physical {}x{}，scale={scale}）",
        physical.width,
        physical.height
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 单元测试：定位与钳制纯算法（spec 测试策略点名：四角/负坐标/小屏/100%/150%）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const W: WorkArea = WorkArea {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };

    fn ok_of<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(err) => panic!("期望成功值，实际错误: {err}"),
        }
    }

    /// 矩形是否完全位于工作区（含 8px 逻辑边距折让的物理边界）内——绝不跨屏。
    fn assert_within_work_area(rect: PanelRect, work: WorkArea, scale: f64) {
        let margin = (WORK_MARGIN_LOGICAL * scale).round() as i32;
        assert!(
            rect.x >= work.x + margin && rect.y >= work.y + margin,
            "矩形左/上越出工作区: {rect:?} in {work:?}"
        );
        assert!(
            rect.x + rect.width as i32 <= work.x + work.width as i32 - margin,
            "矩形右越出工作区: {rect:?} in {work:?}"
        );
        assert!(
            rect.y + rect.height as i32 <= work.y + work.height as i32 - margin,
            "矩形下越出工作区: {rect:?} in {work:?}"
        );
    }

    #[test]
    fn offset_is_logical_and_scales_with_dpi() {
        // 100%：偏移 16 逻辑 px = 16 物理 px；150%：24 物理 px。
        // 面板 460×340 逻辑 → 150% 下物理 690×510。
        let at_100 = place_panel(100.0, 100.0, 460, 340, 1.0, W);
        assert_eq!(
            at_100,
            PanelRect {
                x: 116,
                y: 116,
                width: 460,
                height: 340
            }
        );

        let at_150 = place_panel(100.0, 100.0, 460, 340, 1.5, W);
        assert_eq!(
            at_150,
            PanelRect {
                x: 124,
                y: 124,
                width: 690,
                height: 510
            },
            "150% 下偏移与面板物理尺寸应随 scale 换算（INV-4 视觉一致）"
        );
    }

    #[test]
    fn preferred_position_kept_when_fits() {
        let rect = place_panel(100.0, 100.0, 460, 340, 1.0, W);
        assert_eq!(
            (rect.x, rect.y),
            (116, 116),
            "首选位=锚点+(16,16)，无翻转无钳制"
        );
        assert_within_work_area(rect, W, 1.0);
    }

    #[test]
    fn right_overflow_flips_to_left_of_anchor() {
        // 锚点右侧仅剩 120px（面板 460 → 首选位右缘 2316 远超 1912）→ 反转面板到锚点左侧。
        let rect = place_panel(1800.0, 100.0, 460, 340, 1.0, W);
        assert_eq!(
            (rect.x, rect.y),
            (1324, 116),
            "右翻转：面板右上角贴锚点左侧（偏移 −16）"
        );
        assert_eq!(rect.x + rect.width as i32, 1784, "右缘=锚点 −16");
        assert_within_work_area(rect, W, 1.0);
    }

    #[test]
    fn bottom_overflow_flips_above_anchor() {
        let rect = place_panel(100.0, 900.0, 460, 340, 1.0, W);
        assert_eq!(
            (rect.x, rect.y),
            (116, 544),
            "下翻转：面板底边在锚点上方（−16）"
        );
        assert_eq!(rect.y + rect.height as i32, 884, "底缘=锚点 −16");
        assert_within_work_area(rect, W, 1.0);
    }

    #[test]
    fn corner_overflow_flips_both_axes() {
        // 右下角锚点：双轴同时翻转（design-spec 3.4 角部）。
        let rect = place_panel(1800.0, 900.0, 460, 340, 1.0, W);
        assert_eq!((rect.x, rect.y), (1324, 544));
        assert_eq!(rect.x + rect.width as i32, 1784);
        assert_eq!(rect.y + rect.height as i32, 884);
        assert_within_work_area(rect, W, 1.0);
    }

    #[test]
    fn negative_coordinate_secondary_monitor_keeps_panel_on_that_screen() {
        // 副屏在主屏左侧（x 从 -1920 到 0）：锚点在副屏内，面板不得落回主屏。
        let left = WorkArea {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
        };

        // 锚点距副屏右缘（0）仅 100px：面板 460 首选位右缘 376 越界会侵入主屏 →
        // 必须右翻转（面板整体移到锚点左侧，绝不跨屏）。
        let rect = place_panel(-100.0, 100.0, 460, 340, 1.0, left);
        assert_eq!((rect.x, rect.y), (-576, 116), "右翻转：右缘=锚点 −16");
        assert_eq!(rect.x + rect.width as i32, -116);
        assert_within_work_area(rect, left, 1.0);

        // 右缘贴近 0：首选位右缘 376 > -8 → 右翻转，仍停副屏左侧。
        let rect = place_panel(-200.0, 100.0, 460, 340, 1.0, left);
        assert_eq!((rect.x, rect.y), (-676, 116), "翻转后不得越过 0 进入主屏");
        assert_within_work_area(rect, left, 1.0);

        // 左下角：双轴翻转。
        let rect = place_panel(-100.0, 900.0, 460, 340, 1.0, left);
        assert_eq!((rect.x, rect.y), (-576, 544));
        assert_within_work_area(rect, left, 1.0);

        // 副屏最左缘：首选位（锚点 +16）本就在工作区内（右缘 −1429 < −8），无翻转无钳制。
        let rect = place_panel(-1905.0, 110.0, 460, 340, 1.0, left);
        assert_eq!((rect.x, rect.y), (-1889, 126));
        assert_within_work_area(rect, left, 1.0);

        // 锚点紧贴右缘（x=-5）：首选位右缘 471 已越界 → 翻转，且翻转后右缘 −16 仍在工作区内。
        let rect = place_panel(-5.0, 110.0, 460, 340, 1.0, left);
        assert_eq!((rect.x, rect.y), (-481, 126));
        assert_within_work_area(rect, left, 1.0);
    }

    #[test]
    fn dpi150_on_negative_secondary_monitor() {
        // 150% 副屏（物理工作区不变，换算在逻辑层）：面板 690×510 物理，偏移 24。
        let left = WorkArea {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
        };
        // 锚点距右缘 100px：面板 690 首选位右缘 614 越界 → 右翻转（−24 偏移起算）。
        let rect = place_panel(-100.0, 100.0, 460, 340, 1.5, left);
        assert_eq!((rect.x, rect.y), (-814, 124));
        assert_eq!((rect.width, rect.height), (690, 510));
        assert_within_work_area(rect, left, 1.5);
        // 右缘不够 → 翻转后仍留在副屏。
        let rect = place_panel(-150.0, 100.0, 460, 340, 1.5, left);
        assert_eq!(rect.x, -150 - 24 - 690);
        assert_within_work_area(rect, left, 1.5);
    }

    #[test]
    fn panel_larger_than_small_screen_clamps_to_cap_and_fits() {
        // 小屏物理 400×300（100%）：宽上限=400−16=384、高上限=300−8=292 < 出厂 460×340 →
        // 钳到上限；贴底定位（真机修复轮 13：底部不留空带，磁吸边界=任务栏上沿）。
        let small = WorkArea {
            x: 0,
            y: 0,
            width: 400,
            height: 300,
        };
        let (w, h) = clamp_logical_size(460, 340, 400, 300);
        assert_eq!((w, h), (384, 300), "屏小于面板：宽−16、高=工作区高（零边距）优先");
        let rect = place_panel(200.0, 150.0, 460, 340, 1.0, small);
        assert_eq!((rect.width, rect.height), (384, 300));
        // 满高（=工作区高 300）：y 钳到工作区顶 0，底边 300=工作区底（贴顶贴底，无空带）。
        assert_eq!(rect.y, 0);
        assert_eq!(rect.y + rect.height as i32, small.bottom());
    }

    #[test]
    fn clamp_size_min_and_cap_boundaries() {
        // 正常大屏：出厂 460×340 原样保留。
        assert_eq!(clamp_logical_size(460, 340, 2000, 1500), (460, 340));
        // 小于最小 → 抬升到最小尺寸（真机修复轮 9 起 380×300；断言引用常量防再漂移）。
        assert_eq!(
            clamp_logical_size(100, 100, 2000, 1500),
            (PANEL_MIN_WIDTH_LOGICAL, PANEL_MIN_HEIGHT_LOGICAL)
        );
        // 超大 → 宽压到工作区−16、高=工作区高（磁吸整高原样保留；真机修复轮 13e）。
        assert_eq!(clamp_logical_size(2000, 1500, 1000, 800), (984, 800));
        // 0（配置异常/早期值）→ 最小。
        assert_eq!(
            clamp_logical_size(0, 0, 2000, 1500),
            (PANEL_MIN_WIDTH_LOGICAL, PANEL_MIN_HEIGHT_LOGICAL)
        );
        // 极端窄屏（工作区高 < 最小高）→ 高 cap 优先。
        assert_eq!(clamp_logical_size(460, 340, 300, 200), (284, 200));
    }

    #[test]
    fn fit_height_prefers_content_then_preference_then_cap() {
        // 内容所需 > 偏好 → 随内容展开。
        assert_eq!(fit_height_logical(900, 340, 2000), 900);
        // 内容所需 < 偏好 → 保持用户偏好（短译文不收缩到内容高）。
        assert_eq!(fit_height_logical(320, 340, 2000), 340);
        // 内容所需与偏好均低于最小 → 最小兜底（偏好异常小时的防御路径）。
        assert_eq!(fit_height_logical(10, 200, 2000), PANEL_MIN_HEIGHT_LOGICAL);
        // 上限压回（贴底无空带即极限）。
        assert_eq!(fit_height_logical(2000, 340, 1200), 1200);
        // 上限 < 最小（极端小屏）→ 上限优先。
        assert_eq!(fit_height_logical(900, 340, 100), 100);
    }

    #[test]
    fn clamp_y_after_growth_keeps_top_or_pins_bottom() {
        let work = WorkArea {
            x: 0,
            y: 0,
            width: 1000,
            height: 1000,
        };
        // 顶边有空间：保持原 y（向下生长）。
        assert_eq!(clamp_y_after_growth(100, 500, work), 100);
        // 底边越界：底边贴死工作区底边（等效向上生长，无空带）。
        assert_eq!(clamp_y_after_growth(600, 500, work), 1000 - 500);
        // 满高（=工作区高 1000）：y=0 贴顶，底边=工作区底（不被顶边距推下压任务栏；13e）。
        assert_eq!(clamp_y_after_growth(100, 1000, work), 0);
        assert_eq!(0 + 1000, work.bottom(), "满高：贴顶且底边=工作区底边");
    }

    #[test]
    fn persist_clamp_keeps_snapped_full_height_and_shrinks_floating() {
        // 真机修复轮 13e 场景：系统磁吸贴满工作区（顶=工作区顶 → cap=整工作区高），
        // 952 原样保留——不再被 −8px 上限"回弹"出空带。
        // 工作区 1600×952 逻辑；窗口 800×952 贴顶 → cap_h=952。
        assert_eq!(clamp_persist_size(800, 952, 1600, 952), (800, 952));
        // 浮动小窗（顶边 y=8 逻辑 → cap=944）：944 原样；952 压回 944（防越界）。
        assert_eq!(clamp_persist_size(800, 944, 1600, 944), (800, 944));
        assert_eq!(clamp_persist_size(800, 952, 1600, 944), (800, 944));
        // 小于最小 → 抬升；宽超 → 压到工作区−16。
        assert_eq!(
            clamp_persist_size(100, 100, 1600, 944),
            (PANEL_MIN_WIDTH_LOGICAL, PANEL_MIN_HEIGHT_LOGICAL)
        );
        assert_eq!(clamp_persist_size(2000, 944, 1600, 944), (1584, 944));
    }

    #[test]
    fn place_remembered_defaults_to_right_centered_without_save() {
        // 真机修复轮 13g：无记忆（从未拖动）→ 贴屏幕右侧（右缘 8px）、垂直居中。
        let work = WorkArea {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        };
        let rect = place_remembered(None, 460, 340, 1.0, work);
        assert_eq!(rect.x, 1920 - 8 - 460, "右缘留 8px 边距");
        assert_eq!(rect.y, (1040 - 340) / 2, "垂直居中");
        assert_within_work_area(rect, work, 1.0);
    }

    #[test]
    fn place_remembered_restores_saved_position_clamped_into_work() {
        let work = WorkArea {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        };
        // 记忆位置原样恢复。
        let rect = place_remembered(Some((100, 50)), 460, 340, 1.0, work);
        assert_eq!((rect.x, rect.y), (100, 50));
        // 记忆越界（换屏/改分辨率后）→ 钳回工作区（左右 8px；顶/底零边距）。
        let rect = place_remembered(Some((-5000, 9999)), 460, 340, 1.0, work);
        assert_eq!(rect.x, 8);
        assert_eq!(rect.y, 1040 - 340, "底部贴工作区底边");
        // 满高记忆（磁吸整高，y=0）→ 原样保留（贴顶合法）。
        let rect = place_remembered(Some((800, 0)), 460, 1040, 1.0, work);
        assert_eq!(rect.y, 0);
        assert_eq!(rect.y + rect.height as i32, work.bottom());
    }

    #[test]
    fn sweep_anchors_never_cross_monitor_boundary() {
        // 属性测试：全屏扫锚点（含 100%/150%），矩形永远完整落在所在工作区内（8px 边距）。
        for scale in [1.0_f64, 1.5] {
            let work = WorkArea {
                x: -1920,
                y: -150,
                width: 3840,
                height: 1390,
            };
            for sx in 0..=20 {
                for sy in 0..=12 {
                    let ax = -1920.0 + (3840.0 * sx as f64 / 20.0);
                    let ay = -150.0 + (1390.0 * sy as f64 / 12.0);
                    let rect = place_panel(ax, ay, 460, 340, scale, work);
                    assert_within_work_area(rect, work, scale);
                }
            }
        }
    }

    #[test]
    fn resolve_anchor_center_fallback_pure_geometry() {
        // make_work_area 中心锚点被 place_panel 收进工作区（模拟主屏中心回落路径的纯几何部分）。
        let primary = WorkArea {
            x: 0,
            y: 0,
            width: 2560,
            height: 1440,
        };
        let (cx, cy) = (primary.width as f64 / 2.0, primary.height as f64 / 2.0);
        let rect = place_panel(cx, cy, 460, 340, 1.0, primary);
        assert_within_work_area(rect, primary, 1.0);
        // 中心锚点默认落在中心偏右下（+16,+16），不翻转不钳制。
        assert_eq!((rect.x, rect.y), (1296, 736));
    }

    #[test]
    fn clamp_size_respects_scale_of_work_area() {
        // 同一逻辑工作区（1500×1000 物理 @150% = 1000×666.67 逻辑）：
        // cap=800×533；460×340 原样保留。
        let work = WorkArea {
            x: 0,
            y: 0,
            width: 1500,
            height: 1000,
        };
        let (lw, lh) = work.logical_size(1.5);
        assert_eq!((lw, lh), (1000, 667));
        assert_eq!(clamp_logical_size(460, 340, lw, lh), (460, 340));
    }

    /// 工具函数编译期使用检查（WorkArea 辅助函数在测试中的正确性）。
    #[test]
    fn work_area_boundaries_sanity() {
        let left = WorkArea {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
        };
        assert_eq!(left.right(), 0);
        assert_eq!(left.bottom(), 1080);
        let rect = place_panel(-10.0, 10.0, 460, 340, 1.0, left);
        assert!(rect.x < 0, "面板应整体留在副屏（负坐标）");
        assert!(rect.x >= left.x, "不得越过副屏左缘");
        ok_of::<u32, AppError>(Ok(0)); // 工具函数签名检查（成功分支）
    }

    #[test]
    fn decide_hide_suppressed_dragging_blocks_blur_hide() {
        // 真机修复轮 3：拖拽模态循环的虚假 Focused(false) 不得隐藏面板。
        assert!(decide_hide_suppressed(false, true), "拖拽中必须压制");
        assert!(decide_hide_suppressed(true, false), "引导可见必须压制");
        assert!(decide_hide_suppressed(true, true), "叠加仍压制");
        assert!(
            !decide_hide_suppressed(false, false),
            "常规失焦（非拖拽/无引导）不压制"
        );
    }
}
