//! DDNS 后台调度：固定 5 分钟一轮；未启用时空转跳过，
//! 设置每轮重新读取，故启用/停用或改域名即时生效、无需重启（同 `recorder.rs` 的 interval 用法）。

use std::time::Duration;

use super::sync_once;

pub const DDNS_INTERVAL_SECS: u64 = 300;

pub async fn run_ddns_scheduler() {
    let mut tick = tokio::time::interval(Duration::from_secs(DDNS_INTERVAL_SECS));
    // 休眠/挂起恢复后不补打遗漏的 tick
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    tick.tick().await; // 消耗初始化 tick
    loop {
        tick.tick().await;
        // 每轮重读设置：调度器本身永不休眠，停用只是本轮不干活
        let s = crate::commands::config::read_settings().unwrap_or_default();
        if !s.ddns_enabled {
            continue;
        }
        match sync_once(&s).await {
            // 部分域名失败时本轮仍算完成：用 warn 让日志能直接筛出「有失败的一轮」，
            // 不必从 changes 文案里数失败条数
            Ok(r) if r.failures > 0 => {
                tracing::warn!(
                    failures = r.failures,
                    changes = ?r.changes,
                    "DDNS 同步完成，但有记录失败"
                )
            }
            Ok(r) if !r.changes.is_empty() => {
                tracing::info!(changes = ?r.changes, "DDNS 同步完成")
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "DDNS 同步失败（下轮重试）"),
        }
    }
}
