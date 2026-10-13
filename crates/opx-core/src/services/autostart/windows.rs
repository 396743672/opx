//! Windows：HKCU `Software\Microsoft\Windows\CurrentVersion\Run` 注册表项。
//!
//! 沿用 opx 既有实现（直连 WinAPI，无子进程），**行为不变**：值名 `OPX`，写入带引号的 exe 绝对路径。

use std::path::Path;

// `Result` 别名需常驻：`impl Autostart for WindowsAutostart` 跨平台编译（Linux 上退化为 no-op 桩），
// 其 `enable`/`disable` 签名 `-> Result<()>` 在所有平台都解析为 `anyhow::Result`。
// `anyhow!` 宏只用于 Windows 分支体，按 cfg 门控避免 Linux 下的 unused-import 警告。
use anyhow::Result;
#[cfg(windows)]
use anyhow::anyhow;

use super::Autostart;

/// 注册表值名。
pub const RUN_VALUE: &str = "OPX";
/// Run 键路径。
pub const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// 打开 HKCU 的 Run 键（读写权限）。
#[cfg(windows)]
fn run_key() -> winreg::RegKey {
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey_with_flags(RUN_KEY_PATH, winreg::enums::KEY_READ | winreg::enums::KEY_WRITE)
        .expect("打开注册表 Run 键失败")
}

/// Windows 注册表自启后端。
pub struct WindowsAutostart;

impl Autostart for WindowsAutostart {
    fn platform(&self) -> &'static str {
        "registry"
    }
    fn is_supported(&self) -> bool {
        cfg!(windows)
    }
    fn is_enabled(&self) -> bool {
        #[cfg(windows)]
        {
            run_key().get_value::<String, _>(RUN_VALUE).is_ok()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    fn enable(&self, exe: &Path) -> Result<()> {
        #[cfg(windows)]
        {
            let exe_path = exe.to_string_lossy().replace('/', "\\");
            let quoted = format!("\"{}\"", exe_path);
            run_key()
                .set_value(RUN_VALUE, &quoted)
                .map_err(|e| anyhow!("写入注册表失败: {}", e))?;
        }
        #[cfg(not(windows))]
        {
            let _ = exe;
        }
        Ok(())
    }
    fn disable(&self) -> Result<()> {
        #[cfg(windows)]
        {
            let _ = run_key().delete_value(RUN_VALUE);
        }
        Ok(())
    }
}
