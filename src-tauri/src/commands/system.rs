use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use sysinfo::System;
use crate::models::system::{SystemInfo, HistoryPoint};
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
pub fn system_history() -> Result<Vec<HistoryPoint>, String> {
    // ponytail: 暂返回空，历史接口由后续任务改造为读取 MetricsHistory
    Ok(Vec::new())
}
