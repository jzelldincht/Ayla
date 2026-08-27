//! mask-{i} 窗池（每显示器一窗，T9 实现；spec 5.7.4/5.6.2 ②③④ + design-spec 第 5 章）。
//!
//! 职责：
//! - **窗池与拓扑校验**：`mask-{index}` 按显示器编号（(x,y) 排序）创建/复用；每次触发前经
//!   `available_monitors()` 核对数量/位置/尺寸，几何变化则销毁重建（spec 5.7.4"变化则销毁重建"）。
//! - **几何**：position=显示器 origin（物理）、size=显示器物理尺寸；每窗独立 HWND/DPI 上下文
//!   （混 DPI 下各窗按本屏缩放渲染，每显示器一窗方案的理由见 spec 5.7.4 取舍表）。
//! - **抓帧时序**：`start_capture_session` **先抓全部屏帧 → 再创建/校正窗池 → 显示**——遮罩永不被
//!   截入（D4）；后端会话见 capture 模块。
//! - **窗属性**：无边框、透明（偏差见下）、skip_taskbar、topmost、resizable=false、初始隐藏。
//!   偏差说明：spec 5.5 写"不透明（自绘冻结快照）"，本实现为 transparent(true)——内容为整屏快照时
//!   视觉等同不透明，且支撑 design-spec 第 5 章的 100ms 淡入 / 150ms 淡出（不透明窗体无法呈现
//!   桌面渐现）；快照始终全屏铺满（详见 implementation-report T9 节偏差 3）。
//! - **命令**（6.1）：`complete_selection`（无效选区按 Q11 视同取消）、`cancel_selection`（Esc）；
//!   追加命令 `get_mask_snapshot`（主 Agent 批准方案二：冻结帧以 data: URL 内存字符串返回遮罩页，
//!   INV-3 说明见模块头/implementation-report——**不落盘、无 HTTP，位图仅存于 Rust 会话**，
//!   跨 IPC 是一次性只读编码串，前端绘制后即弃）。会话引导事件 `mask://begin`（events.rs）。
//!
//! 依赖方向：windows/mask.rs → capture/config/events（spec 5.4 禁止反向）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use tauri::{
    AppHandle, Emitter, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use crate::capture::{self, MonitorInfo, ScreenshotsFrameSource};
use crate::error::AppError;
use crate::events::{
    MaskBeginPayload, MaskSnapshotResult, SelectionRect, EVENT_MASK_BEGIN, WINDOW_MASK_LABEL_PREFIX,
};

/// 遮罩窗池（label → 窗口；label 即为编号信息，前端经 URL 查询参数读 monitorIndex）。
#[derive(Default)]
struct MaskPool {
    windows: HashMap<u32, WebviewWindow>,
}

static POOL: OnceLock<Mutex<MaskPool>> = OnceLock::new();

fn pool() -> &'static Mutex<MaskPool> {
    POOL.get_or_init(|| Mutex::new(MaskPool::default()))
}

fn mask_label(index: u32) -> String {
    format!("{WINDOW_MASK_LABEL_PREFIX}{index}")
}

// ---------------------------------------------------------------------------
// 显示器枚举（与 capture 模块同源排序：按 (x,y)，编号=位置）
// ---------------------------------------------------------------------------

/// 收集全部显示器并按 (x, y) 排序编号（INV-4：物理像素；tauri 枚举序不可依赖）。
pub fn collect_monitors(app: &AppHandle) -> Result<Vec<MonitorInfo>, AppError> {
    let mut raw = app
        .available_monitors()
        .map_err(|e| AppError::Window(format!("枚举显示器失败: {e}")))?;
    raw.sort_by_key(|m| (m.position().x, m.position().y));
    Ok(raw
        .into_iter()
        .enumerate()
        .map(|(i, m)| MonitorInfo {
            index: i as u32,
            x: m.position().x,
            y: m.position().y,
            width: m.size().width,
            height: m.size().height,
            scale_factor: m.scale_factor(),
        })
        .collect())
}

