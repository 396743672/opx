use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn ensure_dir_exists(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct SampleConfig {
        name: String,
        count: u32,
    }

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "opx_file_utils_{name}_{}_{}",
            std::process::id(),
            nanos
        ))
    }

    #[test]
    fn ensure_dir_exists_creates_missing_nested_directory() {
        let dir = temp_path("nested").join("a").join("b");

        ensure_dir_exists(&dir).expect("directory should be created");

        assert!(dir.is_dir());
        let root = dir
            .ancestors()
            .nth(2)
            .expect("nested temp root should exist")
            .to_path_buf();
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn write_json_then_read_json_roundtrips_pretty_json() {
        let dir = temp_path("json");
        fs::create_dir_all(&dir).expect("temp dir should be created");
        let file = dir.join("config.json");
        let value = SampleConfig {
            name: "opx".to_string(),
            count: 3,
        };

        write_json(&file, &value).expect("json should be written");
        let read_back: SampleConfig = read_json(&file).expect("json should be read");

        assert_eq!(read_back, value);
        let raw = fs::read_to_string(&file).expect("json file should exist");
        assert!(raw.contains('\n'), "json should be pretty formatted");
        fs::remove_dir_all(dir).ok();
    }
}
