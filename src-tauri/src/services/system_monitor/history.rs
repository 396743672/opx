use crate::models::system::{HistoryPoint, MetricsHistory};
use anyhow::Result;
use std::path::Path;

pub fn load_metrics(path: &Path) -> Result<MetricsHistory> {
    if !path.exists() {
        return Ok(MetricsHistory::default());
    }
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content).unwrap_or_default())
}

pub fn save_history(path: &Path, h: &MetricsHistory) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string(h)?)?;
    Ok(())
}

/// 删除早于 `now_ms - retain_days` 的点（时间戳单位：毫秒，与前端 Date.now() 一致）。
pub fn prune_older_than(points: &mut Vec<HistoryPoint>, now_ms: i64, retain_days: i64) {
    let cutoff = now_ms - retain_days * 86_400_000;
    points.retain(|p| (p.timestamp as i64) >= cutoff);
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
}

