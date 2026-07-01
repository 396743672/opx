use anyhow::Result;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, BuiltinInfo, CatalogEntry, CatalogVersion, MirrorSource,
    SoftwareCategory,
};
use crate::services::software_manager::providers::builtin_manifest;

use super::{InstallContext, SoftwareProvider};

pub struct MinioProvider;

impl MinioProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MinioProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for MinioProvider {
    fn key(&self) -> &str {
        "minio"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];

        #[cfg(windows)]
        {
            versions.push(CatalogVersion {
                version: "latest".to_string(),
                mirrors: {
                    let mut m = vec![];
                    let sha = builtin_manifest()
                        .get_builtin("minio", "latest")
                        .map(|e| e.sha256.clone())
                        .unwrap_or_default();
                    let size = builtin_manifest()
                        .get_builtin("minio", "latest")
                        .map(|e| e.size)
                        .unwrap_or(0);
                    m.push(MirrorSource {
                        name: "内置默认版本（离线）".to_string(),
                        url: "builtin://software/minio/latest.exe".to_string(),
                        builtin: Some(BuiltinInfo {
                            version: "latest".to_string(),
                            sha256: sha,
                            size: size,
                        }),
                    });
                    m.push(MirrorSource {
                        name: "MinIO 官方".to_string(),
                        url: "https://dl.min.io/aistor/minio/release/windows-amd64/minio.exe".to_string(),
                        builtin: None,
                    });
                    m
                },
                archive: ArchiveInfo {
                    format: ArchiveFormat::Executable,
                    size: None,
                    sha256: None,
                },
            });
        }

        #[cfg(unix)]
        {
            versions.push(CatalogVersion {
                version: "latest".to_string(),
                mirrors: vec![],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Executable,
                    size: None,
                    sha256: None,
                },
            });
        }

        CatalogEntry {
            key: "minio".to_string(),
            name: "MinIO".to_string(),
            description: "S3 兼容对象存储".to_string(),
            category: SoftwareCategory::Database,
            icon: "mdi:cloud".to_string(),
            versions,
            default_version: "latest".to_string(),
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
    fn minio_latest_has_builtin_as_first_mirror() {
        let entry = MinioProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "latest")
            .expect("应有 latest 版本");
        assert!(v.mirrors[0].builtin.is_some());
        assert_eq!(v.mirrors[0].builtin.as_ref().unwrap().version, "latest");
    }

    #[test]
    fn minio_uses_executable_format() {
        let entry = MinioProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "latest")
            .expect("应有 latest 版本");
        assert_eq!(v.archive.format, ArchiveFormat::Executable);
    }
}
