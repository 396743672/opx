use anyhow::Result;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, BuiltinInfo, CatalogEntry, CatalogVersion, MirrorSource,
    SoftwareCategory,
};
use crate::services::software_manager::providers::builtin_manifest;

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
            // ponytail: JDK 8 内置 + 清华镜像
            versions.push(CatalogVersion {
                version: "1.8".to_string(),
                mirrors: {
                    let mut m = vec![];
                    let sha = builtin_manifest()
                        .get_builtin("jdk", "1.8")
                        .map(|e| e.sha256.clone())
                        .unwrap_or_default();
                    let size = builtin_manifest()
                        .get_builtin("jdk", "1.8")
                        .map(|e| e.size)
                        .unwrap_or(0);
                    m.push(MirrorSource {
                        name: "i18n:builtinVersion".to_string(),
                        url: "builtin://software/jdk/1.8.zip".to_string(),
                        builtin: Some(BuiltinInfo { version: "1.8".to_string(), sha256: sha, size }),
                    });
                    m.push(MirrorSource {
                        name: "i18n:adoptiumTsinghua".to_string(),
                        // ponytail: JDK 8 清华，文件名为 jdk 而非 jre
                        url: "https://mirrors.tuna.tsinghua.edu.cn/Adoptium/8/jdk/x64/windows/OpenJDK8U-jdk_x64_windows_hotspot_8u492b09.zip".to_string(),
                        builtin: None,
                    });
                    m
                },
                archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
            });
        }

        CatalogEntry {
            key: "jdk".to_string(),
            name: "JDK".to_string(),
            description: "Java 开发工具包（Eclipse Temurin，含 jcmd 等工具）".to_string(),
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
            let dir_url = format!("https://mirrors.tuna.tsinghua.edu.cn/Adoptium/{}/jdk/x64/windows/", major);
            let html = client.get(&dir_url).header("User-Agent", "OPX").send().ok()?.text().ok()?;
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
                versions.push(CatalogVersion {
                    version,
                    mirrors: vec![MirrorSource { name: "i18n:adoptiumTsinghua".to_string(), url: format!("{}{}", dir_url, fname), builtin: None }],
                    archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
                });
            }
        }
        if versions.is_empty() { None } else { Some(versions) }
    }

    fn start_command(&self, _ctx: &super::StartContext) -> Result<super::StartCommand> {
        Err(anyhow::anyhow!("JDK 不参与启停管理"))
    }
}