/// 光标所在显示器编号（无光标位置/无匹配 → None；供焦点归属，spec 5.6.2 ②c）。
pub fn cursor_monitor_index(app: &AppHandle, monitors: &[MonitorInfo]) -> Option<u32> {
    let cursor = app.cursor_position().ok()?;
    let target = app.monitor_from_point(cursor.x, cursor.y).ok().flatten()?;
    monitors
        .iter()
        .find(|m| m.x == target.position().x && m.y == target.position().y)
        .map(|m| m.index)
}

// ---------------------------------------------------------------------------
// 框选触发（T16 action_router 调用点）
// ---------------------------------------------------------------------------

/// 截图框选触发（spec 5.6.2 ②）：抓帧（先于遮罩显示）→ 会话建立 → 窗池校正 → 显示 → 焦点
/// 给光标所在屏窗 → 广播 mask://begin。返回会话 id。
pub fn start_capture_session(app: &AppHandle) -> Result<String, AppError> {
    let monitors = collect_monitors(app)?;
    // ① 先抓全部帧（此刻 Alya 无任何可见窗口——所有 mask 窗尚隐藏/未创建）
    let frames = capture::capture_all(&ScreenshotsFrameSource, &monitors)?;
    // ② 校验/校正窗池（拓扑变化销毁重建；窗仍保持隐藏——抓帧已完成的证明）
    reconcile_pool(app, &monitors)?;
    // ③ 会话建立（新触发替换旧会话，spec 5.7.5）
    let session_id = capture::begin_session(frames);
    // ④ 定位 + 显示全部遮罩窗（物理 origin/尺寸，INV-4）
    {
        let pool_guard = pool()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for monitor in &monitors {
            let window = pool_guard
                .windows
                .get(&monitor.index)
                .ok_or_else(|| AppError::Window(format!("窗池缺失 mask-{}", monitor.index)))?;
            window
                .set_position(PhysicalPosition::new(monitor.x, monitor.y))
                .map_err(|e| AppError::Window(format!("遮罩定位失败: {e}")))?;
            window
                .set_size(PhysicalSize::new(monitor.width, monitor.height))
                .map_err(|e| AppError::Window(format!("遮罩尺寸设置失败: {e}")))?;
            window
                .show()
                .map_err(|e| AppError::Window(format!("遮罩显示失败: {e}")))?;
        }
        // ⑤ 焦点交给光标所在屏（Esc 键收达到该窗；点击任一窗亦可获得焦点）
        if let Some(index) = cursor_monitor_index(app, &monitors) {
            if let Some(window) = pool_guard.windows.get(&index) {
                let _ = window.set_focus(); // 焦点失败不阻断框选
            }
        }
    }
    // ⑥ 广播"会话开始"（前端据此拉取冻结帧；首次创建的窗页面可能尚未载入——
    //    前端挂载即主动拉取兜底，见 src/mask/App.svelte 的 ensureFrame）
    for monitor in &monitors {
        let payload = MaskBeginPayload {
            session_id: session_id.clone(),
            monitor_index: monitor.index,
        };
        let _ = app.emit_to(mask_label(monitor.index), EVENT_MASK_BEGIN, payload);
    }
    log::debug!(
        "框选开始 sessionId={session_id} 显示器 {} 台（编号 0..={}，掩码组已就绪）",
        monitors.len(),
        monitors.len().saturating_sub(1)
    );
    Ok(session_id)
}

