use anyhow::Result;
use std::path::Path;

use crate::utils::paths;

/// 默认保留天数（spec 第 1284 行明确 7 天）
pub const DEFAULT_RETAIN_DAYS: u64 = 7;

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
    fn cleanup_preserves_active_log_file_without_date_suffix() {
        // 当天活动文件 software-manager.log（无日期后缀）不应被误删
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        let active = dir.join("software-manager.log");
        fs::write(&active, "active log").unwrap();

        // 同时放一个 100 天前的日志确保 cleanup 真的执行了
        let old_date = (chrono::Local::now() - chrono::Duration::days(100))
            .format("%Y-%m-%d")
            .to_string();
        let old_path = make_old_log(dir, &old_date);

        cleanup_old_logs(dir, 7);

        assert!(active.exists(), "当天活动文件不应被误删");
        assert!(!old_path.exists(), "100 天前的日志应被删除（验证 cleanup 确实执行了）");
    }

    #[test]
    fn cleanup_preserves_boundary_date_within_retain_days() {
        // 刚好 7 天前的文件（retain_days=7）应保留，因为 cutoff 是 now - 7 天
        // 而 file_time 是当天 00:00，应 >= cutoff_date
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        let boundary_date = (chrono::Local::now() - chrono::Duration::days(7))
            .format("%Y-%m-%d")
            .to_string();
        let boundary_path = make_old_log(dir, &boundary_date);

        cleanup_old_logs(dir, 7);

        // 7 天前的文件应保留（cutoff_date = today - 7 天，file_date = today - 7 天，
        // file_date < cutoff_date 为 false，因为两者是同一天）
        assert!(boundary_path.exists(), "刚好 7 天前的日志应保留");
    }

    #[test]
    fn cleanup_handles_nonexistent_dir() {
        // 不存在的目录应不 panic
        cleanup_old_logs(std::path::Path::new("/nonexistent/audit/log/dir"), 7);
    }

    #[test]
    fn default_retain_days_is_seven() {
        assert_eq!(DEFAULT_RETAIN_DAYS, 7);
    }
}
