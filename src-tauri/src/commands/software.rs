use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::models::software::{
    CatalogEntry, CustomInstallParams, InstallParams, InstalledSoftware,
};
use crate::services::software_manager::{catalog, installer, providers, SoftwareManager};

/// 获取可安装软件列表（catalog）
#[tauri::command]
pub async fn list_available_software(
    manager: State<'_, Arc<SoftwareManager>>,
) -> Result<Vec<CatalogEntry>, String> {
    Ok(manager.get_catalog().entries)
}

/// 刷新 catalog：重新拉取远程并合并
#[tauri::command]
pub async fn refresh_catalog(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
) -> Result<Vec<CatalogEntry>, String> {
    let builtin = catalog::build_builtin_catalog();

    // 从 settings 读取 mirror_url（远程 catalog.json，可选）
    let mirror_url = {
        let sp = crate::utils::paths::settings_path();
        if sp.exists() {
            std::fs::read_to_string(&sp)
                .ok()
                .and_then(|c| serde_json::from_str::<crate::models::settings::AppSettings>(&c).ok())
                .map(|s| s.mirror_url)
                .unwrap_or_else(|| "https://mirrors.aliyun.com".to_string())
        } else {
            "https://mirrors.aliyun.com".to_string()
        }
    };
    let remote_catalog = catalog::fetch_remote_catalog(&mirror_url).await;
    let mut merged = catalog::merge_catalogs(builtin, remote_catalog);

    merged.updated_at = Some(chrono::Local::now().to_rfc3339());
    manager.set_catalog(merged.clone());
    let _ = app.emit("catalog-refreshed", merged.entries.clone());
    Ok(merged.entries)
}

/// 获取指定软件的远程版本列表（前端打开安装对话框时调用）
/// 返回包装后的 CatalogEntry（含远程版本），失败返回错误信息供前端展示
#[tauri::command]
pub async fn fetch_remote_versions_for(
    manager: State<'_, Arc<SoftwareManager>>,
    key: String,
) -> Result<Vec<CatalogEntry>, String> {
    // 用 spawn_blocking 调用 provider 的 sync fetch_remote_versions
    // 避免 reqwest::blocking 在 async 上下文中 panic
    let key_for_blocking = key.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let providers = providers::all_providers();
        let provider = providers
            .iter()
            .find(|p| p.key() == key_for_blocking)
            .ok_or_else(|| format!("未知软件: {}", key_for_blocking))?;
        Ok::<_, String>(provider.fetch_remote_versions())
    })
    .await
    .map_err(|e| format!("获取版本列表失败: {}", e))?;

    match result {
        Ok(Some(versions)) => {
            // 关键：把远程版本合并进 manager 的 catalog，
            // 否则安装时 install_software 从 catalog 查不到网络版本，报"不支持版本"
            manager.merge_entry_versions(&key, versions.clone());

            // 包装成单条 CatalogEntry 返回（前端合并展示）
            let providers = providers::all_providers();
            let provider = providers.iter().find(|p| p.key() == key);
            if let Some(p) = provider {
                let mut entry = p.catalog_entry();
                entry.versions = versions;
                Ok(vec![entry])
            } else {
                Err(format!("未知软件: {}", key))
            }
        }
        Ok(None) => {
            // 该软件不支持动态拉取（如 MySQL/MinIO/RustFS），返回空
            Ok(vec![])
        }
        Err(e) => Err(e),
    }
}

/// 获取已安装软件列表
#[tauri::command]
pub async fn list_installed_software(
    manager: State<'_, Arc<SoftwareManager>>,
) -> Result<Vec<InstalledSoftware>, String> {
    Ok(manager.get_installed())
}

/// 安装预置软件（在线镜像）
#[tauri::command]
pub async fn install_software(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
    params: InstallParams,
) -> Result<String, String> {
    let install_id = uuid::Uuid::new_v4().to_string();
    let manager_arc: Arc<SoftwareManager> = manager.inner().clone();
    let install_id_for_task = install_id.clone();
    tauri::async_runtime::spawn(async move {
        installer::install_software(app, manager_arc, params, install_id_for_task).await;
    });
    Ok(install_id)
}

/// 安装用户上传的自定义压缩包
#[tauri::command]
pub async fn install_custom(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
    params: CustomInstallParams,
) -> Result<String, String> {
    let install_id = uuid::Uuid::new_v4().to_string();
    let manager_arc: Arc<SoftwareManager> = manager.inner().clone();
    let install_id_for_task = install_id.clone();
    tauri::async_runtime::spawn(async move {
        installer::install_custom(app, manager_arc, params, install_id_for_task).await;
    });
    Ok(install_id)
}

/// 卸载已安装软件
#[tauri::command]
pub async fn uninstall_software(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
) -> Result<bool, String> {
    manager
        .remove_installed(&installed_id)
        .map(|_| true)
        .map_err(|e| e.to_string())
}
