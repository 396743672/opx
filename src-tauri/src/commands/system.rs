use crate::models::system::{SystemInfo, HistoryPoint};
use crate::services::system_monitor;
use crate::utils::paths;
use anyhow::Result;

#[tauri::command]
pub fn system_info() -> SystemInfo {
    crate::services::system_monitor::info::sample_system()
}

#[tauri::command]
pub fn system_history() -> Result<Vec<HistoryPoint>, String> {
    let path = paths::data_dir().join("metrics_history.json");
    let h = system_monitor::history::load_metrics(&path).map_err(|e| e.to_string())?;
    Ok(h.system)
}

/// 各进程（pid 字符串键）的持久化样本，供 Dashboard 行内趋势图。
#[tauri::command]
pub fn process_metrics_history() -> Result<std::collections::HashMap<String, Vec<HistoryPoint>>, String> {
    let path = paths::data_dir().join("metrics_history.json");
    let h = system_monitor::history::load_metrics(&path).map_err(|e| e.to_string())?;
    Ok(h.processes)
}
