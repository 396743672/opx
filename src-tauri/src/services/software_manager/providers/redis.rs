use anyhow::Result;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, MirrorSource, SoftwareCategory,
};

use super::{InstallContext, SoftwareProvider};

pub struct RedisProvider;

impl RedisProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RedisProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for RedisProvider {
    fn key(&self) -> &str {
        "redis"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];

        #[cfg(windows)]
        {
            versions.push(CatalogVersion {
                version: "7.4.0".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "华为镜像".to_string(),
                        url: "https://mirrors.huaweicloud.com/redis-for-windows/7.4.0/redis-7.4.0-windows-x64.zip".to_string(),
                    },
                    MirrorSource {
                        name: "GitHub Releases".to_string(),
                        url: "https://github.com/redis/redis-windows/releases/download/7.4.0/redis-7.4.0-windows-x64.zip".to_string(),
                    },
                ],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
            versions.push(CatalogVersion {
                version: "7.2.4".to_string(),
                mirrors: vec![MirrorSource {
                    name: "华为镜像".to_string(),
                    url: "https://mirrors.huaweicloud.com/redis-for-windows/7.2.4/redis-7.2.4-windows-x64.zip".to_string(),
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
                version: "7.4.0".to_string(),
                mirrors: vec![],
                archive: ArchiveInfo {
                    format: ArchiveFormat::TarGz,
                    size: None,
                    sha256: None,
                },
            });
        }

        CatalogEntry {
            key: "redis".to_string(),
            name: "Redis".to_string(),
            description: "内存键值存储".to_string(),
            category: SoftwareCategory::Cache,
            icon: "mdi:database".to_string(),
            versions,
            default_version: "7.4.0".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
        Ok(())
    }
}
