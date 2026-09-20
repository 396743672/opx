//! 更新相关 Tauri 命令与启动自动检查。

use crate::utils::update::{check_for_update, install_update, UpdateCheckResult};
use tauri::{AppHandle, Emitter};

/// 前端「检查更新」按钮：返回是否有可用更新及版本信息。
#[tauri::command]
pub async fn check_app_update(app: AppHandle) -> Result<UpdateCheckResult, String> {
    check_for_update(&app)
        .await
        .map_err(|e| format!("{:#}", e))
}

/// 前端「立即更新」按钮：下载并安装，进度通过 `update-progress` 事件推送。
#[tauri::command]
pub async fn install_app_update(app: AppHandle) -> Result<(), String> {
    install_update(&app).await.map_err(|e| format!("{:#}", e))
}

/// 启动后自动检查（受 `auto_check_update` 设置控制）。仅在有更新时推送 `update-available` 事件，
/// 失败静默忽略（不该因网络问题打扰用户）。
pub async fn auto_check(app: AppHandle) {
    match check_for_update(&app).await {
        Ok(r) if r.available => {
            let _ = app.emit("update-available", r);
        }
        Ok(_) => {}
        Err(e) => tracing::debug!(error = %e, "自动检查更新失败（已忽略）"),
    }
}
