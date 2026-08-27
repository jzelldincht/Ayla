//! instance：单实例前台握手（D7）+ `instance.pid` 缓存文件逻辑（T15 实现，spec 5.7.9；FR-1.4/EX-12）。
//!
//! 职责（配合 tauri-plugin-single-instance，插件注册于 lib.rs 启动最早段）：
//! - **pid 文件**：主实例启动早期写入缓存目录（`%APPDATA%\com.alya.app\instance.pid`，Q7 安装版/
//!   便携版共用），退出时清理（lib.rs `RunEvent::Exit` 兜底 + 托盘「退出」路径显式删除）。写入采用
//!   tmp + rename 原子替换（同 config 先例）；读入缺失/损坏容忍（握手属尽力语义，不阻断启动）。
//! - **D7 前台握手**：二次启动的新实例此刻拥有前台激活权，必须在 `Builder::build()` 之前
//!   （single-instance 插件在 Windows 上于 setup 阶段发现互斥体已存在即 `std::process::exit(0)`）
//!   读取主实例 pid → 校验进程存活 → `AllowSetForegroundWindow(pid)`，让主实例随后 show+focus
//!   面板能够拿到焦点（FR-4.1 焦点落输入区）。
//! - **降级**：`AllowSetForegroundWindow` 被系统拒绝（返回 FALSE）→ 日志记录；主实例侧
//!   `windows/translate.rs` 的 set_focus 失败路径再走「置前无焦点 + 系统通知」降级（spec 5.7.9【A8】）。
//!
//! 约束：本模块运行于启动最早段（无 AppHandle/无配置锁上下文），因此：
//! - 不引入 config/events/windows 依赖，仅依赖 error.rs（AppError）与路径注入（可测试）；
//! - 缓存目录经 `default_pid_dir`（`%APPDATA%` + 冻结 identifier）计算，与 setup 之后
//!   `config::app_config_dir(app)` 同源口径（D8：identifier 发布 1.0.0 前是唯一可改窗口期）；
//! - 所有 OS 调用仅在 `handshake`/`process_alive`/`allow_foreground` 三处，决策逻辑
//!   `decide_grant` 为纯函数可供单测（路径注入测试）。

use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{CloseHandle, HANDLE, STILL_ACTIVE};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;

use crate::error::AppError;

/// pid 缓存文件名（`instance.pid`，spec 5.7.9）。
pub const PID_FILE: &str = "instance.pid";

/// 应用 identifier（D8 冻结候选，与 tauri.conf.json `identifier` 同源：
/// `com.alya.app`；发布 1.0.0 前唯一可改，此后随配置目录/自启项一并冻结）。
pub const IDENTIFIER: &str = "com.alya.app";

/// pid 文件完整路径（`dir` = 应用缓存/配置目录，路径注入以便测试）。
pub fn pid_path(dir: &Path) -> PathBuf {
    dir.join(PID_FILE)
}

// ---------------------------------------------------------------------------
// 读写清（原子写 / 容忍读）
// ---------------------------------------------------------------------------

/// 原子写入 pid（`instance.pid.tmp` → rename 替换；同 config `save_to` 先例）。
/// pid 文件生命周期短、内容单值；失败按错误上抛（调用方记录，不阻断启动）。
pub fn write_pid(dir: &Path, pid: u32) -> Result<(), AppError> {
    use std::io::Write;
    std::fs::create_dir_all(dir)
        .map_err(|err| AppError::Config(format!("创建缓存目录失败 {}: {err}", dir.display())))?;
    let tmp = dir.join(format!("{PID_FILE}.tmp"));
    let final_path = pid_path(dir);
    {
        let mut file = std::fs::File::create(&tmp)
            .map_err(|err| AppError::Config(format!("创建 {} 失败: {err}", tmp.display())))?;
        file.write_all(format!("{pid}\n").as_bytes())
            .map_err(|err| AppError::Config(format!("写入 {} 失败: {err}", tmp.display())))?;
        file.sync_all()
            .map_err(|err| AppError::Config(format!("{} 落盘失败: {err}", tmp.display())))?;
    }
    if let Err(first_err) = std::fs::rename(&tmp, &final_path) {
        let _ = std::fs::remove_file(&final_path);
        if let Err(second_err) = std::fs::rename(&tmp, &final_path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(AppError::Config(format!(
                "{} 原子替换失败: {first_err}; 重试后仍失败: {second_err}",
                final_path.display()
            )));
        }
    }
    Ok(())
}

