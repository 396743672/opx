use crate::models::system::{HistoryPoint, MetricsHistory};
use anyhow::Result;
use std::path::Path;

/// 指标历史文件路径（`data/metrics_history.json`）。读写方共用，避免路径字面量多处重复。
pub fn metrics_path() -> std::path::PathBuf {
    crate::utils::paths::data_dir().join("metrics_history.json")
}

pub fn load_metrics(path: &Path) -> Result<MetricsHistory> {
    if !path.exists() {
        return Ok(MetricsHistory::default());
    }
    let content = std::fs::read_to_string(path)?;
    // 解析失败（截断/旧格式）回退空历史，但要留痕，便于排查「曲线莫名清空」
    Ok(serde_json::from_str(&content).unwrap_or_else(|e| {
        tracing::warn!(error = %e, path = %path.display(), "解析指标历史失败，已回退为空");
        MetricsHistory::default()
    }))
}

/// 写临时文件后 rename 原子替换，避免中途崩溃留下截断文件导致历史归零。
pub fn save_history(path: &Path, h: &MetricsHistory) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string(h)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// 删除早于 `now_ms - retain_days` 的点（时间戳单位：毫秒，与前端 Date.now() 一致）。
pub fn prune_older_than(points: &mut Vec<HistoryPoint>, now_ms: i64, retain_days: i64) {
    let cutoff = now_ms - retain_days * 86_400_000;
    points.retain(|p| (p.timestamp as i64) >= cutoff);
}

/// 对整机与**所有**进程序列裁剪，并丢弃裁剪后为空的键。
/// 必须遍历全部键（而非本轮采到的 pid），否则已退出进程的序列永不淘汰。
pub fn prune_all(h: &mut MetricsHistory, now_ms: i64, retain_days: i64) {
    prune_older_than(&mut h.system, now_ms, retain_days);
    for pts in h.processes.values_mut() {
        prune_older_than(pts, now_ms, retain_days);
    }
    h.processes.retain(|_, pts| !pts.is_empty());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::system::HistoryPoint;

    fn pt(ms: u64) -> HistoryPoint {
        HistoryPoint { timestamp: ms, cpu_usage: 1.0, memory_usage: 2.0 }
    }

    #[test]
    fn prune_removes_older_than_retain_days() {
        let now = 10_000_000_000i64; // ms
        let day = 86_400_000i64;
        let mut pts = vec![pt((now - 8 * day) as u64), pt((now - 6 * day) as u64), pt(now as u64)];
        prune_older_than(&mut pts, now, 7);
        assert_eq!(pts.len(), 2, "8 天前的点应被删除，6 天前与当前保留");
        assert_eq!(pts[0].timestamp, (now - 6 * day) as u64);
    }

    #[test]
    fn prune_keeps_exact_boundary_and_handles_empty() {
        let now = 10_000_000_000i64;
        let day = 86_400_000i64;
        // 恰好等于 cutoff 的点保留（>= 判定）
        let mut pts = vec![pt((now - 7 * day) as u64)];
        prune_older_than(&mut pts, now, 7);
        assert_eq!(pts.len(), 1);
        let mut empty: Vec<HistoryPoint> = Vec::new();
        prune_older_than(&mut empty, now, 7);
        assert!(empty.is_empty());
    }

    #[test]
    fn missing_file_yields_empty_not_error() {
        let dir = std::env::temp_dir().join(format!(
            "__qa_metrics_miss_{}",
            chrono::Local::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let h = load_metrics(&dir.join("nope.json")).expect("文件缺失不应报错");
        assert!(h.system.is_empty() && h.processes.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("__qa_metrics_{}", chrono::Local::now().timestamp_nanos_opt().unwrap_or(0)));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("metrics_history.json");
        let mut h = MetricsHistory::default();
        h.system.push(pt(123));
        h.processes.insert("42".to_string(), vec![pt(456)]);
        save_history(&path, &h).unwrap();
        let back = load_metrics(&path).unwrap();
        assert_eq!(back.system.len(), 1);
        assert_eq!(back.processes.get("42").unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dead_process_series_dropped() {
        let now = 10_000_000_000i64;
        let day = 86_400_000i64;
        let mut h = MetricsHistory::default();
        h.system.push(pt(now as u64));
        // 已退出进程：键仍在，但样本全部超期
        h.processes.insert("123".to_string(), vec![pt((now - 8 * day) as u64)]);
        prune_all(&mut h, now, 7);
        assert!(!h.processes.contains_key("123"), "全部超期的进程键应被丢弃");
        assert_eq!(h.system.len(), 1, "system 未超期点应保留");
    }
}

