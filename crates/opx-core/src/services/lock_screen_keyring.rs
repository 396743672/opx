//! 锁屏凭据后端（macOS / Linux）：用 keyring 封装系统凭据库。
//!
//! 仅非 Windows 平台编译（Windows 走 `lock_screen_win.rs`）。keyring 在 macOS/Linux
//! 上的表现符合预期：写入后新实例、重启均可读回。

use super::*;
use keyring::{Entry, Error as KeyringError};

fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, USER).map_err(|_| "i18n:lockKeyringUnavailable".to_string())
}

/// 写入已算好的 PHC verifier（调用方已完成 Argon2id 哈希与长度校验）
pub fn set_lock_password(phc: &str) -> Result<(), String> {
    entry()?
        .set_password(phc)
        .map_err(|_| "i18n:lockKeyringUnavailable".to_string())
}

/// 读取 PHC；无条目返回 None，其他错误上报
pub fn get_lock_password() -> Result<Option<String>, String> {
    match entry()?.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(_) => Err("i18n:lockKeyringUnavailable".to_string()),
    }
}

/// 清空凭据；无条目也视为成功
pub fn clear_lock_password() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) => Ok(()),
        Err(KeyringError::NoEntry) => Ok(()),
        Err(_) => Err("i18n:lockKeyringUnavailable".to_string()),
    }
}