/// 读取 pid：文件缺失 → `Ok(None)`；内容损坏/非数字 → 警告日志 + `Ok(None)`（容忍，握手尽力）；
/// 其余 IO 错误如实上抛（调用方记录后按无 pid 处理）。
pub fn read_pid(dir: &Path) -> Result<Option<u32>, AppError> {
    let path = pid_path(dir);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(AppError::Config(format!(
                "读取 {} 失败: {err}",
                path.display()
            )))
        }
    };
    match raw.trim().parse::<u32>() {
        Ok(pid) => Ok(Some(pid)),
        Err(_) => {
            log::warn!(
                "instance: {} 内容无法解析（容忍视为无 pid）",
                path.display()
            );
            Ok(None)
        }
    }
}

/// 清理 pid 文件（尽力）：不存在 → Ok；其余失败上抛（调用方记录）。
pub fn remove_pid(dir: &Path) -> Result<(), AppError> {
    let path = pid_path(dir);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::Config(format!(
            "清理 {} 失败: {err}",
            path.display()
        ))),
    }
}

// ---------------------------------------------------------------------------
// OS 原语（进程存活校验 + 前台授权）
// ---------------------------------------------------------------------------

/// 进程是否存活（PID 有效性 + 未退出）。
/// 先 `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)`（失败＝无此进程/无权限 → 不存活），
/// 再 `GetExitCodeProcess`（`STILL_ACTIVE`=259 才视为存活；覆盖句柄可开但进程已退出的窗口期）。
pub fn process_alive(pid: u32) -> bool {
    // SAFETY: 纯内核调用，无借用；句柄用毕立即 CloseHandle 释放。
    unsafe {
        let handle: HANDLE = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(handle) => handle,
            Err(_) => return false, // 无此进程或无查询权限 → 视同不存活
        };
        let mut exit_code: u32 = 0;
        let ok = GetExitCodeProcess(handle, &mut exit_code).is_ok();
        let alive = ok && exit_code == STILL_ACTIVE.0 as u32;
        let _ = CloseHandle(handle);
        alive
    }
}

/// `AllowSetForegroundWindow(pid)`：允许目标进程获得前台激活权（D7）。
/// 返回系统判定结果（windows 0.62 以 `Result<()>` 编码 FALSE＝Err；调用方记录/降级）。
pub fn allow_foreground(pid: u32) -> bool {
    // SAFETY: 无引用参数、无借用，纯系统调用。
    unsafe { AllowSetForegroundWindow(pid) }.is_ok()
}

// ---------------------------------------------------------------------------
// D7 握手（启动最早段调用）
// ---------------------------------------------------------------------------

/// 握手结果（日志与测试断言语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeOutcome {
    /// 无 pid 文件：主实例尚未落盘（或非二次启动）→ 无动作。
    NoPidFile,
    /// pid 文件即本进程：非二次启动，不自授权。
    SelfPid,
    /// pid 指向的进程已不存活：陈旧 pid，无动作。
    StalePid(u32),
    /// 已向既有实例授权前台（D7 成功）。
    Granted(u32),
    /// `AllowSetForegroundWindow` 被系统拒绝：主实例侧将走无焦点降级（spec 5.7.9）。
    Rejected(u32),
}

/// 手握着纯决策（注入读取/存活/授权结果，供单测）：
/// - 读不到 pid（缺失/损坏）→ `NoPidFile`；
/// - pid == 自身 → `SelfPid`（防止误向自己走 D7 路径）；
/// - 进程已死 → `StalePid`；
/// - 存活 → 按 `granted`（系统判定）分支。
pub fn decide_grant(
    pid: Option<u32>,
    self_pid: u32,
    alive: bool,
    granted: bool,
) -> HandshakeOutcome {
    match pid {
        None => HandshakeOutcome::NoPidFile,
        Some(pid) if pid == self_pid => HandshakeOutcome::SelfPid,
        Some(pid) if !alive => HandshakeOutcome::StalePid(pid),
        Some(pid) if granted => HandshakeOutcome::Granted(pid),
        Some(pid) => HandshakeOutcome::Rejected(pid),
    }
}

/// D7 握手（每条路径都记 Info/Warn 日志；不 panic、不返回错误——尽力语义）：
/// 读 pid → 校验存活 → `AllowSetForegroundWindow`。在任何 `Builder::build()` 之前调用。
pub fn handshake(dir: &Path, self_pid: u32) -> HandshakeOutcome {
    let pid = match read_pid(dir) {
        Ok(pid) => pid,
        Err(err) => {
            log::warn!("instance: 握手读取 pid 失败（按无 pid 处理）: {err}");
            return HandshakeOutcome::NoPidFile;
        }
    };
    let outcome = decide_grant(
        pid,
        self_pid,
        pid.map(process_alive).unwrap_or(false),
        pid.map(allow_foreground).unwrap_or(false),
    );
    match outcome {
        HandshakeOutcome::NoPidFile => {
            log::debug!("instance: 握手——无 pid 文件（主实例未落盘或非二次启动）");
        }
        HandshakeOutcome::SelfPid => {
            log::debug!("instance: 握手——pid 属本进程（主实例），无需授权");
        }
        HandshakeOutcome::StalePid(pid) => {
            log::info!("instance: 握手——pid {pid} 已不存活（陈旧 pid，跳过授权）");
        }
        HandshakeOutcome::Granted(pid) => {
            log::info!("instance: 握手——已向既有实例 pid {pid} 授权前台（D7）");
        }
        HandshakeOutcome::Rejected(pid) => {
            log::warn!(
                "instance: 握手——AllowSetForegroundWindow({pid}) 被系统拒绝；主实例将走无焦点降级"
            );
        }
    }
    outcome
}

