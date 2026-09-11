//! 崩溃自愈看门狗：周期性检测三类实体（软件 / SpringBoot / Node）进程是否意外退出，
//! 对开启 auto_restart 的项按「延迟 + 上限」策略自动拉起；连续失败达上限则放弃。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tauri::Emitter;

use crate::models::software::SoftwareStatus;
use crate::models::springboot::AppStatus;
use crate::services::node_app_manager::NodeAppManager;
use crate::services::software_manager::{health_check, SoftwareManager};
use crate::services::springboot_manager::SpringBootManager;

pub const POLL_INTERVAL_SECS: u64 = 5;
pub const RESTART_DELAY_SECS: u64 = 2;
pub const MAX_FAILURES: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Restart,
    GiveUp,
}

/// 连续失败未达上限 → 继续重启；达到 → 放弃。
pub fn next_action(failures: u32, limit: u32) -> Action {
    if failures >= limit {
        Action::GiveUp
    } else {
        Action::Restart
    }
}

/// 是否应视为「意外退出」：开启自动重启 + 运行中 + 有 pid + 进程已不存在。
pub fn is_unexpected_exit(auto_restart: bool, running: bool, pid: Option<u32>, alive: bool) -> bool {
    auto_restart && running && pid.is_some() && !alive
}

#[derive(Debug, Clone, Copy, Default)]
struct Attempt {
    failures: u32,
    /// 已达上限并放弃自动重启；仅当再次观察到健康才解除。
    given_up: bool,
}

/// 各实体重启尝试状态（内存态，不落盘）
pub struct WatchdogState {
    attempts: HashMap<String, Attempt>,
}

impl WatchdogState {
    pub fn new() -> Self {
        Self {
            attempts: HashMap::new(),
        }
    }

    /// 当前失败次数（不随时间自动归零）。
    pub fn failures(&self, key: &str) -> u32 {
        self.attempts.get(key).map(|a| a.failures).unwrap_or(0)
    }

    pub fn is_given_up(&self, key: &str) -> bool {
        self.attempts.get(key).map(|a| a.given_up).unwrap_or(false)
    }

    pub fn record_failure(&mut self, key: &str) {
        self.attempts.entry(key.to_string()).or_default().failures += 1;
    }

    /// 观察到「运行中且存活」→ 健康：清零失败计数并解除「已放弃」。
    pub fn record_healthy(&mut self, key: &str) {
        if let Some(a) = self.attempts.get_mut(key) {
            a.failures = 0;
            a.given_up = false;
        }
    }

    /// 达到上限时置「已放弃」。
    pub fn mark_given_up(&mut self, key: &str) {
        self.attempts.entry(key.to_string()).or_default().given_up = true;
    }
}

/// 看门狗主循环：永不返回，随 OPX 进程结束而终止。
pub async fn run_watchdog(
    software: Arc<SoftwareManager>,
    springboot: Arc<SpringBootManager>,
    node: Arc<NodeAppManager>,
    app: tauri::AppHandle,
    node_exe: Option<PathBuf>,
) {
    let mut state = WatchdogState::new();
    loop {
        tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
        watch_software(&software, &app, &mut state).await;
        watch_springboot(&software, &springboot, &app, &mut state).await;
        watch_node(&node, &app, node_exe.as_deref(), &mut state).await;
    }
}

