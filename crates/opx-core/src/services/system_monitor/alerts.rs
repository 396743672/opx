//! 资源告警判定（纯函数，便于单测）。

/// 是否需要触发告警：达到阈值且此前不在告警态。
pub fn should_alert(value: f64, threshold: u32, already_alerting: bool) -> bool {
    value >= threshold as f64 && !already_alerting
}

/// 是否已恢复：回落到「阈值 - 2 个百分点」以下（迟滞，避免临界抖动反复告警）。
pub fn is_recovered(value: f64, threshold: u32) -> bool {
    value < threshold as f64 - 2.0
}

/// 进程告警键。存活键集合与 `eval` 必须用同一构键函数，否则「释放已退出进程」的判定会失效。
pub fn proc_key(pid: u32, metric: &str) -> String {
    format!("proc:{pid}:{metric}")
}

/// 某进程的全部告警键（cpu/mem），用于每轮构造存活键集合。
pub fn proc_keys(pid: u32) -> [String; 2] {
    [proc_key(pid, "cpu"), proc_key(pid, "mem")]
}

/// 释放已消失进程的告警态：保留非 `proc:` 键与存活键。
/// 进程退出后其键若不清理，PID 被复用（Windows 常见）时新进程会永久失警。
pub fn release_stale(
    alerting: &mut std::collections::HashSet<String>,
    live_keys: &std::collections::HashSet<String>,
) {
    alerting.retain(|k| !k.starts_with("proc:") || live_keys.contains(k));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_alert_only_when_crossing_and_not_already_alerting() {
        assert!(!should_alert(89.9, 90, false));
        assert!(should_alert(90.0, 90, false));
        assert!(should_alert(99.0, 90, false));
        assert!(!should_alert(99.0, 90, true), "已在告警态不重复触发");
    }

    #[test]
    fn is_recovered_requires_hysteresis() {
        assert!(!is_recovered(95.0, 90));
        assert!(!is_recovered(88.5, 90), "阈值下 1.5 个百分点仍在迟滞区，未恢复");
        assert!(is_recovered(87.9, 90), "低于 阈值的 2 个百分点才判恢复");
        assert!(is_recovered(10.0, 90));
    }

    #[test]
    fn proc_keys_match_eval_key_shape() {
        // 存活键集合与 eval 的键必须完全同构，否则 release_stale 会误清仍在告警的进程
        let keys = proc_keys(7);
        assert_eq!(keys, [proc_key(7, "cpu"), proc_key(7, "mem")]);
        let live: std::collections::HashSet<String> = keys.into_iter().collect();
        let mut alerting: std::collections::HashSet<String> =
            [proc_key(7, "cpu"), proc_key(7, "mem")].into_iter().collect();
        release_stale(&mut alerting, &live);
        assert_eq!(alerting.len(), 2, "存活进程的告警键不应被释放");
    }

    #[test]
    fn stale_pid_key_does_not_suppress_new_alert() {
        let mut alerting: std::collections::HashSet<String> =
            ["proc:123:cpu".to_string(), "system:cpu".to_string()].into_iter().collect();
        // 本轮存活进程里没有 123（已退出）
        let live: std::collections::HashSet<String> =
            ["proc:456:cpu".to_string()].into_iter().collect();
        release_stale(&mut alerting, &live);
        assert!(!alerting.contains("proc:123:cpu"), "退出进程的告警键应被释放");
        assert!(alerting.contains("system:cpu"), "非 proc 键不受影响");
        // 释放后同 PID 的新进程可重新告警
        assert!(should_alert(95.0, 90, alerting.contains("proc:123:cpu")));
    }
}