/// 窗池与显示器拓扑对齐：多出销毁、缺失创建、几何不符销毁重建（spec 5.7.4）。
fn reconcile_pool(app: &AppHandle, monitors: &[MonitorInfo]) -> Result<(), AppError> {
    let mut pool_guard = pool()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let expected: std::collections::HashSet<u32> = monitors.iter().map(|m| m.index).collect();

    // 销毁拓扑外的窗
    let stale: Vec<u32> = pool_guard
        .windows
        .keys()
        .filter(|i| !expected.contains(i))
        .copied()
        .collect();
    for index in stale {
        if let Some(window) = pool_guard.windows.remove(&index) {
            let _ = window.destroy(); // 销毁失败仅确保移出池（该窗已不可达）
        }
    }

    for monitor in monitors {
        let geometry_ok = match pool_guard.windows.get(&monitor.index) {
            None => false,
            Some(window) => {
                window
                    .outer_position()
                    .map(|p| p.x == monitor.x && p.y == monitor.y)
                    .unwrap_or(false)
                    && window
                        .inner_size()
                        .map(|s| s.width == monitor.width && s.height == monitor.height)
                        .unwrap_or(false)
            }
        };
        if geometry_ok {
            continue;
        }
        // 缺失或几何变化 → 销毁重建（spec 5.7.4"变化则销毁重建"）
        if let Some(old) = pool_guard.windows.remove(&monitor.index) {
            let _ = old.destroy();
        }
        let window = create_mask_window(app, monitor)?;
        pool_guard.windows.insert(monitor.index, window);
    }
    Ok(())
}

/// 创建单屏遮罩窗（隐藏；显示/定位由触发流程统一执行）。
fn create_mask_window(app: &AppHandle, monitor: &MonitorInfo) -> Result<WebviewWindow, AppError> {
    // monitorIndex 走窗口 URL 查询参数（创建时写入，非 IPC；遮罩页 location.search 读取）
    let url = WebviewUrl::App(PathBuf::from(format!(
        "mask/index.html?monitorIndex={}",
        monitor.index
    )));
    WebviewWindowBuilder::new(app, mask_label(monitor.index), url)
        .title("截图遮罩")
        .inner_size(monitor.width as f64, monitor.height as f64)
        .position(monitor.x as f64, monitor.y as f64)
        .decorations(false)
        .transparent(true) // 偏差见模块头（支撑 100/150ms 淡入淡出；内容为全屏快照时视觉等同不透明）
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(false)
        .visible(false)
        .build()
        .map_err(|e| AppError::Window(format!("遮罩窗 mask-{} 创建失败: {e}", monitor.index)))
}

/// 隐藏全部遮罩窗（确认/取消后回池，不销毁——池化复用）。
fn hide_all_masks() {
    let pool_guard = pool()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for window in pool_guard.windows.values() {
        let _ = window.hide(); // 单窗隐藏失败不扩散（窗口销毁是拓扑任务，不在此处）
    }
}

// ---------------------------------------------------------------------------
// 命令（6.1；T9 注册）
// ---------------------------------------------------------------------------

/// `complete_selection`（6.1：{sessionId, monitorIndex, rect}）：无效选区（Q11）视同取消返回 Ok；
/// 有效则裁剪→PNG 入内存→释放帧→隐藏遮罩（spec 5.6.2 ④）。裁剪为 CPU 密集段，放 spawn_blocking
/// （异步命令）。过期请求（会话已被新触发替换）幂等忽略；本会话内裁剪失败则收回遮罩并返回错误
/// （保持"遮罩可见 ⇔ 活动会话存在"不变量，无残留——FR-8.6）。
#[tauri::command]
pub async fn complete_selection(
    _app: AppHandle,
    session_id: String,
    monitor_index: u32,
    rect: SelectionRect,
) -> Result<(), AppError> {
    if !capture::session_is_current(&session_id) {
        // 过期完成请求：旧会话已被新触发替换——忽略，不影响新会话（下发的 mask://begin 属新会话）
        log::debug!("框选完成忽略（会话已过期） sessionId={session_id}");
        return Ok(());
    }
    if !capture::is_valid_selection(&rect) {
        // 无效判定（Q11）：视同 Esc 取消（FR-8.5），无任何残留
        log::debug!(
            "框选无效（Q11）视同取消 sessionId={session_id} {}x{}",
            rect.width,
            rect.height
        );
        if capture::cancel_session(&session_id) {
            hide_all_masks();
        }
        return Ok(());
    }
    let outcome = tauri::async_runtime::spawn_blocking({
        let session_id_for_crop = session_id.clone();
        move || capture::crop_to_png(&session_id_for_crop, monitor_index, &rect)
    })
    .await
    .map_err(|e| AppError::Capture(format!("裁剪任务被中断: {e}")))?;
    match outcome {
        Ok(crop) => {
            hide_all_masks();
            log::debug!(
                "框选完成 sessionId={session_id} 屏 {monitor_index} 裁剪 {}x{}（PNG 内存 {} 字节）",
                crop.width,
                crop.height,
                crop.png_bytes.len()
            );
            Ok(())
        }
        Err(err) => {
            // 本会话裁剪失败：会话已不可用，收回遮罩保持不变量，错误交上层呈现
            let _ = capture::cancel_session(&session_id);
            hide_all_masks();
            log::warn!("框选裁剪失败 sessionId={session_id}: {err}");
            Err(err)
        }
    }
}

