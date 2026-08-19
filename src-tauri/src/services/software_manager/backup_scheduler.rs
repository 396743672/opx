//! 定时自动备份调度器（Roadmap 2 · R1）
//!
//! 独立于手动快照，按实例配置的分钟间隔自动触发 `backup::create_snapshot`（Hot 模式，
//! 避免停机）。调度配置持久化于 `<app_data>/backup_schedules.json`：`{ installed_id: minutes }`，
//! `minutes` 为 0/缺失表示关闭。
//!
//! 判断"是否到期"复用已有 manifest 的最新快照时间（不做额外持久化状态）：
//! 距最新快照超过配置间隔即触发。

use std::collections::HashMap;
use std::path::PathBuf;

use tokio::time::{Duration, Instant};

use crate::models::software::BackupMode;
use crate::services::software_manager::{backup, SoftwareManager};
use crate::utils::paths;

/// 调度配置文件名
const SCHEDULES_FILE: &str = "backup_schedules.json";

fn schedules_path() -> PathBuf {
    paths::data_dir().join(SCHEDULES_FILE)
}

/// 读取调度配置（installed_id → 分钟间隔；0 表示关闭）
fn read_schedules() -> HashMap<String, u64> {
    let path = schedules_path();
    if !path.exists() {
        return HashMap::new();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 保存调度配置
pub fn write_schedules(schedules: &HashMap<String, u64>) -> anyhow::Result<()> {
    let path = schedules_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(schedules)?;
    std::fs::write(&path, content)?;
    Ok(())
}

/// 设置某实例的定时备份间隔（分钟；0 关闭），返回更新后的完整配置
pub fn set_schedule(installed_id: &str, minutes: u64) -> anyhow::Result<HashMap<String, u64>> {
    let mut schedules = read_schedules();
    if minutes == 0 {
        schedules.remove(installed_id);
    } else {
        schedules.insert(installed_id.to_string(), minutes);
    }
    write_schedules(&schedules)?;
    Ok(schedules)
}

/// 查询某实例的定时备份间隔（0 = 未启用）
pub fn get_schedule(installed_id: &str) -> u64 {
    read_schedules().get(installed_id).copied().unwrap_or(0)
}

/// 判断某实例是否距最新快照超期，应触发备份
fn should_backup(installed_id: &str, minutes: u64) -> anyhow::Result<bool> {
    if minutes == 0 {
        return Ok(false);
    }
    let latest = backup::list_snapshots(installed_id)?.pop();
    // 无任何快照 → 立即备份；否则按最新快照时间 + 间隔判断
    match latest {
        None => Ok(true),
        Some(meta) => {
            let created = chrono::DateTime::parse_from_rfc3339(&meta.created_at)
                .map(|d| d.naive_utc())
                .unwrap_or_else(|_| chrono::Utc::now().naive_utc());
            let elapsed_min = (chrono::Utc::now().naive_utc() - created).num_minutes();
            Ok(elapsed_min >= minutes as i64)
        }
    }
}

/// 后台调度主循环：每 `CHECK_INTERVAL_SECS` 检查一次所有已启用实例
pub async fn run_scheduler(manager: std::sync::Arc<SoftwareManager>, app: tauri::AppHandle) {
    const CHECK_INTERVAL_SECS: u64 = 60;
    let mut tick = tokio::time::interval(Duration::from_secs(CHECK_INTERVAL_SECS));
    // 首次立即执行，避免等待一个完整周期
    tick.tick().await; // 消耗初始化 tick
    loop {
        tick.tick().await;
        let started = Instant::now();
        // 遍历已装软件，对启用了定时备份且到期的执行 Hot 备份
        for sw in manager.get_installed() {
            let minutes = get_schedule(&sw.id);
            if minutes == 0 {
                continue;
            }
            let due = match should_backup(&sw.id, minutes) {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!(installed_id = %sw.id, err = %e, "检查定时备份是否到期失败");
                    false
                }
            };
            if !due {
                continue;
            }
            match backup::create_snapshot(&manager, &app, &sw.id, BackupMode::Hot, None, None) {
                Ok(meta) => {
                    tracing::info!(installed_id = %sw.id, snapshot = %meta.id, "定时备份完成");
                }
                Err(e) => {
                    tracing::warn!(installed_id = %sw.id, err = %e, "定时备份失败");
                }
            }
        }
        let _ = started;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 未启用（minutes=0）不应备份；启用但无快照应立即备份
    #[test]
    fn test_should_backup_off_and_empty() -> anyhow::Result<()> {
        // minutes=0 → false
        assert!(!should_backup("nonexistent-x", 0)?);
        // minutes>0 且无任何快照 → 应备份（list_snapshots 返回空 → pop()=None）
        // 用不存在的实例 id：manifest 缺失 → read_manifest 返回空 → pop()=None → true
        assert!(should_backup("nonexistent-x", 60)?);
        Ok(())
    }
}