async fn watch_software(
    software: &Arc<SoftwareManager>,
    app: &tauri::AppHandle,
    state: &mut WatchdogState,
) {
    for sw in software.get_installed() {
        let key = format!("software:{}", sw.id);
        let running = sw.status == SoftwareStatus::Running;
        let alive = running && sw.pid.map_or(false, health_check::is_process_alive);
        if running && alive {
            state.record_healthy(&key);
            continue;
        }
        if !is_unexpected_exit(sw.auto_restart, running, sw.pid, alive) {
            continue;
        }
        if state.is_given_up(&key) {
            continue;
        }
        let failures = state.failures(&key);
        if next_action(failures, MAX_FAILURES) == Action::GiveUp {
            state.mark_given_up(&key);
            {
                let msg = format!("自动重启失败，已放弃（连续 {} 次）", failures);
                let _ = software.update_runtime_fields(
                    &sw.id,
                    SoftwareStatus::Error,
                    None,
                    None,
                    None,
                    Some(msg.clone()),
                );
                crate::services::software_manager::lifecycle::emit_status_changed(
                    app,
                    &sw.id,
                    SoftwareStatus::Error,
                    None,
                    Some(msg),
                );
                let _ = app.emit(
                    "auto-restart-giveup",
                    serde_json::json!({ "kind": "software", "id": sw.id, "name": sw.name }),
                );
                crate::oplog!("auto_restart_giveup", &sw.name, &format!("连续 {} 次失败", failures));
            }
            continue;
        }
        // 陈旧 Running 会被 validate_start_transition 拒绝，先复位为 Stopped
        let _ = software.update_runtime_fields(
            &sw.id,
            SoftwareStatus::Stopped,
            None,
            None,
            Some(chrono::Local::now().naive_local()),
            None,
        );
        tokio::time::sleep(Duration::from_secs(RESTART_DELAY_SECS)).await;
        match crate::commands::software::do_start_software(software, app, &sw.id, None).await {
            Ok(_) => {
                crate::oplog!("auto_restart", &sw.name, &format!("第 {} 次", failures + 1));
                // 注意：Ok 仅代表 spawn 成功，不代表进程存活。
                // 此处不清零计数；清零交给下一轮「观察到 Running+存活」的 record_healthy。
            }
            Err(e) => {
                tracing::warn!(id = %sw.id, error = %e, "看门狗重启软件失败");
                state.record_failure(&key);
            }
        }
    }
}

async fn watch_springboot(
    software: &Arc<SoftwareManager>,
    springboot: &Arc<SpringBootManager>,
    app: &tauri::AppHandle,
    state: &mut WatchdogState,
) {
    for sb in springboot.snapshot_apps() {
        let key = format!("springboot:{}", sb.id);
        let running = sb.status == AppStatus::Running;
        let alive = running && sb.pid.map_or(false, health_check::is_process_alive);
        if running && alive {
            state.record_healthy(&key);
            continue;
        }
        if !is_unexpected_exit(sb.auto_restart, running, sb.pid, alive) {
            continue;
        }
        if state.is_given_up(&key) {
            continue;
        }
        let failures = state.failures(&key);
        if next_action(failures, MAX_FAILURES) == Action::GiveUp {
            state.mark_given_up(&key);
            {
                let msg = format!("自动重启失败，已放弃（连续 {} 次）", failures);
                let _ = springboot.update_status(&sb.id, AppStatus::Error, None, Some(msg.clone()));
                let _ = app.emit(
                    "springboot-status-changed",
                    (sb.id.clone(), "Error", None::<u32>, Some(msg)),
                );
                let _ = app.emit(
                    "auto-restart-giveup",
                    serde_json::json!({ "kind": "springboot", "id": sb.id, "name": sb.name }),
                );
                crate::oplog!("auto_restart_giveup", &sb.name, &format!("连续 {} 次失败", failures));
            }
            continue;
        }
        // 陈旧 Running 会被 start_app 拒绝，先复位为 Stopped
        let _ = springboot.update_status(&sb.id, AppStatus::Stopped, None, None);
        tokio::time::sleep(Duration::from_secs(RESTART_DELAY_SECS)).await;
        match crate::services::springboot_manager::lifecycle::start_app(
            &sb.id, springboot, software, app,
        )
        .await
        {
            Ok(_) => {
                crate::oplog!("auto_restart", &sb.name, &format!("第 {} 次", failures + 1));
                // 注意：Ok 仅代表 spawn 成功，不代表进程存活。
                // 此处不清零计数；清零交给下一轮「观察到 Running+存活」的 record_healthy。
            }
            Err(e) => {
                tracing::warn!(id = %sb.id, error = %e, "看门狗重启 SpringBoot 失败");
                state.record_failure(&key);
            }
        }
    }
}