/// 启动最早段（无 AppHandle）计算配置/缓存目录：`%APPDATA%\{identifier}`。
/// 与 tauri `app_config_dir`（`FOLDERID_RoamingAppData` + identifier）同源口径（D8）。
/// `APPDATA` 缺失（异常环境）→ None（调用方跳过握手并记调试日志）。
pub fn default_pid_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|base| PathBuf::from(base).join(IDENTIFIER))
}

// ---------------------------------------------------------------------------
// 单元测试：pid 写读清（路径注入）/ 握手决策纯逻辑
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicU32, Ordering};

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
            "alya-instance-test-{tag}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        assert!(std::fs::create_dir_all(&dir).is_ok());
        dir
    }

    fn cleanup(dir: &Path) {
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn write_read_remove_roundtrip() {
        let dir = test_dir("roundtrip");
        assert_eq!(ok_of(read_pid(&dir)), None, "缺失文件应读为 None");

        ok_of(write_pid(&dir, 4242));
        assert_eq!(ok_of(read_pid(&dir)), Some(4242));
        assert!(pid_path(&dir).exists());
        assert!(
            !dir.join(format!("{PID_FILE}.tmp")).exists(),
            "原子写不残留 tmp"
        );

        // 覆盖重写（PID 复用语义：后写胜）
        ok_of(write_pid(&dir, 9999));
        assert_eq!(ok_of(read_pid(&dir)), Some(9999));

        ok_of(remove_pid(&dir));
        assert_eq!(ok_of(read_pid(&dir)), None, "清理后读为 None");
        // 幂等删除
        ok_of(remove_pid(&dir));
        cleanup(&dir);
    }

    #[test]
    fn corrupted_pid_content_tolerated_as_none() {
        let dir = test_dir("corrupt");
        ok_of(write_pid(&dir, 1));
        std::fs::write(pid_path(&dir), "not-a-number\n").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            ok_of(read_pid(&dir)),
            None,
            "损坏内容容忍为无 pid（不 panic 不上抛）"
        );
        // 空文件同样容忍
        std::fs::write(pid_path(&dir), "").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ok_of(read_pid(&dir)), None);
        // 清理仍幂等
        ok_of(remove_pid(&dir));
        cleanup(&dir);
    }

    #[test]
    fn decide_grant_covers_all_outcomes() {
        // 无 pid 文件 → NoPidFile（即使系统判定均按 false 走，也先短路）
        assert_eq!(
            decide_grant(None, 1, false, false),
            HandshakeOutcome::NoPidFile
        );
        // pid == 自身 → SelfPid（不唤起自己）
        assert_eq!(
            decide_grant(Some(7), 7, true, true),
            HandshakeOutcome::SelfPid
        );
        // 陈旧 pid → StalePid（不与存活/授权判定交叉）
        assert_eq!(
            decide_grant(Some(7), 1, false, true),
            HandshakeOutcome::StalePid(7)
        );
        // 存活 + 系统接受 → Granted
        assert_eq!(
            decide_grant(Some(7), 1, true, true),
            HandshakeOutcome::Granted(7)
        );
        // 存活但 AllowSetForegroundWindow 被拒 → Rejected（降级路径记录）
        assert_eq!(
            decide_grant(Some(7), 1, true, false),
            HandshakeOutcome::Rejected(7)
        );
    }

    #[test]
    fn pid_path_and_identifier_stability() {
        // D8：pid 文件与配置目录同根（identifier 冻结，发布前唯一可改窗口期）。
        let dir = test_dir("path");
        assert_eq!(
            pid_path(&dir),
            dir.join(PID_FILE),
            "pid 路径 = 目录 + instance.pid（spec 5.7.9 字面）"
        );
        assert_eq!(
            IDENTIFIER, "com.alya.app",
            "identifier 冻结候选与 tauri.conf.json 同源"
        );
        // default_pid_dir 的合成逻辑（APPDATA 注入变体——仅验证拼装，不触真实环境）
        let base = PathBuf::from("C:\\Users\\x\\AppData\\Roaming");
        assert_eq!(
            base.join(IDENTIFIER),
            PathBuf::from("C:\\Users\\x\\AppData\\Roaming").join("com.alya.app")
        );
        cleanup(&dir);
    }
}
