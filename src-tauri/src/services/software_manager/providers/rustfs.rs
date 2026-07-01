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
                        name: "内置默认版本（离线）".to_string(),
                        url: "builtin://software/rustfs/1.0.0-beta.8.zip".to_string(),
                        builtin: Some(BuiltinInfo {
                            version: "1.0.0-beta.8".to_string(),
                            sha256: sha,
                            size: size,
                        }),
                    });
                    m.push(MirrorSource {
                        name: "RustFS 官方".to_string(),
                        url: "https://dl.rustfs.com/artifacts/rustfs/release/rustfs-windows-x86_64-latest.zip".to_string(),
                        builtin: None,
                    });
                    m
                },
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rustfs_has_builtin_as_first_mirror() {
        let entry = RustfsProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "1.0.0-beta.8")
            .expect("应有 1.0.0-beta.8 版本");
        assert!(v.mirrors[0].builtin.is_some());
        assert_eq!(
            v.mirrors[0].builtin.as_ref().unwrap().version,
            "1.0.0-beta.8"
        );
    }

    #[test]
    fn rustfs_uses_zip_format() {
        let entry = RustfsProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "1.0.0-beta.8")
            .expect("应有 1.0.0-beta.8 版本");
        assert_eq!(v.archive.format, ArchiveFormat::Zip);
    }
}
