//! 资源告警判定（纯函数，便于单测）。

/// 是否需要触发告警：达到阈值且此前不在告警态。
pub fn should_alert(value: f64, threshold: u32, already_alerting: bool) -> bool {
    value >= threshold as f64 && !already_alerting
}

/// 是否已恢复：回落到「阈值 - 2 个百分点」以下（迟滞，避免临界抖动反复告警）。
pub fn is_recovered(value: f64, threshold: u32) -> bool {
    value < threshold as f64 - 2.0
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
}
