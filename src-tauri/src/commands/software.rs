use tauri::State;
use std::sync::Mutex;
use crate::models::software::{
    SoftwareMeta, InstalledSoftware, InstalledSoftwareList, InstallParams
};
use crate::services::software_manager::SoftwareManager;
use anyhow::Result;

static SOFTWARE_MANAGER: Mutex<Option<SoftwareManager>> = Mutex::new(None);

#[tauri::command]
pub fn list_available_software() -> Vec<SoftwareMeta> {
    let app_root = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default();

    let mut manager = SOFTWARE_MANAGER.lock().unwrap();
    if manager.is_none() {
        *manager = Some(SoftwareManager::new(&app_root));
    }

    manager.as_ref().unwrap().list_available()
}

#[tauri::command]
pub fn list_installed_software() -> Result<Vec<InstalledSoftware>, String> {
    let app_root = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot get app directory".to_string())?
        .to_path_buf();

    let mut manager = SOFTWARE_MANAGER.lock().unwrap();
    if manager.is_none() {
        *manager = Some(SoftwareManager::new(&app_root));
    }

    let list = manager.as_ref().unwrap().list_installed()
        .map_err(|e| e.to_string())?;

    Ok(list.software)
}

#[tauri::command]
pub fn install_software(params: InstallParams) -> Result<(), String> {
    // TODO: implement full installation with download and extract
    Ok(())
}

#[tauri::command]
pub fn start_software(id: String) -> Result<(), String> {
    // TODO: implement start
    Ok(())
}

#[tauri::command]
pub fn stop_software(id: String) -> Result<(), String> {
    // TODO: implement stop
    Ok(())
}

#[tauri::command]
pub fn restart_software(id: String) -> Result<(), String> {
    // TODO: implement restart
    Ok(())
}

#[tauri::command]
pub fn uninstall_software(id: String) -> Result<(), String> {
    // TODO: implement uninstall
    Ok(())
}