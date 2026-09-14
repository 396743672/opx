//! 指标采样器：每 30s 采样整机与运行中实例，落盘保留 7 天，并做阈值告警。

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::models::system::HistoryPoint;
use crate::services::software_manager::{process_monitor, SoftwareManager};
use crate::services::springboot_manager::SpringBootManager;
use crate::services::system_monitor::{alerts, history, info};
use crate::utils::paths;

pub const SAMPLE_INTERVAL_SECS: u64 = 30;
pub const RETAIN_DAYS: i64 = 7;

fn metrics_path() -> PathBuf {
    paths::data_dir().join("metrics_history.json")
}

pub async fn run_recorder(
    app: AppHandle,
    software: Arc<SoftwareManager>,
    springboot: Arc<SpringBootManager>,
) {
    let mut alerting: HashSet<String> = HashSet::new();
    let mut tick = tokio::time::interval(Duration::from_secs(SAMPLE_INTERVAL_SECS));
    tick.tick().await; // 消耗初始化 tick
    loop {
        tick.tick().await;
        sample_once(&app, &software, &springboot, &mut alerting).await;
    }
}

async fn sample_once(
    app: &AppHandle,
    software: &Arc<SoftwareManager>,
    springboot: &Arc<SpringBootManager>,
    alerting: &mut HashSet<String>,
) {
    let thresholds = crate::commands::config::read_settings().unwrap_or_default();
    let sys = info::sample_system();
    let now_ms = chrono::Local::now().timestamp_millis();

    // 运行中实例：(显示名, pid)
    let mut targets: Vec<(String, u32)> = Vec::new();
    for sw in software.get_installed() {
        if sw.status == crate::models::software::SoftwareStatus::Running {
            if let Some(pid) = sw.pid {
                targets.push((sw.name.clone(), pid));
            }
        }
    }
    for sb in springboot.snapshot_apps() {
        if sb.status == crate::models::springboot::AppStatus::Running {
            if let Some(pid) = sb.pid {
                targets.push((sb.name.clone(), pid));
            }
        }
    }

    let pids: Vec<u32> = targets.iter().map(|(_, p)| *p).collect();
    let samples = process_monitor::sample_processes(&pids);

    let mut h = history::load_metrics(&metrics_path()).unwrap_or_default();
    h.system.push(HistoryPoint {
        timestamp: now_ms as u64,
        cpu_usage: sys.cpu_usage,
        memory_usage: sys.memory_usage,
    });
    for s in &samples {
        let mem_pct = if sys.memory_total > 0 {
            s.mem_bytes as f64 / sys.memory_total as f64 * 100.0
        } else {
            0.0
        };
        h.processes
            .entry(s.pid.to_string())
            .or_default()
            .push(HistoryPoint {
                timestamp: now_ms as u64,
                cpu_usage: s.cpu_usage,
                memory_usage: mem_pct,
            });
        if let Some(pts) = h.processes.get_mut(&s.pid.to_string()) {
            history::prune_older_than(pts, now_ms, RETAIN_DAYS);
        }
    }
    history::prune_older_than(&mut h.system, now_ms, RETAIN_DAYS);
    // 丢弃已无样本的进程键，避免文件无限膨胀
    h.processes.retain(|_, pts| !pts.is_empty());
    if let Err(e) = history::save_history(&metrics_path(), &h) {
        tracing::warn!(error = %e, "写入指标历史失败");
    }

    // ---- 告警判定 ----
    let name_of = |pid: u32| {
        targets
            .iter()
            .find(|(_, p)| *p == pid)
            .map(|(n, _)| n.clone())
            .unwrap_or_else(|| format!("PID {pid}"))
    };
    eval(app, alerting, "system:cpu", "整机", "cpu", sys.cpu_usage, thresholds.alert_system_cpu);
    eval(app, alerting, "system:mem", "整机", "mem", sys.memory_usage, thresholds.alert_system_mem);
    for s in &samples {
        let mem_pct = if sys.memory_total > 0 {
            s.mem_bytes as f64 / sys.memory_total as f64 * 100.0
        } else {
            0.0
        };
        let name = name_of(s.pid);
        eval(app, alerting, &format!("proc:{}:cpu", s.pid), &name, "cpu", s.cpu_usage, thresholds.alert_process_cpu);
        eval(app, alerting, &format!("proc:{}:mem", s.pid), &name, "mem", mem_pct, thresholds.alert_process_mem);
    }
}

/// 触发/恢复单条告警：触发写审计 + emit 事件；恢复只写审计（不打扰）。
fn eval(
    app: &AppHandle,
    alerting: &mut HashSet<String>,
    key: &str,
    name: &str,
    metric: &str,
    value: f64,
    threshold: u32,
) {
    if alerts::should_alert(value, threshold, alerting.contains(key)) {
        alerting.insert(key.to_string());
        crate::oplog!("alert_high", &format!("{} {} {}%（阈值 {}%）", name, metric, value.round(), threshold));
        let _ = app.emit(
            "resource-alert",
            serde_json::json!({ "kind": if key.starts_with("proc:") { "process" } else { "system" }, "name": name, "metric": metric, "value": value.round(), "threshold": threshold }),
        );
    } else if alerting.contains(key) && alerts::is_recovered(value, threshold) {
        alerting.remove(key);
        crate::oplog!("alert_recovered", &format!("{} {} {}%", name, metric, value.round()));
    }
}
