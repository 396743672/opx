use anyhow::Result;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, BuiltinInfo, CatalogEntry, CatalogVersion, MirrorSource,
    SoftwareCategory,
};
use crate::services::software_manager::providers::builtin_manifest;

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
                version: "8.8.0".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "redis-windows GitHub".to_string(),
                        url: "https://github.com/redis-windows/redis-windows/releases/download/8.8.0/Redis-8.8.0-Windows-x64-cygwin.zip".to_string(),
                        builtin: None,
                    },
                ],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
            versions.push(CatalogVersion {
                version: "8.2.7".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "redis-windows GitHub".to_string(),
                        url: "https://github.com/redis-windows/redis-windows/releases/download/8.2.7/Redis-8.2.7-Windows-x64-cygwin.zip".to_string(),
                        builtin: None,
                    },
                ],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
            versions.push(CatalogVersion {
                version: "7.4.9".to_string(),
                mirrors: {
                    let mut m = vec![];
                    let sha = builtin_manifest()
                        .get_builtin("redis", "7.4.9")
                        .map(|e| e.sha256.clone())
                        .unwrap_or_default();
                    let size = builtin_manifest()
                        .get_builtin("redis", "7.4.9")
                        .map(|e| e.size)
                        .unwrap_or(0);
                    m.push(MirrorSource {
                        name: "内置默认版本（离线）".to_string(),
                        url: "builtin://software/redis/7.4.9.zip".to_string(),
                        builtin: Some(BuiltinInfo {
                            version: "7.4.9".to_string(),
                            sha256: sha,
                            size: size,
                        }),
                    });
                    m.push(MirrorSource {
                        name: "redis-windows GitHub".to_string(),
                        url: "https://github.com/redis-windows/redis-windows/releases/download/7.4.9/Redis-7.4.9-Windows-x64-cygwin.zip".to_string(),
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
                version: "8.8.0".to_string(),
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
            default_version: "8.8.0".to_string(),
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
    fn redis_749_has_builtin_as_first_mirror() {
        let entry = RedisProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "7.4.9")
            .expect("应有 7.4.9 版本");
        assert!(v.mirrors[0].builtin.is_some());
        assert_eq!(v.mirrors[0].builtin.as_ref().unwrap().version, "7.4.9");
    }

    #[test]
    fn redis_880_and_827_have_no_builtin() {
        let entry = RedisProvider::new().catalog_entry();
        for v in &entry.versions {
            if v.version != "7.4.9" {
                for m in &v.mirrors {
                    assert!(m.builtin.is_none(), "版本 {} 不应有 builtin", v.version);
                }
            }
        }
    }
}
