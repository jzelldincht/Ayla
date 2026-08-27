//! translate 窗（label=translate）：翻译面板窗口生命周期与定位（T11 实现，spec 5.5/5.7.2/5.7.3）。
//!
//! 职责：
//! - **暖窗预创建**（D5/J1 前提）：启动序列末尾 `init()` 创建并隐藏（无边框、透明、阴影、
//!   skip_taskbar、初始不可见；C3：topmost 于 show 置位、隐藏后清除）；常驻复用，隐藏≠销毁。
//! - **统一显示路径** `show_panel`：place_at_cursor（光标物理坐标 → 归属显示器含负坐标副屏 →
//!   持久逻辑尺寸 × 该屏 scale → (+16,+16) 逻辑偏移 → 选区避让退化（C2：无系统选区矩形，直按
//!   鼠标锚点规则）→ 右/下/角部翻转 → 工作区 8px 钳制，绝不跨屏）→ show + set_focus →
//!   `panel://show` 统一发射 → 打点 `panel_visible`（7.2）。
//! - **失焦隐藏**（FR-9.2/5.7.3）：`Focused(false)` 且 `panel.hideOnBlur` 开且无压制条件
//!   （框选进行中=遮罩可见 / onboarding 可见；T16 的引导期压制同样经 onboarding 可见投影）→
//!   隐藏；隐藏**不取消**进行中翻译（D10）；隐藏幂等（重复 hide 无副作用）。
//! - **尺寸调整**（FR-9.3/A4）：`resizable(true)`（无边框窗在 Windows 上保留原生隐形调尺寸
//!   边框＝四边四角热区，约系统边框宽度）＋ `Resized` 事件防抖（500ms 松手后）持久化逻辑宽高
//!   （`panel.widthLogical/heightLogical`，INV-4 存逻辑值抗 DPI 变更），并在持久化时按当前
//!   显示器工作区 80% 上限与最小 360×240 钳制回写。
//! - **尺寸上限**（design-spec 3.1）：最小 360×240、上限所在工作区 80%——show 前与 resize
//!   持久化时均经 `clamp_logical_size` 钳制（cap < 最小尺寸时 cap 优先，保证绝不出屏）。
//!
//! 依赖方向：windows/translate.rs → config/events/windows::mask（spec 5.4 禁止反向）。
//! 定位/钳制为纯函数（`place_panel`/`clamp_logical_size`），单测覆盖 spec 测试策略点名项：
//! 四角溢出翻转、负坐标副屏、屏小于面板、100%/150% 换算。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

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

/// 面板最小尺寸（逻辑像素，design-spec 3.1：最小 360×240）。
pub const PANEL_MIN_WIDTH_LOGICAL: u32 = 360;
pub const PANEL_MIN_HEIGHT_LOGICAL: u32 = 240;

/// 面板最大尺寸＝所在显示器工作区的 80%（design-spec 3.1）。
pub const PANEL_MAX_WORK_AREA_RATIO: f64 = 0.8;

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

