use crate::models::springboot::{SpringApp, SpringAppList, AppGroup, JvmInfo};
use crate::services::springboot_manager::SpringBootManager;
use anyhow::Result;
use std::sync::Mutex;

static SPRINGBOOT_MANAGER: Mutex<Option<SpringBootManager>> = Mutex::new(None);

#[tauri::command]
pub fn list_applications() -> Result<SpringAppList, String> {
    let app_root = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot get app directory".to_string())?
        .to_path_buf();

    let mut manager = SPRINGBOOT_MANAGER.lock().unwrap();
    if manager.is_none() {
        *manager = Some(SpringBootManager::new(&app_root));
    }

    let list = manager.as_ref().unwrap().list_applications()
        .map_err(|e| e.to_string())?;

    Ok(list)
}

#[tauri::command]
pub fn save_application(app: SpringApp) -> Result<(), String> {
    let app_root = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot get app directory".to_string())?
        .to_path_buf();

    let mut manager = SPRINGBOOT_MANAGER.lock().unwrap();
    if manager.is_none() {
        *manager = Some(SpringBootManager::new(&app_root));
    }

    let mut list = manager.as_ref().unwrap().list_applications()
        .map_err(|e| e.to_string())?;

    if let Some(idx) = list.applications.iter().position(|a| a.id == app.id) {
        list.applications[idx] = app;
    } else {
        list.applications.push(app);
    }

    manager.as_ref().unwrap().save_applications(&list)
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn delete_application(id: String) -> Result<(), String> {
    let app_root = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot get app directory".to_string())?
        .to_path_buf();

    let mut manager = SPRINGBOOT_MANAGER.lock().unwrap();
    if manager.is_none() {
        *manager = Some(SpringBootManager::new(&app_root));
    }

    let mut list = manager.as_ref().unwrap().list_applications()
        .map_err(|e| e.to_string())?;

    list.applications.retain(|a| a.id != id);

    manager.as_ref().unwrap().save_applications(&list)
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn start_application(id: String) -> Result<(), String> {
    let app_root = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot get app directory".to_string())?
        .to_path_buf();

    let mut manager = SPRINGBOOT_MANAGER.lock().unwrap();
    if manager.is_none() {
        *manager = Some(SpringBootManager::new(&app_root));
    }

    let mut list = manager.as_ref().unwrap().list_applications()
        .map_err(|e| e.to_string())?;

    if let Some(app) = list.applications.iter_mut().find(|a| a.id == id) {
        manager.as_ref().unwrap().start_application(app)
            .map_err(|e| e.to_string())?;
        manager.as_ref().unwrap().save_applications(&list)
            .map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("Application not found".to_string())
    }
}

#[tauri::command]
pub fn stop_application(id: String) -> Result<(), String> {
    let app_root = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot get app directory".to_string())?
        .to_path_buf();

    let mut manager = SPRINGBOOT_MANAGER.lock().unwrap();
    if manager.is_none() {
        *manager = Some(SpringBootManager::new(&app_root));
    }

    let mut list = manager.as_ref().unwrap().list_applications()
        .map_err(|e| e.to_string())?;

    if let Some(app) = list.applications.iter_mut().find(|a| a.id == id) {
        manager.as_ref().unwrap().stop_application(app)
            .map_err(|e| e.to_string())?;
        manager.as_ref().unwrap().save_applications(&list)
            .map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("Application not found".to_string())
    }
}

#[tauri::command]
pub fn restart_application(id: String) -> Result<(), String> {
    stop_application(id.clone())?;
    std::thread::sleep(std::time::Duration::from_secs(1));
    start_application(id)
}

#[tauri::command]
pub fn start_all_applications() -> Result<(), String> {
    // TODO: use starter::start_ordered
    Ok(())
}

#[tauri::command]
pub fn stop_all_applications() -> Result<(), String> {
    let app_root = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot get app directory".to_string())?
        .to_path_buf();

    let mut manager = SPRINGBOOT_MANAGER.lock().unwrap();
    if manager.is_none() {
        *manager = Some(SpringBootManager::new(&app_root));
    }

    let list = manager.as_ref().unwrap().list_applications()
        .map_err(|e| e.to_string())?;

    for app in list.applications {
        if app.status == crate::models::software::AppStatus::Running {
            let _ = manager.as_ref().unwrap().stop_application(&mut app.clone());
        }
    }

    Ok(())
}