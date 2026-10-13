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
        // 静态列表留空（与 jdk 一致）：JRE 版本由 `fetch_remote_versions` 按运行时 OS
        // 从 Adoptium API 拉取。原实现在 `#[cfg(unix)]` 下塞一个**无 mirror** 的占位条目、
        // Windows 反为空（反向平台锁），既不可安装又不一致，故移除。
        let versions: Vec<CatalogVersion> = vec![];

        CatalogEntry {
            key: "jre".to_string(),
            name: "JRE".to_string(),
            description: "Java 运行时环境（Eclipse Temurin）".to_string(),
            description_i18n: Some("catalogDesc.jre".to_string()),
            category: SoftwareCategory::Runtime,
            icon: "mdi:language-java".to_string(),
            versions,
            default_version: "1.8".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
        Ok(())
    }

    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        let client = reqwest::blocking::Client::builder()
            // 关掉环境变量代理探测（ALL_PROXY 等）：reqwest 默认会读，宿主若设了
            // 不支持 CONNECT 的 HTTP 代理，公网直连被劫持后必失败
            .no_proxy()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .ok()?;
        let os = if cfg!(target_os = "windows") { "windows" } else if cfg!(target_os = "macos") { "mac" } else { "linux" };
        let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x64" };

        let mut versions = vec![];
        for major in [8, 17, 21, 25] {
            let api_url = format!("https://api.adoptium.net/v3/assets/latest/{}/hotspot?image_type=jre&os={}&arch={}", major, os, arch);
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
        Err(anyhow::anyhow!("JRE 不参与启停管理（作为依赖项被 Spring Boot 应用拉起）"))
    }
}
