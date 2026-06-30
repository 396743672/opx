use anyhow::Result;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, MirrorSource, SoftwareCategory,
};

use super::{InstallContext, SoftwareProvider};

pub struct JreProvider;

impl JreProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for JreProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for JreProvider {
    fn key(&self) -> &str {
        "jre"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];

        #[cfg(windows)]
        {
            versions.push(CatalogVersion {
                version: "21.0.2".to_string(),
                mirrors: vec![MirrorSource {
                    name: "华为镜像".to_string(),
                    url: "https://mirrors.huaweicloud.com/adoptium/releases/21.0.2/OpenJDK21U-jre_x64_windows_hotspot_21.0.2_13.zip".to_string(),
                }],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
            versions.push(CatalogVersion {
                version: "17.0.10".to_string(),
                mirrors: vec![MirrorSource {
                    name: "华为镜像".to_string(),
                    url: "https://mirrors.huaweicloud.com/adoptium/releases/17.0.10/OpenJDK17U-jre_x64_windows_hotspot_17.0.10_7.zip".to_string(),
                }],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
            versions.push(CatalogVersion {
                version: "11.0.22".to_string(),
                mirrors: vec![MirrorSource {
                    name: "华为镜像".to_string(),
                    url: "https://mirrors.huaweicloud.com/adoptium/releases/11.0.22/OpenJDK11U-jre_x64_windows_hotspot_11.0.22_7.zip".to_string(),
                }],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
            versions.push(CatalogVersion {
                version: "1.8".to_string(),
                mirrors: vec![MirrorSource {
                    name: "腾讯镜像".to_string(),
                    url: "https://mirrors.cloud.tencent.com/Adoptium/jdk8u422-b05/OpenJDK8U-jre_x64_windows_hotspot_8u422b05.zip".to_string(),
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
                version: "21.0.2".to_string(),
                mirrors: vec![],
                archive: ArchiveInfo {
                    format: ArchiveFormat::TarGz,
                    size: None,
                    sha256: None,
                },
            });
        }

        CatalogEntry {
            key: "jre".to_string(),
            name: "JRE".to_string(),
            description: "Java 运行时环境（Eclipse Temurin）".to_string(),
            category: SoftwareCategory::Runtime,
            icon: "mdi:play-circle".to_string(),
            versions,
            default_version: "17.0.10".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
        Ok(())
    }
}
