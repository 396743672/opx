use crate::services::software_manager::audit;

/// 查询操作记录（默认上限 2000 条，超出以 truncated 标记）
#[tauri::command]
pub fn list_audit_entries(
    days: u64,
    action: Option<String>,
    keyword: Option<String>,
    limit: Option<usize>,
) -> audit::AuditQuery {
    let days = days.clamp(1, 31);
    audit::query(days, action.as_deref(), keyword.as_deref(), limit.unwrap_or(2000))
}

/// 操作记录概览统计
#[tauri::command]
pub fn audit_stats(days: u64) -> audit::AuditStats {
    audit::stats(days.clamp(1, 31))
}

/// 按过滤条件导出 CSV 到 dest_path
#[tauri::command]
pub fn export_audit_entries(
    days: u64,
    action: Option<String>,
    keyword: Option<String>,
    dest_path: String,
) -> Result<(), String> {
    let days = days.clamp(1, 31);
    audit::export_csv(days, action.as_deref(), keyword.as_deref(), &dest_path)
}
