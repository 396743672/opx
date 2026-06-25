use tauri::State;
use std::sync::Mutex;
use sysinfo::System;
use crate::models::system::{SystemInfo, ProcessInfo, HistoryPoint};
use anyhow::Result;
use crate::services::system_monitor::{self};

static SYSTEM: Mutex<System> = Mutex::new(System::new());

#[tauri::command]
pub fn system_info() -> SystemInfo {
    let mut system = SYSTEM.lock().unwrap();
    system_monitor::info::get_system_info(&mut system)
}

#[tauri::command]
pub fn process_list() -> Vec<ProcessInfo> {
    let mut system = SYSTEM.lock().unwrap();
    system_monitor::info::get_process_list(&mut system)
}

#[tauri::command]
pub fn kill_process(pid: u32) -> Result<bool, String> {
    let success = system_monitor::info::kill_process(pid);
    if success {
        Ok(true)
    } else {
        Err("Failed to kill process".to_string())
    }
}

#[tauri::command]
pub fn system_history() -> Result<Vec<HistoryPoint>, String> {
    let app_dir = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot get app directory".to_string())?;
    let history_path = app_dir.join("data").join("system_history.json");
    let history = system_monitor::history::load_history(&history_path)
        .map_err(|e| e.to_string())?;
    Ok(history)
}