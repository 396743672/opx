//! 崩溃自愈看门狗：周期性检测三类实体（软件 / SpringBoot / Node）进程是否意外退出，
//! 对开启 auto_restart 的项按「延迟 + 上限」策略自动拉起；连续失败达上限则放弃。

use std::collections::HashMap;
use std::time::Instant;

pub const POLL_INTERVAL_SECS: u64 = 5;
pub const RESTART_DELAY_SECS: u64 = 2;
pub const MAX_FAILURES: u32 = 3;
pub const RESET_AFTER_SECS: u64 = 60;

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

/// 距上次成功重启已稳定运行 reset_after_secs 秒，则失败计数归零；否则保持。
pub fn effective_failures(failures: u32, elapsed_secs: u64, reset_after_secs: u64) -> u32 {
    if elapsed_secs >= reset_after_secs {
        0
    } else {
        failures
    }
}

/// 是否应视为「意外退出」：开启自动重启 + 运行中 + 有 pid + 进程已不存在。
pub fn is_unexpected_exit(auto_restart: bool, running: bool, pid: Option<u32>, alive: bool) -> bool {
    auto_restart && running && pid.is_some() && !alive
}

#[derive(Debug, Clone, Copy, Default)]
struct Attempt {
    failures: u32,
    last_restart_at: Option<Instant>,
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

    /// 当前有效失败次数（稳定窗口外加权归零）。
    pub fn failures_now(&mut self, key: &str, now: Instant) -> u32 {
        let a = self.attempts.entry(key.to_string()).or_default();
        let eff = match a.last_restart_at {
            Some(t) => effective_failures(a.failures, now.duration_since(t).as_secs(), RESET_AFTER_SECS),
            None => a.failures,
        };
        a.failures = eff;
        eff
    }

    pub fn record_success(&mut self, key: &str, now: Instant) {
        let a = self.attempts.entry(key.to_string()).or_default();
        a.failures = 0;
        a.last_restart_at = Some(now);
    }

    pub fn record_failure(&mut self, key: &str) {
        self.attempts.entry(key.to_string()).or_default().failures += 1;
    }

    /// 观察到「运行中且存活」→ 视为健康，清零失败计数（用户手动恢复后不再立即放弃）。
    pub fn record_healthy(&mut self, key: &str) {
        if let Some(a) = self.attempts.get_mut(key) {
            a.failures = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn next_action_stops_at_limit() {
        assert_eq!(next_action(0, 3), Action::Restart);
        assert_eq!(next_action(2, 3), Action::Restart);
        assert_eq!(next_action(3, 3), Action::GiveUp);
        assert_eq!(next_action(4, 3), Action::GiveUp);
    }

    #[test]
    fn effective_failures_resets_after_stable_window() {
        assert_eq!(effective_failures(3, 59, 60), 3);
        assert_eq!(effective_failures(3, 60, 60), 0);
        assert_eq!(effective_failures(3, 120, 60), 0);
        assert_eq!(effective_failures(0, 5, 60), 0);
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
    fn state_counts_failures_and_resets_on_success_or_health() {
        let mut s = WatchdogState::new();
        let now = Instant::now();
        assert_eq!(s.failures_now("software:a", now), 0);
        s.record_failure("software:a");
        s.record_failure("software:a");
        assert_eq!(s.failures_now("software:a", now), 2);
        s.record_success("software:a", now);
        assert_eq!(s.failures_now("software:a", now), 0);
        s.record_failure("software:a");
        s.record_healthy("software:a");
        assert_eq!(s.failures_now("software:a", now), 0);
    }

    #[test]
    fn state_resets_failures_after_stable_window() {
        let mut s = WatchdogState::new();
        let past = Instant::now() - Duration::from_secs(120);
        s.record_failure("node:x");
        s.record_failure("node:x");
        s.record_success("node:x", past);
        s.record_failure("node:x"); // 成功后又失败 1 次
        // 距上次成功已 120s ≥ 60s → 计数归零
        assert_eq!(s.failures_now("node:x", Instant::now()), 0);
    }
}
