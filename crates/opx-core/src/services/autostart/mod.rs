//! 开机自启（用户级）抽象——让 **opx 自身**随用户登录自动启动。
//!
//! ## 范围（ADR §6 #3；2026-10-09 与用户澄清）
//! - **仅 opx 自身**注册为自启项。
//! - **被管软件不注册系统服务**：它们由 opx 启动后按各自 `auto_start_on_app_start` 配置拉起
//!   （见 `services/startup_bootstrap.rs` 的统一启动编排），不在本模块范围内。
//!
//! ## 三平台实现（均**无需 root/admin**，随用户登录生效）
//! - **Linux**：XDG autostart —— `~/.config/autostart/opx.desktop`
//! - **macOS**：LaunchAgent —— `~/Library/LaunchAgents/com.opx.app.plist`
//! - **Windows**：HKCU `...\CurrentVersion\Run` 注册表项（沿用既有行为，不变）
//!
//! 纯函数（`.desktop` / plist 文本、路径布局）可跨平台单测。

use std::path::{Path, PathBuf};

use anyhow::Result;

pub mod linux;
pub mod macos;
pub mod windows;

pub use linux::LinuxAutostart;
pub use macos::MacosAutostart;
pub use windows::WindowsAutostart;

/// 开机自启后端。
pub trait Autostart: Send + Sync {
    /// 后端标识（`xdg` / `launchd` / `registry` / `unsupported`）。
    fn platform(&self) -> &'static str;
    /// 当前平台是否真正支持。
    fn is_supported(&self) -> bool;
    /// 是否已启用自启。
    fn is_enabled(&self) -> bool;
    /// 启用自启并绑定到 `exe`（当前程序绝对路径）。
    fn enable(&self, exe: &Path) -> Result<()>;
    /// 关闭自启；**幂等**（未启用时也应成功）。
    fn disable(&self) -> Result<()>;
}

/// 按编译目标返回自启后端。
pub fn current() -> Box<dyn Autostart> {
    #[cfg(target_os = "linux")]
    {
        Box::new(LinuxAutostart)
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(MacosAutostart)
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(WindowsAutostart)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Box::new(Unsupported)
    }
}

/// 不支持平台上的空实现（保持「关闭即 no-op、查询为 false」的既有语义）。
pub struct Unsupported;

impl Autostart for Unsupported {
    fn platform(&self) -> &'static str {
        "unsupported"
    }
    fn is_supported(&self) -> bool {
        false
    }
    fn is_enabled(&self) -> bool {
        false
    }
    fn enable(&self, _exe: &Path) -> Result<()> {
        Ok(())
    }
    fn disable(&self) -> Result<()> {
        Ok(())
    }
}

/// 用户主目录（`$HOME`；Windows 用 `$USERPROFILE`）。
///
/// systemd 服务（本项目的 `opx-svc` 经 `xvfb-run` 启动）默认**不注入 HOME**，导致
/// 环境变量为空、`set_autostart` 报「无法确定用户主目录（$HOME）」500（实测服务器）。
/// 因此增加两级 fallback：先从环境变量取，Unix 下再查 `/etc/passwd`（getpwuid），
/// 两者都失败才返回 None。桌面正常会话仍走环境变量，行为不变。
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let var = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let var = std::env::var_os("HOME");

    // fallback：从 passwd 数据库按当前 uid 取 home（systemd 无 HOME 场景）
    #[cfg(unix)]
    let passwd_home: Option<PathBuf> = unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() {
            None
        } else {
            let home = std::ffi::CStr::from_ptr((*pw).pw_dir).to_string_lossy();
            if home.is_empty() {
                None
            } else {
                Some(PathBuf::from(home.into_owned()))
            }
        }
    };
    #[cfg(not(unix))]
    let passwd_home: Option<PathBuf> = None;

    pick_home(var, passwd_home.as_deref())
}

/// 纯函数：给定环境变量值与 fallback home，返回应采用的 home。
/// 供单测覆盖「env 缺失 → 用 fallback」逻辑，不必改动真实进程环境。
fn pick_home(env_home: Option<std::ffi::OsString>, fallback: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = env_home.map(PathBuf::from).filter(|p| !p.as_os_str().is_empty()) {
        return Some(p);
    }
    fallback.map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_backend_is_noop_and_safe() {
        let b = Unsupported;
        assert!(!b.is_supported());
        assert!(!b.is_enabled());
        assert!(b.enable(Path::new("/x/opx")).is_ok(), "enable 应为幂等 no-op");
        assert!(b.disable().is_ok());
    }

    /// 回归：systemd 服务无 HOME 环境变量时，必须用 passwd fallback，
    /// 否则 set_autostart 报「无法确定用户主目录」500。
    #[test]
    fn pick_home_falls_back_when_env_missing_or_empty() {
        use std::ffi::OsString;
        let fallback = Path::new("/root");
        // env 有值 → 优先用 env（桌面正常会话行为不变）
        assert_eq!(
            pick_home(Some(OsString::from("/home/alice")), Some(fallback)),
            Some(PathBuf::from("/home/alice"))
        );
        // env 缺失 → 用 passwd fallback（systemd 场景）
        assert_eq!(pick_home(None, Some(fallback)), Some(PathBuf::from("/root")));
        // env 为空串 → 跳过，用 fallback
        assert_eq!(
            pick_home(Some(OsString::from("")), Some(fallback)),
            Some(PathBuf::from("/root"))
        );
        // 两者都无 → None
        assert_eq!(pick_home(None, None), None);
    }

    #[test]
    fn current_backend_matches_target_os() {
        let b = current();
        #[cfg(target_os = "linux")]
        assert_eq!(b.platform(), "xdg");
        #[cfg(target_os = "macos")]
        assert_eq!(b.platform(), "launchd");
        #[cfg(target_os = "windows")]
        assert_eq!(b.platform(), "registry");
    }
}
