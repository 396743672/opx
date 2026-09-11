use anyhow::Result;
use std::path::Path;

use crate::utils::paths;

/// 默认保留天数（spec 第 1284 行明确 7 天）
pub const DEFAULT_RETAIN_DAYS: u64 = 7;

/// 操作日志宏 — 写入独立审计 JSONL（供「操作记录」页查询）
#[macro_export]
macro_rules! oplog {
    ($action:expr, $target:expr) => {
        $crate::services::software_manager::audit::record($action, $target, "");
    };
    ($action:expr, $target:expr, $detail:expr) => {
        $crate::services::software_manager::audit::record($action, $target, $detail);
    };
}

/// 初始化 tracing + 按日 rolling appender + stderr 输出
/// 应在 Tauri setup hook 中调用一次
///
/// 返回 WorkerGuard，调用方必须持有到应用退出，drop 会导致尾部日志丢失。
/// 失败时返回 Err，调用方可记录后继续启动应用（审计日志降级，不阻断）。
pub fn init() -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let log_dir = paths::logs_dir();
    std::fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::daily(&log_dir, "software-manager.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // 双层 fmt layer：文件 appender（无 ANSI）+ stderr（带 ANSI 供终端着色）
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true);

    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer)
        .with(file_layer)
        .try_init()
        .map_err(|e| anyhow::anyhow!("tracing 初始化失败: {}", e))?;

    Ok(guard)
}

/// 清理超过 retain_days 天的日志文件
pub fn cleanup_old_logs(log_dir: &Path, retain_days: u64) {
    // cutoff 取当天 00:00，避免边界日期因当前时刻不同而误删
    let cutoff_date = (chrono::Local::now() - chrono::Duration::days(retain_days as i64))
        .date_naive();
    if let Ok(entries) = std::fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                // 当天活动文件 software-manager.log（无日期后缀）不会被误删
                // （strip_prefix 要求尾随点，无后缀返回 None）
                if let Some(date_str) = name.strip_prefix("software-manager.log.") {
                    if let Ok(date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                        let file_time = date.and_hms_opt(0, 0, 0).expect("valid naive date has midnight");
                        if file_time.date() < cutoff_date {
                            let _ = std::fs::remove_file(entry.path());
                        }
                    }
                }
                // 审计文件 audit-YYYY-MM-DD.jsonl：同规则清理
                if let Some(date_str) = name
                    .strip_prefix("audit-")
                    .and_then(|s| s.strip_suffix(".jsonl"))
                {
                    if let Ok(date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                        if date < cutoff_date {
                            let _ = std::fs::remove_file(entry.path());
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uniq_dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "__qa_auditlog_{}",
            chrono::Local::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn cleanup_removes_old_audit_files_keeps_recent() {
        let dir = uniq_dir();
        let old = dir.join("audit-2000-01-01.jsonl");
        let recent = dir.join(format!(
            "audit-{}.jsonl",
            chrono::Local::now().date_naive().format("%Y-%m-%d")
        ));
        std::fs::write(&old, "{}").unwrap();
        std::fs::write(&recent, "{}").unwrap();

        cleanup_old_logs(&dir, DEFAULT_RETAIN_DAYS);

        assert!(!old.exists(), "old audit file removed");
        assert!(recent.exists(), "recent audit file kept");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
