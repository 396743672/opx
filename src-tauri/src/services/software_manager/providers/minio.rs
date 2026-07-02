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
            // 版本 1：RELEASE.2021-04-22（内置 zip，离线）
            versions.push(CatalogVersion {
                version: "RELEASE.2021-04-22".to_string(),
                mirrors: {
                    let mut m = vec![];
                    let sha = builtin_manifest()
                        .get_builtin("minio", "RELEASE.2021-04-22")
                        .map(|e| e.sha256.clone())
                        .unwrap_or_default();
                    let size = builtin_manifest()
                        .get_builtin("minio", "RELEASE.2021-04-22")
                        .map(|e| e.size)
                        .unwrap_or(0);
                    m.push(MirrorSource {
                        name: "i18n:builtinVersion".to_string(),
                        url: "builtin://software/minio/RELEASE.2021-04-22.zip".to_string(),
                        builtin: Some(BuiltinInfo {
                            version: "RELEASE.2021-04-22".to_string(),
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
            // 版本 2：latest（网络 exe）
            versions.push(CatalogVersion {
                version: "latest".to_string(),
                mirrors: vec![MirrorSource {
                    name: "i18n:minioOfficial".to_string(),
                    url: "https://dl.min.io/aistor/minio/release/windows-amd64/minio.exe".to_string(),
                    builtin: None,
                }],
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
                version: "RELEASE.2021-04-22".to_string(),
                mirrors: vec![],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
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
            default_version: "RELEASE.2021-04-22".to_string(),
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
    fn minio_has_two_versions() {
        let entry = MinioProvider::new().catalog_entry();
        assert_eq!(entry.versions.len(), 2);
        let versions: Vec<_> = entry.versions.iter().map(|v| v.version.as_str()).collect();
        assert!(versions.contains(&"RELEASE.2021-04-22"));
        assert!(versions.contains(&"latest"));
    }

    #[test]
    fn minio_release_version_has_builtin_zip() {
        let entry = MinioProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "RELEASE.2021-04-22")
            .expect("应有 RELEASE.2021-04-22 版本");
        assert!(v.mirrors[0].builtin.is_some());
        assert_eq!(v.archive.format, ArchiveFormat::Zip);
    }

    #[test]
    fn minio_latest_version_is_network_executable() {
        let entry = MinioProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "latest")
            .expect("应有 latest 版本");
        assert!(v.mirrors[0].builtin.is_none());
        assert_eq!(v.archive.format, ArchiveFormat::Executable);
    }

    #[test]
    fn minio_default_version_is_release() {
        let entry = MinioProvider::new().catalog_entry();
        assert_eq!(entry.default_version, "RELEASE.2021-04-22");
    }
}
