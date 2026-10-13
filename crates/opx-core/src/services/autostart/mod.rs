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
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let var = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let var = std::env::var_os("HOME");
    var.map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
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
