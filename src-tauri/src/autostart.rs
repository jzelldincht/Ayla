//! autostart：开机自启真实接线（T15 实现，spec 5.7.9 + FR-1.5；轮 17 起双模式支持 MSIX/Store）。
//!
//! 双模式（真机修复轮 17，MS Store 准备）：
//! - **非包身份**（便携版/NSIS 安装版，现状）：tauri-plugin-autostart 2.5 经
//!   `ManagerExt::autolaunch()` 写/删 **HKCU `SOFTWARE\Microsoft\Windows\CurrentVersion\Run`**
//!   （值名＝"alya"；应用路径由插件以 `current_exe()` 注入）。
//! - **包身份**（MSIX/Store 版）：MSIX 注册表虚拟化使 HKCU Run 写入落到包容器、真实 Run 键
//!   不会被写——改走 **WinRT StartupTask**（manifest `uap5:windows.startupTask` 扩展声明的
//!   `AylaStartupTask`）：`GetForCurrentPackageAsync` 取任务 → 状态判定 →
//!   `RequestEnableAsync`/`Disable`。用户在系统设置 > 应用 > 启动 中的开关即
//!   `DisabledByUser` 态（此时应用内重开会被拒，报错提示去系统设置）。
//!
//! 共享语义（FR-1.5「开关开启才写启动项、关闭即移除」，经纯决策 `decide_action`）：
//! - 目标开启 → 启用（幂等）；
//! - 目标关闭且当前已启用 → 禁用；
//! - 目标关闭且当前已禁用 → 空闲（幂等，不误报）。
//!
//! 任一步失败返回 `AppError`（设置页行内 error；config 值不经失败路径写入——调用方 windows/settings.rs 先自启后 config、失败即中止）。
//!
//! 已知限制：非包便携版移动目录后旧 Run 项指向旧路径，重开关一次即修复（A10，不变）。
//! 日志纪律（INV-7 精神）：只记状态与错误类别，不记 exe 路径。

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

use crate::error::AppError;

/// MSIX manifest 中声明的 StartupTask TaskId（scripts/build-msix.ps1 生成的清单同值）。
const STARTUP_TASK_ID: &str = "AylaStartupTask";

/// 应用自启目标状态（纯决策单测覆盖：is_enabled=已启用/已禁用 × 开关方向）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutostartAction {
    /// 需启用（目标开启）。
    Enable,
    /// 需禁用（目标关闭且当前已启用）。
    Disable,
    /// 无操作（目标关闭且当前已禁用——幂等）。
    Idle,
}

/// 纯决策：开关目标 × 当前注册表/任务状态 → 应执行的动作。
pub fn decide_action(target_on: bool, currently_enabled: bool) -> AutostartAction {
    match (target_on, currently_enabled) {
        (true, _) => AutostartAction::Enable,
        (false, true) => AutostartAction::Disable,
        (false, false) => AutostartAction::Idle,
    }
}

/// 是否运行于 MSIX 包身份下（Store 版）。非包进程 `Package::Current()` 返回 Err。
pub fn is_packaged() -> bool {
    use windows::ApplicationModel::Package;
    Package::Current().is_ok()
}

/// FR-1.5 接线：按开关目标使能/禁用开机自启（双模式分派；失败上抛不落 config）。
pub fn apply(app: &AppHandle, target_on: bool) -> Result<(), AppError> {
    if is_packaged() {
        apply_packaged(target_on)
    } else {
        apply_registry(app, target_on)
    }
}

// ---------------------------------------------------------------------------
// 非包身份：HKCU Run（tauri-plugin-autostart，便携版/安装版现状）
// ---------------------------------------------------------------------------

