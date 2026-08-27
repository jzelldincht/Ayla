//! autostart：开机自启真实接线（T15 实现，spec 5.7.9 + FR-1.5）。
//!
//! 机制：tauri-plugin-autostart 2.5（已注册于 lib.rs）经 `ManagerExt::autolaunch()` 提供
//! `AutoLaunchManager`，Windows 端走 **HKCU `SOFTWARE\Microsoft\Windows\CurrentVersion\Run`**
//! 写/删当前 exe 路径（值名＝`app.package_info().name`＝"alya"，经 auto-launch 0.5 的
//! `RegKey::set_value`/`delete_value` 完成；应用启动的 exe 路径由插件在 setup 时以
//! `current_exe()` 注入）。普通用户权限（HKCU 免提权，request 权限要求）；无任何提权路径。
//!
//! 语义（FR-1.5「开关开启才写启动项、关闭即移除」）：
//! - `apply(app, true)`＝写/覆盖 Run 项（已启用时覆盖为当前路径，幂等）；
//! - `apply(app, false)`＝**先 `is_enabled` 判定**：已禁用（Run 值不存在）→ 无事（幂等——auto-launch
//!   对不存在值的 `delete_value` 返回错误，若直接删会把「本来就没开」误报为失败）；
//!   已启用 → 删除 Run 项；
//! - 任一步失败返回 `AppError`（Command 逐层上抛，设置页行内 error；config 值不经此失败
//!   路径写入——调用方 windows/settings.rs 遵循「先注册表后 config，失败即中止」顺序）。
//!
//! 已知限制（A10，记录于 implementation-report）：便携版移动目录后既有 Run 项仍指向旧路径，
//! 重新开关一次即修复（enable 会以当前 exe 路径覆写）；本模块不做启动时自愈探测——
//! 避免与「重新开关修复」的既定兜底语义冲突，且写入动作只应由用户显式开关触发（FR-1.5）。
//!
//! 日志纪律（INV-7 精神）：只记启用/关闭状态与错误类别，不记 exe 全路径（路径非凭据，但
//! 此处无记录必要——错误串由 auto-launch 内部构造，本模块不追加路径字段）。

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

use crate::error::AppError;

/// 应用自启目标状态（纯决策单测覆盖：is_enabled=已启用/已禁用 × 开关方向）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutostartAction {
    /// 需写入 Run 项（目标开启）。
    Enable,
    /// 需删除 Run 项（目标关闭且当前已启用）。
    Disable,
    /// 无操作（目标关闭且当前已禁用——幂等）。
    Idle,
}

/// 纯决策：开关目标 × 当前注册表状态 → 应执行的动作。
/// `target_on` 来自 config 补丁（general.autostart）；`currently_enabled` 来自 `is_enabled`。
pub fn decide_action(target_on: bool, currently_enabled: bool) -> AutostartAction {
    match (target_on, currently_enabled) {
        (true, _) => AutostartAction::Enable,
        (false, true) => AutostartAction::Disable,
        (false, false) => AutostartAction::Idle,
    }
}

/// FR-1.5 接线：按开关目标使能/禁用开机自启（HKCU Run；失败上抛不落 config）。
pub fn apply(app: &AppHandle, target_on: bool) -> Result<(), AppError> {
    let manager = app.autolaunch();
    let currently_enabled = manager
        .is_enabled()
        .map_err(|err| AppError::Config(format!("开机自启状态查询失败: {err}")))?;
    match decide_action(target_on, currently_enabled) {
        AutostartAction::Enable => manager
            .enable()
            .map_err(|err| AppError::Config(format!("开机自启写入失败: {err}")))?,
        AutostartAction::Disable => manager
            .disable()
            .map_err(|err| AppError::Config(format!("开机自启移除失败: {err}")))?,
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
// 单元测试：决策纯逻辑（写入/移除/空闲三分支）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decide_action_covers_all_states() {
        // 目标开启：无论当前状态都写（覆盖为当前 exe 路径，幂等——含 A10 修复语义）
        assert_eq!(decide_action(true, false), AutostartAction::Enable);
        assert_eq!(decide_action(true, true), AutostartAction::Enable);
        // 目标关闭 + 当前启用 → 删除 Run 项
        assert_eq!(decide_action(false, true), AutostartAction::Disable);
        // 目标关闭 + 当前已禁用 → 空闲（幂等，不因 delete_value 不存在误报失败）
        assert_eq!(decide_action(false, false), AutostartAction::Idle);
    }
}
