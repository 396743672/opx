use crate::models::system::HistoryPoint;
use anyhow::Result;
use std::path::Path;

const MAX_HISTORY_POINTS: usize = 1440; // 24 hours at 1-minute intervals

pub fn load_history(path: &Path) -> Result<Vec<HistoryPoint>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let history = crate::utils::file::read_json::<Vec<HistoryPoint>>(path)?;
    Ok(history)
}

pub fn save_history(path: &Path, history: &[HistoryPoint]) -> Result<()> {
    let mut history = history.to_vec();
    if history.len() > MAX_HISTORY_POINTS {
        history = history.split_off(history.len() - MAX_HISTORY_POINTS);
    }
    crate::utils::file::write_json(path, &history)?;
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