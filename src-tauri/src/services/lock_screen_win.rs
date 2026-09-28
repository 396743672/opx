//! 锁屏凭据后端（Windows）：直接用 `windows-sys` 访问 Credential Manager。
//!
//! 为什么不用 keyring：keyring 3.x 在 Windows 上把凭据持久化写死为
//! `CRED_PERSIST_ENTERPRISE`，在「非域单机」环境下 `CredWriteW` 虽返回成功，但
//! `CredReadW` 对同一 `target` 的新实例/重启进程返回 `ERROR_NOT_FOUND`（NoEntry），
//! 导致设了密码却永远读不回、解不开锁。改用 `CRED_PERSIST_LOCAL_MACHINE` 后，凭据对
//! 同用户所有进程可见、可跨重启读取，满足锁屏需求。
//!
//! 存储内容：Argon2id 的 PHC verifier 字符串（纯 ASCII）。`target` 固定为 `opx.lock-screen`。

use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, FILETIME, GetLastError};
use windows_sys::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
    CRED_TYPE_GENERIC,
};

/// 固定 target 名（等价于 keyring 的 (service, user) 概念）
const TARGET: &str = "opx.lock-screen";

fn to_wstr(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 把 verifier 写入 Credential Manager（UTF-8 字节存入 blob）
fn persist(secret: &str) -> Result<(), String> {
    let mut target_w = to_wstr(TARGET);
    let mut blob = secret.as_bytes().to_vec();
    let cred = CREDENTIALW {
        Flags: 0,
        Type: CRED_TYPE_GENERIC,
        TargetName: target_w.as_mut_ptr(),
        Comment: null_mut(),
        LastWritten: FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        },
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: null_mut(),
        TargetAlias: null_mut(),
        UserName: null_mut(),
    };
    let ret = unsafe { CredWriteW(&cred, 0) };
    // 明文 verifier 已写入凭据库，清零本地副本
    for b in blob.iter_mut() {
        *b = 0;
    }
    if ret == 0 {
        Err("i18n:lockKeyringUnavailable".to_string())
    } else {
        Ok(())
    }
}

/// 写入已算好的 PHC verifier
pub fn set_lock_password(phc: &str) -> Result<(), String> {
    persist(phc)
}

/// 读取 PHC；无条目返回 None，其他错误上报
pub fn get_lock_password() -> Result<Option<String>, String> {
    let mut target_w = to_wstr(TARGET);
    let mut p: *mut CREDENTIALW = null_mut();
    let ret = unsafe { CredReadW(target_w.as_mut_ptr(), CRED_TYPE_GENERIC, 0, &mut p) };
    if ret == 0 {
        // 区分「无条目」与「其他错误」：无条目属正常（未设锁）
        return match unsafe { GetLastError() } {
            ERROR_NOT_FOUND => Ok(None),
            _ => Err("i18n:lockKeyringUnavailable".to_string()),
        };
    }
    unsafe {
        let c = *p;
        let len = c.CredentialBlobSize as usize;
        let slice = std::slice::from_raw_parts(c.CredentialBlob, len);
        let s = String::from_utf8(slice.to_vec())
            .map_err(|_| "i18n:lockKeyringUnavailable".to_string())?;
        CredFree(p as *mut _);
        Ok(Some(s))
    }
}

/// 清空凭据；无条目也视为成功
pub fn clear_lock_password() -> Result<(), String> {
    let mut target_w = to_wstr(TARGET);
    let ret = unsafe { CredDeleteW(target_w.as_mut_ptr(), CRED_TYPE_GENERIC, 0) };
    if ret != 0 {
        return Ok(());
    }
    // 删除失败：若本就无条目则视为成功，否则上报
    match get_lock_password() {
        Ok(Some(_)) => Err("i18n:lockKeyringUnavailable".to_string()),
        _ => Ok(()),
    }
}
