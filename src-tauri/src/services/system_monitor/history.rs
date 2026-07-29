use crate::models::system::HistoryPoint;
use anyhow::Result;
use std::path::Path;


pub fn load_history(path: &Path) -> Result<Vec<HistoryPoint>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path)?;
    let history: Vec<HistoryPoint> = serde_json::from_str(&content)?;
    Ok(history)
}

