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
                version: "21.0.5".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "Adoptium(清华)".to_string(),
                        url: "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.2%2B13/OpenJDK21U-jre_x64_windows_hotspot_21.0.2_13.zip".to_string(),
                    },
                ],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
            versions.push(CatalogVersion {
                version: "17.0.15".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "Adoptium(清华)".to_string(),
                        url: "https://github.com/adoptium/temurin17-binaries/releases/download/jdk-17.0.10%2B7/OpenJDK17U-jre_x64_windows_hotspot_17.0.10_7.zip".to_string(),
                    },
                ],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
            versions.push(CatalogVersion {
                version: "11.0.26".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "Adoptium(清华)".to_string(),
                        url: "https://github.com/adoptium/temurin11-binaries/releases/download/jdk-11.0.22%2B7/OpenJDK11U-jre_x64_windows_hotspot_11.0.22_7.zip".to_string(),
                    },
                ],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
            versions.push(CatalogVersion {
                version: "1.8".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "Adoptium(清华)".to_string(),
                        url: "https://github.com/adoptium/temurin8-binaries/releases/download/jdk8u422-b05/OpenJDK8U-jre_x64_windows_hotspot_8u422b05.zip".to_string(),
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
            default_version: "17.0.15".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
        Ok(())
    }
}
