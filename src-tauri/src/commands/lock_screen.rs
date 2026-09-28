use crate::services::lock_screen as ls;

/// 设置或更新锁屏密码（≥6 位）。空锁→开启，已有锁→更换。
#[tauri::command]
pub fn set_lock_password(pw: String) -> Result<(), String> {
    ls::set_lock_password(&pw)
}

/// 校验锁屏密码，返回是否匹配。未设锁时返回 false。
#[tauri::command]
pub fn verify_lock_password(pw: String) -> Result<bool, String> {
    ls::verify_lock_password(&pw)
}

/// 是否存在锁屏密码（即锁是否开启）
#[tauri::command]
pub fn has_lock_password() -> Result<bool, String> {
    ls::has_lock_password()
}

/// 清空锁屏密码（关闭锁）。无密码也视为成功。
#[tauri::command]
pub fn clear_lock_password() -> Result<(), String> {
    ls::clear_lock_password()
}
