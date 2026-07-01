use anyhow::Result;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, BuiltinInfo, CatalogEntry, CatalogVersion, MirrorSource,
    SoftwareCategory,
};
use crate::services::software_manager::providers::builtin_manifest;

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
                version: "17.0.15".to_string(),
                mirrors: {
                    let mut m = vec![];
                    // builtin 项（首项）
                    let sha = builtin_manifest()
                        .get_builtin("jre", "17.0.15")
                        .map(|e| e.sha256.clone())
                        .unwrap_or_default();
                    let size = builtin_manifest()
                        .get_builtin("jre", "17.0.15")
                        .map(|e| e.size)
                        .unwrap_or(0);
                    m.push(MirrorSource {
                        name: "内置默认版本（离线）".to_string(),
                        url: "builtin://software/jre/17.0.15.zip".to_string(),
                        builtin: Some(BuiltinInfo {
                            version: "17.0.15".to_string(),
                            sha256: sha,
                            size: size,
                        }),
                    });
                    // 网络镜像项
                    m.push(MirrorSource {
                        name: "Adoptium(清华)".to_string(),
                        url: "https://github.com/adoptium/temurin17-binaries/releases/download/jdk-17.0.10%2B7/OpenJDK17U-jre_x64_windows_hotspot_17.0.10_7.zip".to_string(),
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
            versions.push(CatalogVersion {
                version: "11.0.26".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "Adoptium(清华)".to_string(),
                        url: "https://github.com/adoptium/temurin11-binaries/releases/download/jdk-11.0.22%2B7/OpenJDK11U-jre_x64_windows_hotspot_11.0.22_7.zip".to_string(),
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
                version: "1.8".to_string(),
                mirrors: {
                    let mut m = vec![];
                    let sha = builtin_manifest()
                        .get_builtin("jre", "1.8")
                        .map(|e| e.sha256.clone())
                        .unwrap_or_default();
                    let size = builtin_manifest()
                        .get_builtin("jre", "1.8")
                        .map(|e| e.size)
                        .unwrap_or(0);
                    m.push(MirrorSource {
                        name: "内置默认版本（离线）".to_string(),
                        url: "builtin://software/jre/1.8.zip".to_string(),
                        builtin: Some(BuiltinInfo {
                            version: "1.8".to_string(),
                            sha256: sha,
                            size: size,
                        }),
                    });
                    m.push(MirrorSource {
                        name: "Adoptium(清华)".to_string(),
                        url: "https://github.com/adoptium/temurin8-binaries/releases/download/jdk8u422-b05/OpenJDK8U-jre_x64_windows_hotspot_8u422b05.zip".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jre_17_has_builtin_as_first_mirror() {
        let entry = JreProvider::new().catalog_entry();
        let v17 = entry
            .versions
            .iter()
            .find(|v| v.version == "17.0.15")
            .expect("应有 17.0.15 版本");
        assert!(!v17.mirrors.is_empty());
        let first = &v17.mirrors[0];
        assert!(first.builtin.is_some(), "17.0.15 的首个镜像应为 builtin");
        let builtin = first.builtin.as_ref().unwrap();
        assert_eq!(builtin.version, "17.0.15");
    }

    #[test]
    fn jre_18_has_builtin_as_first_mirror() {
        let entry = JreProvider::new().catalog_entry();
        let v18 = entry
            .versions
            .iter()
            .find(|v| v.version == "1.8")
            .expect("应有 1.8 版本");
        assert!(!v18.mirrors.is_empty());
        assert!(v18.mirrors[0].builtin.is_some());
    }

    #[test]
    fn jre_non_builtin_versions_have_no_builtin() {
        let entry = JreProvider::new().catalog_entry();
        for v in &entry.versions {
            if v.version != "17.0.15" && v.version != "1.8" {
                for m in &v.mirrors {
                    assert!(m.builtin.is_none(), "版本 {} 不应有 builtin", v.version);
                }
            }
        }
    }
}
