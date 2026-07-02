use anyhow::Result;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, BuiltinInfo, CatalogEntry, CatalogVersion, MirrorSource,
    SoftwareCategory,
};
use crate::services::software_manager::providers::builtin_manifest;

use super::{InstallContext, SoftwareProvider};

pub struct RustfsProvider;

impl RustfsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RustfsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for RustfsProvider {
    fn key(&self) -> &str {
        "rustfs"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];

        #[cfg(windows)]
        {
            // 版本 1：1.0.0-beta.8（内置 zip，离线）
            versions.push(CatalogVersion {
                version: "1.0.0-beta.8".to_string(),
                mirrors: {
                    let mut m = vec![];
                    let sha = builtin_manifest()
                        .get_builtin("rustfs", "1.0.0-beta.8")
                        .map(|e| e.sha256.clone())
                        .unwrap_or_default();
                    let size = builtin_manifest()
                        .get_builtin("rustfs", "1.0.0-beta.8")
                        .map(|e| e.size)
                        .unwrap_or(0);
                    m.push(MirrorSource {
                        name: "i18n:builtinVersion".to_string(),
                        url: "builtin://software/rustfs/1.0.0-beta.8.zip".to_string(),
                        builtin: Some(BuiltinInfo {
                            version: "1.0.0-beta.8".to_string(),
                            sha256: sha,
                            size: size,
                        }),
                    });
                    m
                },
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
            // 版本 2：latest（网络 zip）
            versions.push(CatalogVersion {
                version: "latest".to_string(),
                mirrors: vec![MirrorSource {
                    name: "i18n:rustfsOfficial".to_string(),
                    url: "https://dl.rustfs.com/artifacts/rustfs/release/rustfs-windows-x86_64-latest.zip".to_string(),
                    builtin: None,
                }],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
        }

        #[cfg(unix)]
        {
            versions.push(CatalogVersion {
                version: "1.0.0-beta.8".to_string(),
                mirrors: vec![],
                archive: ArchiveInfo {
                    format: ArchiveFormat::TarGz,
                    size: None,
                    sha256: None,
                },
            });
        }

        CatalogEntry {
            key: "rustfs".to_string(),
            name: "RustFS".to_string(),
            description: "Rust 实现的 S3 兼容对象存储".to_string(),
            category: SoftwareCategory::Database,
            icon: "mdi:cloud".to_string(),
            versions,
            default_version: "1.0.0-beta.8".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
        Ok(())
    }

    fn start_command(&self, _ctx: &super::StartContext) -> Result<super::StartCommand> {
        Err(anyhow::anyhow!("start_command 尚未实现（任务 3 完成）"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rustfs_has_two_versions() {
        let entry = RustfsProvider::new().catalog_entry();
        assert_eq!(entry.versions.len(), 2);
        let versions: Vec<_> = entry.versions.iter().map(|v| v.version.as_str()).collect();
        assert!(versions.contains(&"1.0.0-beta.8"));
        assert!(versions.contains(&"latest"));
    }

    #[test]
    fn rustfs_beta_version_has_builtin_zip() {
        let entry = RustfsProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "1.0.0-beta.8")
            .expect("应有 1.0.0-beta.8 版本");
        assert!(v.mirrors[0].builtin.is_some());
        assert_eq!(v.archive.format, ArchiveFormat::Zip);
    }

    #[test]
    fn rustfs_latest_version_is_network_zip() {
        let entry = RustfsProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "latest")
            .expect("应有 latest 版本");
        assert!(v.mirrors[0].builtin.is_none());
        assert_eq!(v.archive.format, ArchiveFormat::Zip);
    }

    #[test]
    fn rustfs_default_version_is_beta() {
        let entry = RustfsProvider::new().catalog_entry();
        assert_eq!(entry.default_version, "1.0.0-beta.8");
    }
}
