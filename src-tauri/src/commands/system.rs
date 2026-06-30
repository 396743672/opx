use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use sysinfo::System;
use crate::models::system::{SystemInfo, ProcessInfo, HistoryPoint};
use crate::services::system_monitor;
use anyhow::Result;

/// 初始化时 refresh 两次（间隔 200ms）建立 CPU 采样基准，
/// 避免首次调用 system_info 时 cpu_usage 返回 0 或异常满值。
static SYSTEM: Lazy<Mutex<System>> = Lazy::new(|| {
    let mut s = System::new();
    s.refresh_all();
    thread::sleep(Duration::from_millis(200));
    s.refresh_all();
    Mutex::new(s)
});

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
    let current_exe = std::env::current_exe()
        .map_err(|e| e.to_string())?;
    let app_dir = current_exe
        .parent()
        .ok_or("Cannot get app directory".to_string())?;
    let history_path = app_dir.join("data").join("system_history.json");
    let history = system_monitor::history::load_history(&history_path)
        .map_err(|e| e.to_string())?;
    Ok(history)
}
