use crate::models::springboot::{SpringApp, SpringAppList, AppGroup, JvmInfo};
use crate::services::springboot_manager::{SpringBootManager, starter};
use anyhow::Result;
use std::sync::Mutex;

static SPRINGBOOT_MANAGER: Mutex<Option<SpringBootManager>> = Mutex::new(None);

/// 获取或初始化SpringBootManager
fn get_manager() -> Result<SpringBootManager, String> {
    let app_root = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot get app directory".to_string())?
        .to_path_buf();

    let mut manager = SPRINGBOOT_MANAGER.lock().unwrap();
    if manager.is_none() {
        *manager = Some(SpringBootManager::new(&app_root));
    }

    Ok(manager.as_ref().unwrap().clone())
}

/// 获取app root和manager的可变引用
fn with_manager<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut SpringBootManager) -> Result<R, String>,
{
    let app_root = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot get app directory".to_string())?
        .to_path_buf();

    let mut manager = SPRINGBOOT_MANAGER.lock().unwrap();
    if manager.is_none() {
        *manager = Some(SpringBootManager::new(&app_root));
    }

    f(manager.as_mut().unwrap())
}

/// 获取app root和manager的可变引用（异步版本）
async fn with_manager_async<F, Fut, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut SpringBootManager) -> Fut,
    Fut: std::future::Future<Output = Result<R, String>>,
{
    let app_root = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot get app directory".to_string())?
        .to_path_buf();

    let mut manager = SPRINGBOOT_MANAGER.lock().unwrap();
    if manager.is_none() {
        *manager = Some(SpringBootManager::new(&app_root));
    }

    f(manager.as_mut().unwrap()).await
}

#[tauri::command]
pub fn list_applications() -> Result<SpringAppList, String> {
    with_manager(|manager| {
        manager.list_applications()
    })
}

#[tauri::command]
pub fn save_application(app: SpringApp) -> Result<(), String> {
    with_manager(|manager| {
        let mut list = manager.list_applications()?;

        if let Some(idx) = list.applications.iter().position(|a| a.id == app.id) {
            list.applications[idx] = app;
        } else {
            list.applications.push(app);
        }

        manager.save_applications(&list)
    })
}

#[tauri::command]
pub fn delete_application(id: String) -> Result<(), String> {
    with_manager(|manager| {
        let mut list = manager.list_applications()?;

        list.applications.retain(|a| a.id != id);

        manager.save_applications(&list)
    })
}

#[tauri::command]
pub fn start_application(id: String) -> Result<(), String> {
    with_manager(|manager| {
        let mut list = manager.list_applications()?;

        if let Some(app) = list.applications.iter_mut().find(|a| a.id == id) {
            manager.start_application(app)?;
            manager.save_applications(&list)
        } else {
            Err("Application not found".to_string())
        }
    })
}

#[tauri::command]
pub fn stop_application(id: String) -> Result<(), String> {
    with_manager(|manager| {
        let mut list = manager.list_applications()?;

        if let Some(app) = list.applications.iter_mut().find(|a| a.id == id) {
            manager.stop_application(app)?;
            manager.save_applications(&list)
        } else {
            Err("Application not found".to_string())
        }
    })
}

#[tauri::command]
pub fn restart_application(id: String) -> Result<(), String> {
    with_manager(|manager| {
        let mut list = manager.list_applications()?;

        if let Some(app) = list.applications.iter_mut().find(|a| a.id == id) {
            manager.stop_application(app)?;
            std::thread::sleep(std::time::Duration::from_secs(1));
            manager.start_application(app)?;
            manager.save_applications(&list)
        } else {
            Err("Application not found".to_string())
        }
    })
}

#[tauri::command]
pub async fn start_all_applications() -> Result<(), String> {
    with_manager_async(|manager| {
        let list = manager.list_applications()?;
        let apps: Vec<SpringApp> = list.applications.clone();

        starter::start_ordered(&apps, &manager.process_manager, |msg| {
            println!("{}", msg);
        }).await.map_err(|e| e.to_string())?;

        Ok(())
    }).await
}

#[tauri::command]
pub fn stop_all_applications() -> Result<(), String> {
    with_manager(|manager| {
        let mut list = manager.list_applications()?;

        for app in list.applications.iter_mut() {
            if app.status == crate::models::software::AppStatus::Running {
                manager.stop_application(app)?;
            }
        }

        manager.save_applications(&list)
    })
}