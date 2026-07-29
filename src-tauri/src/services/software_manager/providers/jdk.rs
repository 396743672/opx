use anyhow::Result;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, MirrorSource, SoftwareCategory,
};

use super::{InstallContext, SoftwareProvider};

pub struct JdkProvider;

impl JdkProvider { pub fn new() -> Self { Self } }
impl Default for JdkProvider { fn default() -> Self { Self::new() } }

impl SoftwareProvider for JdkProvider {
    fn key(&self) -> &str { "jdk" }

    fn catalog_entry(&self) -> CatalogEntry {
        // ponytail: 版本从 fetch_remote_versions 获取，catalog 不写死
        let versions = vec![];
        CatalogEntry {
            key: "jdk".to_string(),
            name: "JDK".to_string(),
            description: "Java 开发工具包（Eclipse Temurin，含 jcmd 等工具）".to_string(),
            description_i18n: Some("catalogDesc.jdk".to_string()),
            category: SoftwareCategory::Runtime,
            icon: "mdi:language-java".to_string(),
            versions,
            default_version: "1.8".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> { Ok(()) }

    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build().ok()?;
        let os = if cfg!(target_os = "windows") { "windows" } else if cfg!(target_os = "macos") { "mac" } else { "linux" };
        let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x64" };

        let mut versions = vec![];
        for major in [8, 17, 21, 25] {
            let api_url = format!("https://api.adoptium.net/v3/assets/latest/{}/hotspot?image_type=jdk&os={}&arch={}", major, os, arch);
            if let Ok(r) = client.get(&api_url).header("User-Agent", "OPX").send() {
                if let Ok(body) = r.text() {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                        if let Some(bin) = json.get(0) {
                            let default_ver = format!("{}", major);
                            let ver = bin["version"]["semver"].as_str().unwrap_or(&default_ver);
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
        Err(anyhow::anyhow!("JDK 不参与启停管理"))
    }
}
