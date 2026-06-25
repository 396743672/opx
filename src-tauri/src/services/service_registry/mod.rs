use anyhow::{Result, anyhow};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use self::windows::{register_current, unregister_current};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use self::linux::{register_current, unregister_current};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use self::macos::{register_current, unregister_current};

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub fn register_current() -> Result<()> {
    Err(anyhow!("Unsupported platform"))
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub fn unregister_current() -> Result<()> {
    Err(anyhow!("Unsupported platform"))
}