/// `cancel_selection`（6.1：{sessionId}）：Esc 取消——销毁会话、隐藏遮罩，无任何后续动作
/// （FR-8.3/FR-8.6）。幂等：会话 id 不匹配（已被新会话替换）时不隐藏新增遮罩。
#[tauri::command]
pub async fn cancel_selection(_app: AppHandle, session_id: String) -> Result<(), AppError> {
    if capture::cancel_session(&session_id) {
        hide_all_masks();
        log::debug!("框选取消 sessionId={session_id}（遮罩已隐藏，无残留）");
    }
    Ok(())
}

/// `get_mask_snapshot`（追加命令，主 Agent 批准方案二）：遮罩页向 Rust 拉取本屏冻结帧，
/// 返回 data: URL 内存字符串（`data:image/png;base64,...`——无磁盘、无 HTTP、无 asset 协议）。
/// INV-3 说明：位图本体始终只存在于 Rust 会话；本次返回是**一次性只读编码串**——前端解析绘制
/// 到 canvas 后即弃，无重放、无存储；后续 complete/cancel/reocr 的 IPC 不含任何位图。
#[tauri::command]
pub async fn get_mask_snapshot(monitor_index: u32) -> Result<MaskSnapshotResult, AppError> {
    let (session_id, png_bytes) =
        tauri::async_runtime::spawn_blocking(move || capture::frame_png_bytes(monitor_index))
            .await
            .map_err(|e| AppError::Capture(format!("快照编码任务被中断: {e}")))??;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    Ok(MaskSnapshotResult {
        session_id,
        monitor_index,
        data_url: format!("data:image/png;base64,{b64}"),
    })
}

// ---------------------------------------------------------------------------
// 单元测试（纯逻辑；不触真实窗口/屏幕）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_label_format() {
        assert_eq!(mask_label(0), "mask-0");
        assert_eq!(mask_label(2), "mask-2");
        assert_eq!(WINDOW_MASK_LABEL_PREFIX, "mask-");
    }

    #[test]
    fn selection_validity_delegates_to_capture_contract() {
        // 命令语义：无效键（2x2）→ is_valid=false → 视同取消
        let tiny = SelectionRect {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        };
        assert!(!capture::is_valid_selection(&tiny));
        let ok = SelectionRect {
            x: 0,
            y: 0,
            width: 320,
            height: 200,
        };
        assert!(capture::is_valid_selection(&ok));
    }

    /// 排序规则（与 capture 模块约定一致：按 (x,y)，负坐标副屏在前）。
    #[test]
    fn monitor_sort_key_is_position() {
        let mut monitors = [
            MonitorInfo {
                index: 0,
                x: 1920,
                y: 100,
                width: 1920,
                height: 1080,
                scale_factor: 1.0,
            },
            MonitorInfo {
                index: 0,
                x: -1920,
                y: 0,
                width: 1920,
                height: 1080,
                scale_factor: 1.0,
            },
            MonitorInfo {
                index: 0,
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                scale_factor: 1.5,
            },
        ];
        monitors.sort_by_key(|m| (m.x, m.y));
        let xs: Vec<i32> = monitors.iter().map(|m| m.x).collect();
        assert_eq!(xs, [-1920, 0, 1920]);
    }
}
