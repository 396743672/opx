use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn ensure_dir_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let content = fs::read_to_string(path)?;
    let value = serde_json::from_str(&content)?;
    Ok(value)
}

pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let content = serde_json::to_string_pretty(value)?;
    fs::write(path, content)?;
    Ok(())
}