use anyhow::Result;
use crate::services::service_registry;
use crate::models::software::{self, InstalledSoftware};
use crate::models::springboot;
use std::sync::Mutex;
use tokio::runtime::Runtime;

// 全局管理器引用
static SOFTWARE_MANAGER: Mutex<Option<crate::services::software_manager::SoftwareManager>> = Mutex::new(None);
static SPRINGBOOT_MANAGER: Mutex<Option<crate::services::springboot_manager::SpringBootManager>> = Mutex::new(None);

#[tauri::command]
pub fn toggle_opx_system_service(enable: bool) -> Result<(), String> {
    if enable {
        service_registry::register_current()
            .map_err(|e| e.to_string())
    } else {
        service_registry::unregister_current()
            .map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn auto_start_enabled() -> Result<bool, String> {
    let app_root = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot get app directory".to_string())?
        .to_path_buf();

    let settings_path = app_root.join("config").join("settings.json");
    if !settings_path.exists() {
        return Ok(false);
    }

    let settings: crate::models::settings::AppSettings = crate::utils::file::read_json(&settings_path)
        .unwrap_or_default();

    Ok(settings.auto_start_managed_services)
}

#[tauri::command]
pub fn start_auto_started() -> Result<(), String> {
    let app_root = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot get app directory".to_string())?
        .to_path_buf();

    // Read installed software
    let software_path = app_root.join("config").join("installed.json");
    let mut software_list: software::InstalledSoftwareList = if software_path.exists() {
        crate::utils::file::read_json(&software_path)
            .map_err(|e| format!("Failed to read installed software: {}", e))?
    } else {
        software::InstalledSoftwareList::default()
    };

    // Read springboot applications
    let sb_path = app_root.join("config").join("springboot.json");
    let mut sb_list: springboot::SpringAppList = if sb_path.exists() {
        crate::utils::file::read_json(&sb_path)
            .map_err(|e| format!("Failed to read springboot applications: {}", e))?
    } else {
        springboot::SpringAppList::default()
    };

    // Collect all enabled apps by startup_order
    let mut all_enabled = Vec::new();

    // Add software
    for sw in software_list.software {
        if sw.auto_start_on_app_start {
            all_enabled.push((sw.startup_order, sw.id.clone(), "software"));
        }
    }

    // Add springboot applications
    for app in sb_list.applications {
        if app.auto_start_on_app_start {
            all_enabled.push((app.startup_order, app.id.clone(), "springboot"));
        }
    }

    // Sort by startup_order (ascending)
    all_enabled.sort_by_key(|(order, _, _)| *order);

    // Create tokio runtime
    let rt = Runtime::new()
        .map_err(|e| format!("Failed to create tokio runtime: {}", e))?;

    rt.block_on(async {
        // Group by startup_order
        use std::collections::BTreeMap;
        let mut ordered: BTreeMap<u32, Vec<(String, &str)>> = BTreeMap::new();

        for (order, id, typ) in all_enabled {
            ordered.entry(order).or_default().push((id, typ));
        }

        // Start layer by layer
        for (order, items) in ordered {
            println!("Starting layer {} with {} items", order, items.len());

            // Parallel start all in this layer
            let mut futures = Vec::new();

            for (id, typ) in items {
                futures.push(async move {
                    match typ {
                        "software" => {
                            let mut software_manager = SOFTWARE_MANAGER.lock().unwrap();
                            if software_manager.is_none() {
                                let app_root = std::env::current_exe()
                                    .unwrap()
                                    .parent()
                                    .unwrap()
                                    .to_path_buf();
                                *software_manager = Some(crate::services::software_manager::SoftwareManager::new(&app_root));
                            }

                            if let Some(manager) = software_manager.as_mut() {
                                let mut list = manager.list_installed().unwrap_or_default();
                                if let Some(sw) = list.software.iter_mut().find(|s| s.id == id) {
                                    let _ = manager.start_software(sw);
                                }
                            }
                        }
                        "springboot" => {
                            let mut sb_manager = SPRINGBOOT_MANAGER.lock().unwrap();
                            if sb_manager.is_none() {
                                let app_root = std::env::current_exe()
                                    .unwrap()
                                    .parent()
                                    .unwrap()
                                    .to_path_buf();
                                *sb_manager = Some(crate::services::springboot_manager::SpringBootManager::new(&app_root));
                            }

                            if let Some(manager) = sb_manager.as_mut() {
                                let mut list = manager.list_applications().unwrap_or_default();
                                if let Some(app) = list.applications.iter_mut().find(|a| a.id == id) {
                                    let _ = manager.start_application(app);
                                }
                            }
                        }
                        _ => {}
                    }
                });
            }

            // Wait for all in this layer to complete
            for future in futures {
                let _ = future.await;
            }
        }
    });

    Ok(())
}