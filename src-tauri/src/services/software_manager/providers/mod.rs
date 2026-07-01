use anyhow::Result;
use std::path::Path;

use crate::models::software::CatalogEntry;

pub mod mysql;
pub mod jre;
pub mod redis;
pub mod nginx;

pub trait SoftwareProvider: Send + Sync {
    fn key(&self) -> &str;
    fn catalog_entry(&self) -> CatalogEntry;
    fn post_install(&self, ctx: &InstallContext) -> Result<()>;
}

pub struct InstallContext {
    pub key: String,
    pub version: String,
    pub install_path: String,
}

impl InstallContext {
    pub fn new(key: String, version: String, install_path: String) -> Self {
        Self {
            key,
            version,
            install_path,
        }
    }

    pub fn install_dir(&self) -> &Path {
        Path::new(&self.install_path)
    }
}

use std::collections::HashMap;
use std::sync::OnceLock;

/// 内置 zip 清单条目
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ManifestEntry {
    pub sha256: String,
    pub size: u64,
}

/// 内置 zip 清单：{key: {version: ManifestEntry}}
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct BuiltinManifest {
    #[serde(flatten)]
    entries: HashMap<String, HashMap<String, ManifestEntry>>,
}

impl BuiltinManifest {
    /// 从指定路径加载 manifest.json，文件不存在或解析失败返回空 manifest
    pub fn load_from_path(path: &std::path::Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Self::default(),
        };
        serde_json::from_str(&content).unwrap_or_default()
    }

    /// 查询 {key}/{version} 对应的 manifest 条目
    pub fn get_builtin(&self, key: &str, version: &str) -> Option<&ManifestEntry> {
        self.entries
            .get(key)
            .and_then(|versions| versions.get(version))
    }
}

/// 测试辅助：从指定路径加载 manifest
#[cfg(test)]
fn load_builtin_manifest_from_path(path: &std::path::Path) -> BuiltinManifest {
    BuiltinManifest::load_from_path(path)
}

/// 全局 manifest 单例（启动时从 resource_dir/software/manifest.json 加载）
static MANIFEST: OnceLock<BuiltinManifest> = OnceLock::new();

/// 获取全局 manifest 单例。首次调用时返回默认空 manifest（真正的初始化在 init_builtin_manifest）。
pub fn builtin_manifest() -> &'static BuiltinManifest {
    MANIFEST.get_or_init(|| BuiltinManifest::default())
}

/// 用指定 manifest 路径初始化全局单例（应用启动时调用）
pub fn init_builtin_manifest(path: &std::path::Path) {
    let manifest = BuiltinManifest::load_from_path(path);
    let _ = MANIFEST.set(manifest);
}

pub fn all_providers() -> Vec<Box<dyn SoftwareProvider>> {
    vec![
        Box::new(mysql::MySqlProvider::new()),
        Box::new(jre::JreProvider::new()),
        Box::new(redis::RedisProvider::new()),
        Box::new(nginx::NginxProvider::new()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_builtin_manifest_parses_valid_json() {
        let tmp = std::env::temp_dir().join(format!(
            "opx_manifest_test_{}.json",
            std::process::id()
        ));
        let content = r#"{
            "jre": {
                "17.0.15": { "sha256": "abc123", "size": 43478873 }
            }
        }"#;
        std::fs::write(&tmp, content).unwrap();

        let manifest = load_builtin_manifest_from_path(&tmp);
        std::fs::remove_file(&tmp).ok();

        let info = manifest.get_builtin("jre", "17.0.15").unwrap();
        assert_eq!(info.sha256, "abc123");
        assert_eq!(info.size, 43478873);
    }

    #[test]
    fn load_builtin_manifest_missing_file_returns_empty() {
        let manifest = load_builtin_manifest_from_path(
            std::path::Path::new("/nonexistent/manifest.json"),
        );
        assert!(manifest.get_builtin("jre", "17.0.15").is_none());
    }

    #[test]
    fn load_builtin_manifest_missing_entry_returns_none() {
        let tmp = std::env::temp_dir().join(format!(
            "opx_manifest_test2_{}.json",
            std::process::id()
        ));
        std::fs::write(&tmp, r#"{"jre": {}}"#).unwrap();

        let manifest = load_builtin_manifest_from_path(&tmp);
        std::fs::remove_file(&tmp).ok();

        assert!(manifest.get_builtin("jre", "17.0.15").is_none());
        assert!(manifest.get_builtin("mysql", "8.4.0").is_none());
    }
}
