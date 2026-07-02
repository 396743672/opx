use anyhow::Result;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, BuiltinInfo, CatalogEntry, CatalogVersion, MirrorSource,
    SoftwareCategory,
};
use crate::services::software_manager::providers::builtin_manifest;

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
                mirrors: {
                    let mut m = vec![];
                    let sha = builtin_manifest()
                        .get_builtin("nginx", "1.31.2")
                        .map(|e| e.sha256.clone())
                        .unwrap_or_default();
                    let size = builtin_manifest()
                        .get_builtin("nginx", "1.31.2")
                        .map(|e| e.size)
                        .unwrap_or(0);
                    m.push(MirrorSource {
                        name: "i18n:builtinVersion".to_string(),
                        url: "builtin://software/nginx/1.31.2.zip".to_string(),
                        builtin: Some(BuiltinInfo {
                            version: "1.31.2".to_string(),
                            sha256: sha,
                            size: size,
                        }),
                    });
                    m.push(MirrorSource {
                        name: "i18n:huaweiMirror".to_string(),
                        url: "https://mirrors.huaweicloud.com/nginx/nginx-1.31.2.zip".to_string(),
                        builtin: None,
                    });
                    m.push(MirrorSource {
                        name: "i18n:official".to_string(),
                        url: "https://nginx.org/download/nginx-1.31.2.zip".to_string(),
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

    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        // 爬 nginx.org/en/download.html，正则提取版本号
        let url = "https://nginx.org/en/download.html";
        let response = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .ok()?
            .get(url)
            .header("User-Agent", "OPX")
            .send()
            .ok()?;

        if !response.status().is_success() {
            eprintln!("[nginx] 下载页返回 {}", response.status());
            return None;
        }

        let html = response.text().ok()?;
        let mut versions = vec![];

        // 正则匹配 nginx-X.Y.Z.zip（mainline 版本 1.31.x）
        let re = regex::Regex::new(r"nginx-(\d+\.\d+\.\d+)\.zip").ok()?;
        let mut seen = std::collections::HashSet::new();

        for cap in re.captures_iter(&html) {
            let version = cap.get(1)?.as_str().to_string();
            if seen.contains(&version) {
                continue;
            }
            seen.insert(version.clone());

            // 只取主线版本（1.31.x）
            let minor: u32 = version
                .split('.')
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let major: u32 = version
                .split('.')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            if major == 1 && minor == 31 {
                versions.push(CatalogVersion {
                    version: version.clone(),
                    mirrors: vec![
                        MirrorSource {
                            name: "i18n:huaweiMirror".to_string(),
                            url: format!(
                                "https://mirrors.huaweicloud.com/nginx/nginx-{}.zip",
                                version
                            ),
                            builtin: None,
                        },
                        MirrorSource {
                            name: "i18n:official".to_string(),
                            url: format!("https://nginx.org/download/nginx-{}.zip", version),
                            builtin: None,
                        },
                    ],
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
        Err(anyhow::anyhow!("start_command 尚未实现（待任务 3 实现）"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nginx_1312_has_builtin_as_first_mirror() {
        let entry = NginxProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "1.31.2")
            .expect("应有 1.31.2 版本");
        assert!(v.mirrors[0].builtin.is_some());
        assert_eq!(v.mirrors[0].builtin.as_ref().unwrap().version, "1.31.2");
    }

    #[test]
    fn nginx_fetch_remote_versions_method_exists() {
        let provider = NginxProvider::new();
        let _ = provider.fetch_remote_versions();
    }
}
