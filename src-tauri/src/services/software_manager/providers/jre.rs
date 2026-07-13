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
            // 1.8 内置版本（离线 + 清华镜像）
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
                        name: "i18n:builtinVersion".to_string(),
                        url: "builtin://software/jre/1.8.zip".to_string(),
                        builtin: Some(BuiltinInfo {
                            version: "1.8".to_string(),
                            sha256: sha,
                            size: size,
                        }),
                    });
                    m.push(MirrorSource {
                        name: "i18n:adoptiumTsinghua".to_string(),
                        url: "https://mirrors.tuna.tsinghua.edu.cn/Adoptium/8/jre/x64/windows/OpenJDK8U-jre_x64_windows_hotspot_8u492b09.zip".to_string(),
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
            // 21/25 实际版本由 fetch_remote_versions 从清华镜像运行时拉取追加，
            // catalog 不放 latest 占位项，避免与拉取的实际版本重复显示。
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
            default_version: "1.8".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
        Ok(())
    }

    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        // 从清华 TUNA 镜像爬取 JRE 21 和 25 的实际版本
        // 清华目录：Adoptium/{major}/jre/x64/windows/
        // 每个 major 只镜像 1 个最新版本
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .ok()?;

        let mut versions = vec![];

        for major in [21, 25] {
            let dir_url = format!(
                "https://mirrors.tuna.tsinghua.edu.cn/Adoptium/{}/jre/x64/windows/",
                major
            );
            let response = match client
                .get(&dir_url)
                .header("User-Agent", "OPX")
                .send()
            {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[jre] 清华目录 {} 请求失败: {}", major, e);
                    continue;
                }
            };

            if !response.status().is_success() {
                eprintln!("[jre] 清华目录 {} 返回 {}", major, response.status());
                continue;
            }

            let html = match response.text() {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("[jre] 清华目录 {} 读取失败: {}", major, e);
                    continue;
                }
            };

            // 正则提取 .zip 文件名，如 OpenJDK21U-jre_x64_windows_hotspot_21.0.11_10.zip
            // 版本部分格式：21.0.11_10 / 25.0.3_9 / 8u492b09
            let re = match regex::Regex::new(
                r"OpenJDK(\d+)U-jre_x64_windows_hotspot_(\d+\.\d+[._]\d+)_?(\d+)?\.zip",
            ) {
                Ok(r) => r,
                Err(_) => continue,
            };

            if let Some(cap) = re.captures(&html) {
                // raw_version 形如 "21.0.11_10"，转换为 "21.0.11+10"
                let raw_version = cap.get(2)?.as_str().to_string();
                let version = if raw_version.contains('_') {
                    raw_version.replace('_', "+")
                } else if let Some(patch) = cap.get(3) {
                    format!("{}+{}", raw_version, patch.as_str())
                } else {
                    raw_version
                };

                // 文件名重建：包含 patch 段时拼回
                let filename = if let Some(patch) = cap.get(3) {
                    format!(
                        "OpenJDK{}U-jre_x64_windows_hotspot_{}_{}.zip",
                        major,
                        cap.get(2)?.as_str(),
                        patch.as_str()
                    )
                } else {
                    format!(
                        "OpenJDK{}U-jre_x64_windows_hotspot_{}.zip",
                        major,
                        cap.get(2)?.as_str()
                    )
                };
                let asset_url = format!("{}{}", dir_url, filename);

                versions.push(CatalogVersion {
                    version: version,
                    mirrors: vec![MirrorSource {
                        name: "i18n:adoptiumTsinghua".to_string(),
                        url: asset_url,
                        builtin: None,
                    }],
                    archive: ArchiveInfo {
                        format: ArchiveFormat::Zip,
                        size: None,
                        sha256: None,
                    },
                });
            }
        }

        if versions.is_empty() {
            None
        } else {
            Some(versions)
        }
    }

    fn start_command(&self, _ctx: &super::StartContext) -> Result<super::StartCommand> {
        Err(anyhow::anyhow!("JRE 不参与启停管理（作为依赖项被 Spring Boot 应用拉起）"))
    }
}
