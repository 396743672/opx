use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ArchiveFormat {
    Zip,
    TarGz,
    Executable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveInfo {
    pub format: ArchiveFormat,
    pub size: Option<u64>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinInfo {
    pub version: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorSource {
    pub name: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builtin: Option<BuiltinInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogVersion {
    pub version: String,
    pub mirrors: Vec<MirrorSource>,
    pub archive: ArchiveInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SoftwareCategory {
    Database,
    Runtime,
    Cache,
    WebServer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub key: String,
    pub name: String,
    pub description: String,
    pub category: SoftwareCategory,
    pub icon: String,
    pub versions: Vec<CatalogVersion>,
    pub default_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub entries: Vec<CatalogEntry>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SoftwareStatus {
    Running,
    Stopped,
    Error,
    Unknown,
}

impl Default for SoftwareStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InstallSource {
    Mirror {
        mirror_name: String,
        url: String,
    },
    Builtin {
        version: String,
    },
    Custom {
        archive_name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSoftware {
    pub id: String,
    pub key: String,
    pub version: String,
    pub name: String,
    pub install_path: String,
    pub install_time: NaiveDateTime,
    pub status: SoftwareStatus,
    pub port: u16,
    pub config: serde_json::Value,
    pub is_custom: bool,
    pub auto_start_on_app_start: bool,
    pub startup_order: u32,
    pub source: InstallSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSoftwareList {
    pub software: Vec<InstalledSoftware>,
}

impl Default for InstalledSoftwareList {
    fn default() -> Self {
        Self {
            software: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallParams {
    pub key: String,
    pub version: String,
    pub mirror_index: usize,
    pub set_as_default_jre: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomInstallParams {
    pub name: String,
    pub archive_path: String,
}

// 保留原设计中的类型（兼容性）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareMeta {
    pub key: String,
    pub name: String,
    pub description: String,
    pub available_versions: Vec<String>,
    pub default_version: String,
}

#[cfg(test)]
mod tests {
    use super::{BuiltinInfo, InstallSource, MirrorSource};

    #[test]
    fn mirror_source_with_builtin_serializes_correctly() {
        let mirror = MirrorSource {
            name: "内置默认版本（离线）".to_string(),
            url: "builtin://software/jre/17.0.15.zip".to_string(),
            builtin: Some(BuiltinInfo {
                version: "17.0.15".to_string(),
                sha256: "abc123".to_string(),
                size: 43478873,
            }),
        };
        let json = serde_json::to_string(&mirror).unwrap();
        assert!(json.contains("\"builtin\""));
        assert!(json.contains("\"sha256\":\"abc123\""));

        let deserialized: MirrorSource = serde_json::from_str(&json).unwrap();
        assert!(deserialized.builtin.is_some());
        assert_eq!(deserialized.builtin.unwrap().sha256, "abc123");
    }

    #[test]
    fn mirror_source_without_builtin_omits_field() {
        let mirror = MirrorSource {
            name: "Adoptium(清华)".to_string(),
            url: "https://github.com/...".to_string(),
            builtin: None,
        };
        let json = serde_json::to_string(&mirror).unwrap();
        // None 字段应被 skip_serializing_if 跳过，不出现在 JSON 中
        assert!(!json.contains("builtin"));
    }

    #[test]
    fn install_source_builtin_serializes_correctly() {
        let source = InstallSource::Builtin {
            version: "17.0.15".to_string(),
        };
        let json = serde_json::to_string(&source).unwrap();
        assert!(json.contains("\"Builtin\""));
        assert!(json.contains("\"version\":\"17.0.15\""));

        let deserialized: InstallSource = serde_json::from_str(&json).unwrap();
        match deserialized {
            InstallSource::Builtin { version } => assert_eq!(version, "17.0.15"),
            _ => panic!("应反序列化为 Builtin 变体"),
        }
    }
}
