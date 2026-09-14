use crate::services::software_manager::audit;

/// 查询操作记录（分页：默认每页 50 条；total 为过滤后总数，truncated 表示还有下一页）
#[tauri::command]
pub fn list_audit_entries(
    days: u64,
    action: Option<String>,
    keyword: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> audit::AuditQuery {
    let days = days.clamp(1, 31);
    audit::query(
        days,
        action.as_deref(),
        keyword.as_deref(),
        limit.unwrap_or(50),
        offset.unwrap_or(0),
    )
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
