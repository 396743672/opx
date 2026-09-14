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

/// 后台任务发起：记一条「进行中」（result = running）。
#[macro_export]
macro_rules! oplog_begin {
    ($action:expr, $target:expr) => {
        $crate::oplog_begin!($action, $target, "");
    };
    ($action:expr, $target:expr, $detail:expr) => {
        $crate::services::software_manager::audit::record_full(
            $action,
            $target,
            $detail,
            $crate::services::software_manager::audit::RESULT_RUNNING,
            "",
        );
    };
}

/// 同步操作完成：按 Result 记 ok / fail + 错误原因。
#[macro_export]
macro_rules! oplog_result {
    ($action:expr, $target:expr, $detail:expr, $res:expr) => {{
        let (__result, __err) = $crate::services::software_manager::audit::classify(&$res);
        $crate::services::software_manager::audit::record_full(
            $action, $target, $detail, __result, __err,
        );
    }};
}

/// 已知失败原因时直接记 fail（如命令前的同步校验被拒）。
#[macro_export]
macro_rules! oplog_fail {
    ($action:expr, $target:expr, $detail:expr, $err:expr) => {
        $crate::services::software_manager::audit::record_full(
            $action,
            $target,
            $detail,
            $crate::services::software_manager::audit::RESULT_FAIL,
            $err,
        );
    };
}

/// 用闭包包住同步命令体：`?` 与早 `return` 都归属闭包，退出时统一记结果。
/// 用法：`audited!("action", target, "", { ...body... })`（须作为函数尾表达式）。
#[macro_export]
macro_rules! audited {
    ($action:expr, $target:expr, $detail:expr, $body:block) => {{
        // 先求值为 owned String：避免 action/target 的借用与体内的 move 冲突
        let __act = ($action).to_string();
        let __tgt = ($target).to_string();
        let __det = ($detail).to_string();
        let __r = (|| $body)();
        $crate::oplog_result!(__act, __tgt, __det, __r);
        __r
    }};
}

/// 同 `audited!`，用于 async 命令体（体内 `?` / `return` 归属 async 块）。
#[macro_export]
macro_rules! audited_async {
    ($action:expr, $target:expr, $detail:expr, $body:block) => {{
        let __act = ($action).to_string();
        let __tgt = ($target).to_string();
        let __det = ($detail).to_string();
        let __r = (async $body).await;
        $crate::oplog_result!(__act, __tgt, __det, __r);
        __r
    }};
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
    use crate::services::software_manager::audit::{
        query, RESULT_FAIL, RESULT_OK, RESULT_RUNNING,
    };

    /// 唯一标记，避免测试与真实记录/其他测试互相干扰。
    fn uniq() -> String {
        format!("__qa_{}", chrono::Local::now().timestamp_nanos_opt().unwrap_or(0))
    }

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

    /// 五个宏的取值契约：begin→running、result→ok/fail、fail→fail+原因。
    #[test]
    fn result_macros_write_expected_result_and_error() {
        let u = uniq();

        oplog_begin!("__qa_macro", format!("{u}_begin"));
        oplog_begin!("__qa_macro", format!("{u}_begin3"), "d");
        oplog_fail!("__qa_macro", format!("{u}_fail"), "d", format!("{u}_boom"));
        let ok: Result<(), String> = Ok(());
        oplog_result!("__qa_macro", format!("{u}_ok"), "d", ok);
        let bad: Result<(), String> = Err(format!("{u}_bad"));
        oplog_result!("__qa_macro", format!("{u}_bad"), "d", bad);

        let q = query(1, Some("__qa_macro"), Some(&u), None, 100, 0);
        let find = |suffix: &str| {
            q.entries
                .iter()
                .find(|e| e.target == format!("{u}_{suffix}"))
                .expect("macro entry found")
        };
        assert_eq!(find("begin").result, RESULT_RUNNING);
        assert_eq!(find("begin").error, "");
        assert_eq!(find("begin3").result, RESULT_RUNNING);
        assert_eq!(find("fail").result, RESULT_FAIL);
        assert_eq!(find("fail").error, format!("{u}_boom"));
        assert_eq!(find("ok").result, RESULT_OK);
        assert_eq!(find("ok").error, "");
        assert_eq!(find("bad").result, RESULT_FAIL);
        assert_eq!(find("bad").error, format!("{u}_bad"));
    }

    /// audited! 包装体：`?` 与早 `return` 归属闭包，返回值原样透出并记录结果。
    #[test]
    fn audited_records_body_result_and_returns_it() {
        let u = uniq();

        fn run(fail: bool, u: &str) -> Result<u8, String> {
            audited!("__qa_audited", format!("{u}_sync"), "d", {
                if fail {
                    return Err(format!("{u}_early"));
                }
                let n = Ok::<u8, String>(7)?;
                Ok(n)
            })
        }

        assert_eq!(run(false, &u).unwrap(), 7, "返回值原样透出");
        assert_eq!(run(true, &u).unwrap_err(), format!("{u}_early"));

        let q = query(1, Some("__qa_audited"), Some(&u), None, 100, 0);
        assert_eq!(q.entries.len(), 2);
        // ts 倒序：后写入的 fail 在前
        assert_eq!(q.entries[0].result, RESULT_FAIL);
        assert_eq!(q.entries[0].error, format!("{u}_early"));
        assert_eq!(q.entries[1].result, RESULT_OK);
        assert_eq!(q.entries[1].error, "");
    }

    /// audited_async! 同上，用于 async 函数体。
    #[tokio::test]
    async fn audited_async_records_body_result_and_returns_it() {
        let u = uniq();

        async fn run(fail: bool, u: &str) -> Result<u8, String> {
            audited_async!("__qa_audited_async", format!("{u}_async"), "d", {
                if fail {
                    return Err(format!("{u}_early"));
                }
                let n = Ok::<u8, String>(9)?;
                Ok(n)
            })
        }

        assert_eq!(run(false, &u).await.unwrap(), 9);
        assert_eq!(run(true, &u).await.unwrap_err(), format!("{u}_early"));

        let q = query(1, Some("__qa_audited_async"), Some(&u), None, 100, 0);
        assert_eq!(q.entries.len(), 2);
        assert_eq!(q.entries[0].result, RESULT_FAIL);
        assert_eq!(q.entries[0].error, format!("{u}_early"));
        assert_eq!(q.entries[1].result, RESULT_OK);
    }
}
