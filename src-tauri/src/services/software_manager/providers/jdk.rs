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
        let mut versions = vec![];

        #[cfg(windows)]
        {
            versions.push(CatalogVersion {
                version: "1.8".to_string(),
                mirrors: vec![MirrorSource {
                    name: "i18n:adoptiumTsinghua".to_string(),
                    url: "https://mirrors.tuna.tsinghua.edu.cn/Adoptium/8/jdk/x64/windows/OpenJDK8U-jdk_x64_windows_hotspot_8u492b09.zip".to_string(),
                    builtin: None,
                }],
                archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
            });
        }

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
        let mut versions = vec![];
        for major in [21, 25] {
            let mut found: Option<CatalogVersion> = None;

            // 尝试清华镜像
            let dir_url = format!("https://mirrors.tuna.tsinghua.edu.cn/Adoptium/{}/jdk/x64/windows/", major);
            if let Ok(r) = client.get(&dir_url).header("User-Agent", "OPX").send() {
                if let Ok(html) = r.text() {
                    let re = regex::Regex::new(
                        r"OpenJDK(\d+)U-jdk_x64_windows_hotspot_(\d+\.\d+[._]\d+)_?(\d+)?\.zip",
                    ).ok()?;
                    if let Some(cap) = re.captures(&html) {
                        let raw = cap.get(2)?.as_str().to_string();
                        let version = if raw.contains('_') { raw.replace('_', "+") }
                            else if let Some(p) = cap.get(3) { format!("{}+{}", raw, p.as_str()) } else { raw };
                        let fname = if let Some(p) = cap.get(3) {
                            format!("OpenJDK{}U-jdk_x64_windows_hotspot_{}_{}.zip", major, cap.get(2)?.as_str(), p.as_str())
                        } else {
                            format!("OpenJDK{}U-jdk_x64_windows_hotspot_{}.zip", major, cap.get(2)?.as_str())
                        };
                        found = Some(CatalogVersion {
                            version,
                            mirrors: vec![MirrorSource { name: "i18n:adoptiumTsinghua".to_string(), url: format!("{}{}", dir_url, fname), builtin: None }],
                            archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
                        });
                    }
                }
            }

            // 清华失败时尝试 Adoptium 官方 API
            if found.is_none() {
                let api_url = format!("https://api.adoptium.net/v3/assets/version/{}/latest?image_type=jdk&os=windows&arch=x64", major);
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
                                    found = Some(CatalogVersion {
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

            if let Some(cv) = found {
                versions.push(cv);
            }
        }
        if versions.is_empty() { None } else { Some(versions) }
    }

    fn start_command(&self, _ctx: &super::StartContext) -> Result<super::StartCommand> {
        Err(anyhow::anyhow!("JDK 不参与启停管理"))
    }
}