async fn watch_node(
    node: &Arc<NodeAppManager>,
    app: &tauri::AppHandle,
    node_exe: Option<&std::path::Path>,
    state: &mut WatchdogState,
) {
    for na in node.snapshot() {
        let key = format!("node:{}", na.id);
        let running = na.status == crate::models::node_app::NodeAppStatus::Running;
        let alive = running && na.pid.map_or(false, health_check::is_process_alive);
        if running && alive {
            state.record_healthy(&key);
            continue;
        }
        if !is_unexpected_exit(na.auto_restart, running, na.pid, alive) {
            continue;
        }
        if state.is_given_up(&key) {
            continue;
        }
        let failures = state.failures(&key);
        if next_action(failures, MAX_FAILURES) == Action::GiveUp {
            state.mark_given_up(&key);
            {
                let msg = format!("自动重启失败，已放弃（连续 {} 次）", failures);
                let _ = node.set_status(&na.id, crate::models::node_app::NodeAppStatus::Error, None, Some(msg));
                let _ = app.emit(
                    "auto-restart-giveup",
                    serde_json::json!({ "kind": "node", "id": na.id, "name": na.name }),
                );
                crate::oplog!("auto_restart_giveup", &na.name, &format!("连续 {} 次失败", failures));
            }
            continue;
        }
        let Some(exe) = node_exe else {
            state.record_failure(&key);
            continue;
        };
        tokio::time::sleep(Duration::from_secs(RESTART_DELAY_SECS)).await;
        match node.start(&na.id, exe) {
            Ok(_) => {
                crate::oplog!("auto_restart", &na.name, &format!("第 {} 次", failures + 1));
                // 注意：Ok 仅代表 spawn 成功，不代表进程存活。
                // 此处不清零计数；清零交给下一轮「观察到 Running+存活」的 record_healthy。
            }
            Err(e) => {
                tracing::warn!(id = %na.id, error = %e, "看门狗重启 Node 应用失败");
                state.record_failure(&key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_action_stops_at_limit() {
        assert_eq!(next_action(0, 3), Action::Restart);
        assert_eq!(next_action(2, 3), Action::Restart);
        assert_eq!(next_action(3, 3), Action::GiveUp);
        assert_eq!(next_action(4, 3), Action::GiveUp);
    }

    #[test]
    fn is_unexpected_exit_requires_all_conditions() {
        // 开启 + 运行中 + 有 pid + 进程已死 → true
        assert!(is_unexpected_exit(true, true, Some(1), false));
        // 进程活着 → false
        assert!(!is_unexpected_exit(true, true, Some(1), true));
        // 无 pid → false
        assert!(!is_unexpected_exit(true, true, None, false));
        // 非运行态 → false
        assert!(!is_unexpected_exit(true, false, Some(1), false));
        // 未开启自动重启 → false
        assert!(!is_unexpected_exit(false, true, Some(1), false));
    }

    #[test]
    fn failures_accumulate_without_time_reset() {
        let mut s = WatchdogState::new();
        assert_eq!(s.failures("software:a"), 0);
        s.record_failure("software:a");
        s.record_failure("software:a");
        assert_eq!(s.failures("software:a"), 2);
    }

    #[test]
    fn given_up_blocks_until_healthy_observed() {
        let mut s = WatchdogState::new();
        s.record_failure("springboot:y");
        s.mark_given_up("springboot:y");
        assert!(s.is_given_up("springboot:y"));
        // 观察到健康（用户手动拉起成功）→ 解除并清零
        s.record_healthy("springboot:y");
        assert!(!s.is_given_up("springboot:y"));
        assert_eq!(s.failures("springboot:y"), 0);
    }

    #[test]
    fn record_healthy_does_not_create_missing_entry() {
        let mut s = WatchdogState::new();
        s.record_healthy("ghost");
        assert_eq!(s.failures("ghost"), 0);
        assert!(!s.is_given_up("ghost"));
    }
}
