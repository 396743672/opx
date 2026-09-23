use crate::models::system::{SystemInfo, HistoryPoint};
use crate::services::system_monitor;
use anyhow::Result;

#[tauri::command]
pub fn system_info() -> SystemInfo {
    crate::services::system_monitor::info::sample_system()
}

#[tauri::command]
pub fn system_history() -> Result<Vec<HistoryPoint>, String> {
    // 文件缺失/损坏都降级为空曲线，不把错误抛给前端（采样器侧会对损坏文件留 warn 并跳过落盘）
    let h = system_monitor::history::load_metrics(&system_monitor::history::metrics_path())
        .unwrap_or_default();
    Ok(h.system)
}

/// 各进程（pid 字符串键）的持久化样本，供 Dashboard 行内趋势图。
#[tauri::command]
pub fn process_metrics_history() -> Result<std::collections::HashMap<String, Vec<HistoryPoint>>, String> {
    let h = system_monitor::history::load_metrics(&system_monitor::history::metrics_path())
        .unwrap_or_default();
    Ok(h.processes)
}

/// 最近一次应用启动编排报告（无记录返回 null，前端展示空态）
#[tauri::command]
pub fn get_last_startup_report() -> Option<crate::models::startup::StartupReport> {
    crate::services::startup_bootstrap::last_report()
}
