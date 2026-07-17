use crate::models::system::HistoryPoint;
use anyhow::Result;
use std::path::Path;

const MAX_HISTORY_POINTS: usize = 1440;

pub fn load_history(path: &Path) -> Result<Vec<HistoryPoint>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path)?;
    let history: Vec<HistoryPoint> = serde_json::from_str(&content)?;
    Ok(history)
}

pub fn save_history(path: &Path, history: &[HistoryPoint]) -> Result<()> {
    let mut history = history.to_vec();
    if history.len() > MAX_HISTORY_POINTS {
        history = history.split_off(history.len() - MAX_HISTORY_POINTS);
    }
    let content = serde_json::to_string_pretty(&history)?;
    std::fs::write(path, content)?;
    Ok(())
}

pub fn add_history_point(history: &mut Vec<HistoryPoint>, cpu: f64, memory: f64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    history.push(HistoryPoint {
        timestamp: now,
        cpu_usage: cpu,
        memory_usage: memory,
    });

    if history.len() > MAX_HISTORY_POINTS {
        history.remove(0);
    }
}
