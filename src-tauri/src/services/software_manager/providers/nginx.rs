use anyhow::Result;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, MirrorSource, SoftwareCategory,
};

use super::{InstallContext, SoftwareProvider};

pub struct NginxProvider;

impl NginxProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NginxProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for NginxProvider {
    fn key(&self) -> &str {
        "nginx"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];

        #[cfg(windows)]
        {
            versions.push(CatalogVersion {
                version: "1.31.2".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "华为镜像".to_string(),
                        url: "https://mirrors.huaweicloud.com/nginx/nginx-1.31.2.zip".to_string(),
                    },
                    MirrorSource {
                        name: "官方".to_string(),
                        url: "https://nginx.org/download/nginx-1.31.2.zip".to_string(),
                    },
                ],
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
                version: "1.31.2".to_string(),
                mirrors: vec![],
                archive: ArchiveInfo {
                    format: ArchiveFormat::TarGz,
                    size: None,
                    sha256: None,
                },
            });
        }

        CatalogEntry {
            key: "nginx".to_string(),
            name: "Nginx".to_string(),
            description: "高性能 HTTP 服务器与反向代理".to_string(),
            category: SoftwareCategory::WebServer,
            icon: "mdi:web".to_string(),
            versions,
            default_version: "1.31.2".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
        Ok(())
    }
}
