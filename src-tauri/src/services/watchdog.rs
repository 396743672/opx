//! 崩溃自愈看门狗：周期性检测三类实体（软件 / SpringBoot / Node）进程是否意外退出，
//! 对开启 auto_restart 的项按「延迟 + 上限」策略自动拉起；连续失败达上限则放弃。

use std::collections::HashMap;

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

    pub fn record_success(&mut self, key: &str) {
        let a = self.attempts.entry(key.to_string()).or_default();
        a.failures = 0;
        a.given_up = false;
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
    fn successful_restart_clears_failures_and_given_up() {
        let mut s = WatchdogState::new();
        s.record_failure("node:x");
        s.record_success("node:x");
        assert_eq!(s.failures("node:x"), 0);
        assert!(!s.is_given_up("node:x"));
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
