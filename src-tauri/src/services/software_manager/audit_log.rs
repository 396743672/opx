use anyhow::Result;
use std::path::Path;

use crate::utils::paths;

/// 初始化 tracing + 按日 rolling appender
/// 应在 Tauri setup hook 中调用一次
pub fn init() -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let log_dir = paths::logs_dir();
    std::fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::daily(&log_dir, "software-manager.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    Ok(guard)
}

/// 清理超过 retain_days 天的日志文件
pub fn cleanup_old_logs(log_dir: &Path, retain_days: u64) {
    let cutoff = chrono::Local::now() - chrono::Duration::days(retain_days as i64);
    if let Ok(entries) = std::fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                if let Some(date_str) = name.strip_prefix("software-manager.log.") {
                    if let Ok(date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                        let file_time = date.and_hms_opt(0, 0, 0).unwrap();
                        let cutoff_naive = cutoff.naive_local();
                        if file_time < cutoff_naive {
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
    use std::fs;
    use std::path::PathBuf;

    fn make_old_log(dir: &Path, date_str: &str) -> PathBuf {
        let filename = format!("software-manager.log.{}", date_str);
        let path = dir.join(&filename);
        fs::write(&path, "old log content").unwrap();
        path
    }

    #[test]
    fn cleanup_removes_logs_older_than_retain_days() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        // 30 天前的日志（应被删）
        let old_date = (chrono::Local::now() - chrono::Duration::days(30))
            .format("%Y-%m-%d")
            .to_string();
        let old_path = make_old_log(dir, &old_date);
        assert!(old_path.exists());

        // 3 天前的日志（应保留）
        let recent_date = (chrono::Local::now() - chrono::Duration::days(3))
            .format("%Y-%m-%d")
            .to_string();
        let recent_path = make_old_log(dir, &recent_date);
        assert!(recent_path.exists());

        cleanup_old_logs(dir, 7);

        assert!(!old_path.exists(), "30 天前的日志应被删除");
        assert!(recent_path.exists(), "3 天前的日志应保留");
    }

    #[test]
    fn cleanup_ignores_files_not_matching_pattern() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        let other = dir.join("other.log");
        fs::write(&other, "content").unwrap();

        // 100 天前的非匹配文件
        let old_date = (chrono::Local::now() - chrono::Duration::days(100))
            .format("%Y-%m-%d")
            .to_string();
        let filename = format!("not-our-log.{}", old_date);
        let path = dir.join(&filename);
        fs::write(&path, "content").unwrap();

        cleanup_old_logs(dir, 7);

        assert!(other.exists(), "无关文件应保留");
        assert!(path.exists(), "非匹配前缀的文件应保留");
    }

    #[test]
    fn cleanup_handles_nonexistent_dir() {
        // 不存在的目录应不 panic
        cleanup_old_logs(std::path::Path::new("/nonexistent/audit/log/dir"), 7);
    }
}