/// 面板逻辑尺寸钳制（design-spec 3.1）：
/// - 最小 360×240（不足抬升到最小）；
/// - 上限＝所在工作区 80%（超出压回）；
/// - 工作区 80% < 最小尺寸的极端窄屏：**上限优先**（绝不出屏的硬约束优先于最小尺寸）。
///
/// `work_w/work_h` 为工作区**逻辑像素**尺寸（调用方负责按所在屏 scale 换算）。
pub fn clamp_logical_size(width: u32, height: u32, work_w: u32, work_h: u32) -> (u32, u32) {
    let cap_w = (work_w as f64 * PANEL_MAX_WORK_AREA_RATIO).round() as u32;
    let cap_h = (work_h as f64 * PANEL_MAX_WORK_AREA_RATIO).round() as u32;
    let w = std::cmp::min(std::cmp::max(width, PANEL_MIN_WIDTH_LOGICAL), cap_w);
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
/// 3. 仍有溢出 → 工作区内钳制，四周 8px 逻辑边距；
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
    let bottom_limit = work.bottom() - margin;

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
    // 钳制（8px 边距；翻转后仍溢出或触碰边缘时贴回工作区，绝不跨屏）
    x = clamp_i(x, work.x + margin, right_limit - panel_w);
    y = clamp_i(y, work.y + margin, bottom_limit - panel_h);

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
    .title("翻译面板")
    .inner_size(DEFAULT_INNER_SIZE_LOGICAL.0, DEFAULT_INNER_SIZE_LOGICAL.1)
    .min_inner_size(
        PANEL_MIN_WIDTH_LOGICAL as f64,
        PANEL_MIN_HEIGHT_LOGICAL as f64,
    )
    .decorations(false)
    .transparent(true)
    .shadow(true)
    .skip_taskbar(true)
    // 无边框 + resizable：Windows 上保留系统隐形调尺寸边框（四边四角热区，FR-9.3）；
    // 松手后由 Resized 防抖持久化逻辑尺寸（本模块 on_window_event）。
    .resizable(true)
    // C3 裁决：topmost 于 show 置位、隐藏后清除；建窗期恒为 false。
    .always_on_top(false)
    // D5 暖窗：初始不可见，show 由统一显示路径执行。
    .visible(false)
    .build()
    .map_err(|e| AppError::Window(format!("translate 窗创建失败: {e}")))
    .inspect(|window| {
        // tauri 2：WebviewWindowBuilder 无 on_window_event——建窗成功后经窗口句柄订阅
        // （WindowEvent 监听为"不阻塞构建"的附加回调，失败仅记日志，不阻断启动）。
        window.on_window_event(move |event| match event {
            WindowEvent::Focused(false) => on_focus_lost(&app_clone),
            WindowEvent::Resized(size) => on_resized(&app_clone, *size),
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
    let rect = place_at_cursor(app)?;

    window
        .set_position(PhysicalPosition::new(rect.x, rect.y))
        .map_err(|e| AppError::Window(format!("translate 定位失败: {e}")))?;
    window
        .set_size(PhysicalSize::new(rect.width, rect.height))
        .map_err(|e| AppError::Window(format!("translate 尺寸设置失败: {e}")))?;
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
            .body("翻译面板已显示，但未能获得焦点；请点击任务栏图标或按 Alt+E 重新呼出。")
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

/// 面板定位（spec 5.7.2）：光标物理坐标 → 归属显示器（含负坐标副屏）→
/// 持久逻辑尺寸 × 该屏 scale → `place_panel`（偏移/翻转/钳制）。
/// 光标位置获取失败 → 回落主屏工作区中心（spec 5.7.2 第 1 步）。
pub fn place_at_cursor(app: &AppHandle) -> Result<PanelRect, AppError> {
    let cursor = app.cursor_position().ok().map(|p| (p.x, p.y));
    let (anchor_x, anchor_y, work, scale) = resolve_anchor(app, cursor)?;
    let settings = config::read_global()?;
    Ok(place_panel(
        anchor_x,
        anchor_y,
        settings.panel.width_logical,
        settings.panel.height_logical,
        scale,
        work,
    ))
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

/// `Focused(false)`：hideOnBlur 开 且 无压制条件 → 隐藏。
/// 压制条件（spec 5.7.3 交汇矩阵）：框选进行中（任一 mask 窗可见）、
/// onboarding 可见（含 T14 引导窗与 T16 引导期压制投影）。
fn on_focus_lost(app: &AppHandle) {
    let window = match get_panel(app) {
        Ok(w) => w,
        Err(_) => return,
    };
    if !window.is_visible().unwrap_or(false) {
        return; // 已隐藏（含 JS 主动隐藏）→ 幂等
    }
    if is_hide_suppressed(app) {
        return;
    }
    let hide_on_blur = config::read_global()
        .map(|s| s.panel.hide_on_blur)
        .unwrap_or(true);
    if hide_on_blur {
        if let Err(err) = hide_panel(app) {
            log::error!("translate: 失焦隐藏失败: {err}");
        }
    }
}

fn is_hide_suppressed(app: &AppHandle) -> bool {
    // 引导窗可见（T14 建窗后按可见性判定；引导期压制的面板本身也处于被隐藏态）。
    if let Some(onboarding) = app.get_webview_window("onboarding") {
        if onboarding.is_visible().unwrap_or(false) {
            return true;
        }
    }
    // 框选进行中（spec 5.7.3：面板已在触发时强制隐藏，此判定防御散落状态漂移）。
    crate::windows::mask::any_visible()
}

// ---------------------------------------------------------------------------
// 尺寸调整持久化（FR-9.3/A4：四形态共用单一记忆值；松手持久化逻辑宽高）
// ---------------------------------------------------------------------------

fn on_resized(app: &AppHandle, size: PhysicalSize<u32>) {
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

/// 松手持久化：物理 → 逻辑（÷ 所在屏 scale，INV-4）→ 钳制（最小 360×240、上限工作区 80%）→
/// 若被钳制先回写窗口 → 写 config（值未变则跳过落盘）。
fn persist_panel_size(app: &AppHandle, physical: PhysicalSize<u32>) -> Result<(), AppError> {
    let window = get_panel(app)?;
    let scale = window.scale_factor().unwrap_or(1.0);
    let (work_w, work_h) = match window.current_monitor() {
        Ok(Some(monitor)) => {
            let wa = work_area_of(&monitor);
            wa.logical_size(scale)
        }
        Ok(None) => (u32::MAX / 2, u32::MAX / 2), // 无显示器信息：仅最小尺寸钳制
        Err(_) => (u32::MAX / 2, u32::MAX / 2),
    };
    let logical_w = ((physical.width as f64) / scale).round() as u32;
    let logical_h = ((physical.height as f64) / scale).round() as u32;
    let (lw, lh) = clamp_logical_size(logical_w, logical_h, work_w, work_h);

    // 上限钳制回写窗口（把超 80% 的窗口物理尺寸压回）
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
        // 小屏物理 400×300（100%）：80% 上限=320×240 < 出厂 460×340 → 钳到上限；布局仍完整贴边。
        let small = WorkArea {
            x: 0,
            y: 0,
            width: 400,
            height: 300,
        };
        let (w, h) = clamp_logical_size(460, 340, 400, 300);
        assert_eq!(
            (w, h),
            (320, 240),
            "屏小于面板：上限 80% 优先（不出屏硬约束）"
        );
        let rect = place_panel(200.0, 150.0, 460, 340, 1.0, small);
        assert_eq!((rect.width, rect.height), (320, 240));
        assert_within_work_area(rect, small, 1.0);
    }

    #[test]
    fn clamp_size_min_and_cap_boundaries() {
        // 正常大屏：出厂 460×340 原样保留。
        assert_eq!(clamp_logical_size(460, 340, 2000, 1500), (460, 340));
        // 小于最小 → 抬升到 360×240。
        assert_eq!(clamp_logical_size(100, 100, 2000, 1500), (360, 240));
        // 超大 → 压到 80%（逻辑）。
        assert_eq!(clamp_logical_size(2000, 1500, 1000, 800), (800, 640));
        // 0（配置异常/早期值）→ 最小。
        assert_eq!(clamp_logical_size(0, 0, 2000, 1500), (360, 240));
        // cap < 最小（更极端窄屏）→ cap 优先。
        assert_eq!(clamp_logical_size(460, 340, 300, 200), (240, 160));
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
}
