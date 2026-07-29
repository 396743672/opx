use anyhow::Result;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, MirrorSource,
    SoftwareCategory,
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
            // 1.8 内置版本（离线 + 清华镜像）
            versions.push(CatalogVersion {
                version: "1.8".to_string(),
                mirrors: {
                    let mut m = vec![];
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
            description_i18n: Some("catalogDesc.jre".to_string()),
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
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .ok()?;
        let os = if cfg!(target_os = "windows") { "windows" } else if cfg!(target_os = "macos") { "mac" } else { "linux" };
        let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x64" };

        let mut versions = vec![];
        for major in [8, 17, 21, 25] {
            let api_url = format!("https://api.adoptium.net/v3/assets/version/{}/latest?image_type=jre&os={}&arch={}", major, os, arch);
            if let Ok(r) = client.get(&api_url).header("User-Agent", "OPX").send() {
                if let Ok(body) = r.text() {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                        if let Some(bin) = json.get(0) {
                            let default_ver = format!("{}", major);
                            let ver = bin["version_data"]["semver"].as_str().or(
                                bin["version_data"]["openjdk_version"].as_str()
                            ).unwrap_or(&default_ver);
                            let link = bin["binary"]["package"]["link"].as_str();
                            if let Some(url) = link {
                                versions.push(CatalogVersion {
                                    version: ver.to_string(),
                                    mirrors: vec![MirrorSource { name: "i18n:official".to_string(), url: url.to_string(), builtin: None }],
                                    archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
                                });
                            }
                        }
                    }
                }
            }
        }
        if versions.is_empty() { None } else { Some(versions) }
    }

    fn start_command(&self, _ctx: &super::StartContext) -> Result<super::StartCommand> {
        Err(anyhow::anyhow!("JRE 不参与启停管理（作为依赖项被 Spring Boot 应用拉起）"))
    }
}
