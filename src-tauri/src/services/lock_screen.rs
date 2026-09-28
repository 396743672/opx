//! 锁屏：本机密码校验（UI 级锁，非安全边界）。
//!
//! 设计要点（与项目约束一致）：
//! - 派生密钥（Argon2id）**现场算出、绝不落盘**；永久化的是 `verifier`（PHC 字符串，
//!   含 salt + 哈希，既非密码也非密钥）。
//! - `verifier` 存入 **OS 凭据库**（Windows Credential Manager / macOS Keychain /
//!   Linux Secret Service），不进 opx 的 `config_dir()` 明文 JSON；清 app 数据/卸载不会
//!   误删，也不会随文件夹迁移到别的机器。
//! - 仅当「有锁」时才执行锁定（手动锁 + 闲置自动锁）；清空锁（删条目）即关闭。
//! - 锁只锁前端 UI；被管进程继续运行。忘记密码 = 清空条目重设，无数据损失。
//!
//! 平台差异：Windows 走 `lock_screen_win.rs`（直接用 `windows-sys` +
//! `CRED_PERSIST_LOCAL_MACHINE`）。原因：keyring 3.x 默认 `CRED_PERSIST_ENTERPRISE`，
//! 在「非域单机」上写入后仅同 Entry 实例可见、新实例/重启读不回，导致设了密码却解不开；
//! `LOCAL_MACHINE` 对同用户所有进程可见、可跨重启读取，符合需求。macOS/Linux 由 keyring
//! 统一封装，无此问题。

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand_core::OsRng;

/// keyring 的 (service, user) 标识；仅非 Windows 平台（走 keyring）使用
#[cfg(not(windows))]
const SERVICE: &str = "opx";
#[cfg(not(windows))]
const USER: &str = "lock-screen";
/// 密码最小长度，避免过短被暴破
const MIN_PASSWORD_LEN: usize = 6;

/// 用 Argon2id 从密码派生 PHC verifier 字符串（纯函数，可单测，不触凭据库）
pub fn hash_password(pw: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(pw.as_bytes(), &salt)
        .map_err(|_| "i18n:lockHashFailed".to_string())?;
    Ok(hash.to_string())
}

/// 校验密码是否匹配已存 PHC verifier（常量时间比较，纯函数，可单测）
pub fn verify_password(phc: &str, pw: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(phc) else {
        return false;
    };
    Argon2::default()
        .verify_password(pw.as_bytes(), &parsed)
        .is_ok()
}

// 平台相关凭据存取：Windows 走 windows-sys，其余平台走 keyring。
#[cfg(windows)]
#[path = "lock_screen_win.rs"]
mod platform;
#[cfg(not(windows))]
#[path = "lock_screen_keyring.rs"]
mod platform;

/// 设置/更换锁屏密码。空锁→开，已有锁→更换。
pub fn set_lock_password(pw: &str) -> Result<(), String> {
    if pw.len() < MIN_PASSWORD_LEN {
        return Err("i18n:lockPasswordTooShort".to_string());
    }
    let phc = hash_password(pw)?;
    platform::set_lock_password(&phc)
}

/// 校验锁屏密码，返回是否匹配。无条目（未设锁）返回 false 而非错误。
pub fn verify_lock_password(pw: &str) -> Result<bool, String> {
    let Some(stored) = platform::get_lock_password()? else {
        return Ok(false);
    };
    Ok(verify_password(&stored, pw))
}

/// 是否存在锁屏密码（凭据库有条目即视为已开启）
pub fn has_lock_password() -> Result<bool, String> {
    Ok(platform::get_lock_password()?.is_some())
}

/// 清空锁屏密码（关闭锁）。无条目也视为成功。
pub fn clear_lock_password() -> Result<(), String> {
    platform::clear_lock_password()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_matches() {
        let phc = hash_password("s3cret-pw").expect("hash");
        assert!(verify_password(&phc, "s3cret-pw"));
        assert!(!verify_password(&phc, "wrong-pw"));
    }

    #[test]
    fn different_passwords_do_not_match() {
        let a = hash_password("alpha123").unwrap();
        let b = hash_password("alpha123").unwrap();
        // 同密码不同 salt → PHC 不同，但仍都能验过自身
        assert_ne!(a, b);
        assert!(verify_password(&a, "alpha123"));
        assert!(verify_password(&b, "alpha123"));
        assert!(!verify_password(&a, "alpha124"));
    }

    #[test]
    fn malformed_phc_verifies_false() {
        assert!(!verify_password("not-a-valid-phc", "anything"));
    }

    /// 走真实凭据库的全链路往返：设→有→验过→验错→清→无。
    /// 同时覆盖「新实例读取」场景（每次都新建底层条目），验证 keyring 在
    /// 非域单机写后读不回的问题已在 Windows 端规避。
    #[test]
    fn roundtrip_set_verify_clear() {
        let pw = "123456";
        assert!(!has_lock_password().unwrap());
        set_lock_password(pw).expect("set");
        assert!(has_lock_password().unwrap());
        assert!(verify_lock_password(pw).unwrap());
        assert!(!verify_lock_password("wrong-pw").unwrap());
        clear_lock_password().expect("clear");
        assert!(!has_lock_password().unwrap());
        assert!(!verify_lock_password(pw).unwrap());
    }
}
