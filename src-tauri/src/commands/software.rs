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

    // 调用各 provider 的 fetch_remote_versions，与内置版本合并
    let providers = providers::all_providers();
    for entry in &mut merged.entries {
        if let Some(provider) = providers.iter().find(|p| p.key() == entry.key) {
            let remote_versions = provider.fetch_remote_versions();
            entry.versions = catalog::merge_versions(entry.versions.clone(), remote_versions);
        }
    }

    merged.updated_at = Some(chrono::Local::now().to_rfc3339());
    manager.set_catalog(merged.clone());
    let _ = app.emit("catalog-refreshed", merged.entries.clone());
    Ok(merged.entries)
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