fn apply_registry(app: &AppHandle, target_on: bool) -> Result<(), AppError> {
    let manager = app.autolaunch();
    let currently_enabled = manager
        .is_enabled()
        .map_err(|err| AppError::Config(crate::ui_text::resolve_global().autostart_query_failed(&err.to_string())))?;
    match decide_action(target_on, currently_enabled) {
        AutostartAction::Enable => manager
            .enable()
            .map_err(|err| AppError::Config(crate::ui_text::resolve_global().autostart_write_failed(&err.to_string())))?,
        AutostartAction::Disable => manager
            .disable()
            .map_err(|err| AppError::Config(crate::ui_text::resolve_global().autostart_remove_failed(&err.to_string())))?,
        AutostartAction::Idle => {
            log::debug!("autostart: 目标关闭且当前未启用（HKCU Run 无此项，无需移除）");
            return Ok(());
        }
    }
    log::info!(
        "autostart: {}（HKCU Run，普通用户权限，无提权）",
        if target_on {
            "开机自启已启用"
        } else {
            "开机自启已关闭"
        }
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 包身份：WinRT StartupTask（MSIX/Store 版；manifest uap5 扩展）
// ---------------------------------------------------------------------------

/// 阻塞等待 IAsyncOperation（与 ocr::wait_async 同构；本模块私有精简版）。
fn wait_async<T>(operation: windows_future::IAsyncOperation<T>) -> Result<T, windows::core::Error>
where
    T: windows::core::RuntimeType + Send + 'static,
{
    use windows_future::{AsyncOperationCompletedHandler, AsyncStatus};
    use std::sync::mpsc;
    match operation.Status()? {
        AsyncStatus::Completed => return operation.GetResults(),
        AsyncStatus::Error => {
            return Err(windows::core::Error::from_hresult(operation.ErrorCode()?))
        }
        _ => {}
    }
    let (tx, rx) = mpsc::channel::<Result<T, windows::core::Error>>();
    operation.SetCompleted(&AsyncOperationCompletedHandler::new(move |sender, status| {
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
    }))?;
    rx.recv().map_err(|_| {
        windows::core::Error::from_hresult(windows::core::HRESULT::from_win32(1223))
    })?
}

/// 取 manifest 声明的本包 StartupTask（TaskId=STARTUP_TASK_ID）。
fn packaged_startup_task() -> Result<windows::ApplicationModel::StartupTask, AppError> {
    let lang = crate::ui_text::resolve_global();
    let op = windows::ApplicationModel::StartupTask::GetAsync(&windows::core::HSTRING::from(
        STARTUP_TASK_ID,
    ))
    .map_err(|err| AppError::Config(lang.autostart_query_failed(&err.to_string())))?;
    let task = wait_async(op)
        .map_err(|err| AppError::Config(lang.autostart_query_failed(&err.to_string())))?;
    // GetAsync 对不存在的 TaskId 会失败；成功即视为清单声明与请求一致。
    let _ = task.TaskId().unwrap_or_default();
    Ok(task)
}

fn apply_packaged(target_on: bool) -> Result<(), AppError> {
    use windows::ApplicationModel::StartupTaskState;
    let lang = crate::ui_text::resolve_global();
    let task = packaged_startup_task()?;
    let state = task
        .State()
        .map_err(|err| AppError::Config(lang.autostart_query_failed(&err.to_string())))?;
    let currently_enabled =
        matches!(state, StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy);
    match decide_action(target_on, currently_enabled) {
        AutostartAction::Enable => {
            if currently_enabled {
                log::debug!("autostart: 包身份目标开启且已启用（幂等）");
                return Ok(());
            }
            match state {
                StartupTaskState::Disabled => {
                    let enable_op = task
                        .RequestEnableAsync()
                        .map_err(|err| {
                            AppError::Config(lang.autostart_write_failed(&err.to_string()))
                        })?;
                    let new_state = wait_async(enable_op).map_err(|err| {
                        AppError::Config(lang.autostart_write_failed(&err.to_string()))
                    })?;
                    if !matches!(
                        new_state,
                        StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy
                    ) {
                        return Err(AppError::Config(lang.autostart_enable_rejected()));
                    }
                }
                StartupTaskState::DisabledByUser => {
                    // 用户在系统设置里关过：应用内 API 不可再启用，指引用户去系统设置。
                    return Err(AppError::Config(lang.autostart_user_disabled()));
                }
                StartupTaskState::DisabledByPolicy => {
                    return Err(AppError::Config(lang.autostart_policy_disabled()));
                }
                _ => {
                    return Err(AppError::Config(lang.autostart_enable_rejected()));
                }
            }
        }
        AutostartAction::Disable => task
            .Disable()
            .map_err(|err| AppError::Config(lang.autostart_remove_failed(&err.to_string())))?,
        AutostartAction::Idle => {
            log::debug!("autostart: 包身份目标关闭且当前未启用（幂等）");
            return Ok(());
        }
    }
    log::info!(
        "autostart: {}（MSIX StartupTask {STARTUP_TASK_ID}）",
        if target_on {
            "开机自启已启用"
        } else {
            "开机自启已关闭"
        }
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 单元测试：决策纯逻辑（写入/移除/空闲三分支）——两模式共用同一决策
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decide_action_covers_all_states() {
        // 目标开启：无论当前状态都启用（幂等——含 A10 修复语义）
        assert_eq!(decide_action(true, false), AutostartAction::Enable);
        assert_eq!(decide_action(true, true), AutostartAction::Enable);
        // 目标关闭 + 当前启用 → 禁用
        assert_eq!(decide_action(false, true), AutostartAction::Disable);
        // 目标关闭 + 当前已禁用 → 空闲（幂等）
        assert_eq!(decide_action(false, false), AutostartAction::Idle);
    }
}
