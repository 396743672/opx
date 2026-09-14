use crate::models::system::{SystemInfo, HistoryPoint};
use anyhow::Result;

#[tauri::command]
pub fn system_info() -> SystemInfo {
    crate::services::system_monitor::info::sample_system()
}

#[tauri::command]
pub fn system_history() -> Result<Vec<HistoryPoint>, String> {
    // ponytail: 暂返回空，历史接口由后续任务改造为读取 MetricsHistory
    Ok(Vec::new())
}
