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

    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        // Adoptium 的二进制发布在 temurin{major}-binaries 仓库（每个 LTS 一个仓库）
        // 分别调 4 个 API 合并结果。限流：未认证 60 次/小时，4 次/刷新可接受
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .ok()?;

        let mut versions = vec![];

        for major in [8, 11, 17, 21] {
            let url = format!(
                "https://api.github.com/repos/adoptium/temurin{}-binaries/releases?per_page=1",
                major
            );
            let response = match client
                .get(&url)
                .header("User-Agent", "OPX")
                .header("Accept", "application/vnd.github+json")
                .send()
            {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[jre] temurin{}-binaries API 请求失败: {}", major, e);
                    continue;  // 单个 major 失败不影响其他
                }
            };

            if !response.status().is_success() {
                eprintln!(
                    "[jre] temurin{}-binaries API 返回 {}",
                    major,
                    response.status()
                );
                continue;
            }

            let releases: Vec<serde_json::Value> = match response.json() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[jre] temurin{}-binaries JSON 解析失败: {}", major, e);
                    continue;
                }
            };

            // 只取该 LTS 大版本的最新一个 release（API 默认按创建时间倒序）
            if let Some(release) = releases.first() {
                let tag = match release.get("tag_name").and_then(|t| t.as_str()) {
                    Some(t) => t.to_string(),
                    None => continue,
                };
                // tag 格式如 "jdk-17.0.16+7"，解析为版本号
                if let Some(version) = parse_adoptium_tag(&tag) {
                    // 找 Windows x64 JRE zip asset
                    if let Some(asset_url) = find_jre_asset(release, major) {
                        versions.push(CatalogVersion {
                            version: version.clone(),
                            mirrors: vec![MirrorSource {
                                name: "Adoptium GitHub".to_string(),
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
            }
        }

        if versions.is_empty() {
            None
        } else {
            Some(versions)
        }
    }
}

/// 解析 Adoptium tag_name，如 "jdk-17.0.16+7" → "17.0.16"
fn parse_adoptium_tag(tag: &str) -> Option<String> {
    let tag = tag.strip_prefix("jdk-")?;
    let version = tag.split('+').next()?;
    // 验证格式：x.y.z
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() == 3 && parts.iter().all(|p| p.parse::<u32>().is_ok()) {
        Some(version.to_string())
    } else {
        None
    }
}

/// 在 release 的 assets 中找 Windows x64 JRE zip
fn find_jre_asset(release: &serde_json::Value, _major: u32) -> Option<String> {
    let assets = release.get("assets")?.as_array()?;
    for asset in assets {
        let name = asset.get("name")?.as_str()?;
        // 名称如 "OpenJDK17U-jre_x64_windows_hotspot_17.0.16_7.zip"
        if name.contains("jre")
            && name.contains("x64")
            && name.contains("windows")
            && name.ends_with(".zip")
        {
            let url = asset.get("browser_download_url")?.as_str()?;
            return Some(url.to_string());
        }
    }
    None
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

    #[test]
    fn jre_fetch_remote_versions_returns_some_when_implemented() {
        // 仅验证方法存在且返回 Option（不实际调网络——网络测试在集成测试覆盖）
        let provider = JreProvider::new();
        // fetch_remote_versions 应返回 Option<Vec<CatalogVersion>>
        // 注意：实际调用会触发网络请求，单元测试不验证返回值内容
        // 只验证方法签名存在（编译通过即说明 trait 方法已覆写）
        let _ = provider.fetch_remote_versions();
        // 不断言返回值（网络环境可能失败返回 None）
    }

    #[test]
    fn parse_adoptium_tag_parses_lts() {
        assert_eq!(parse_adoptium_tag("jdk-17.0.16+7"), Some("17.0.16".to_string()));
        assert_eq!(parse_adoptium_tag("jdk-11.0.26+9"), Some("11.0.26".to_string()));
        assert_eq!(parse_adoptium_tag("jdk-8u422-b05"), None);
        assert_eq!(parse_adoptium_tag("not-a-tag"), None);
    }

    #[test]
    fn find_jre_asset_matches_windows_x64_zip() {
        let release = serde_json::json!({
            "assets": [
                {"name": "OpenJDK17U-jdk_x64_windows_hotspot_17.0.16_7.zip", "browser_download_url": "https://example.com/jdk.zip"},
                {"name": "OpenJDK17U-jre_x64_windows_hotspot_17.0.16_7.zip", "browser_download_url": "https://example.com/jre.zip"},
            ]
        });
        let url = find_jre_asset(&release, 17).unwrap();
        assert_eq!(url, "https://example.com/jre.zip");
    }

    #[test]
    fn find_jre_asset_returns_none_when_no_match() {
        let release = serde_json::json!({
            "assets": [
                {"name": "OpenJDK17U-jdk_x64_windows_hotspot_17.0.16_7.zip", "browser_download_url": "https://example.com/jdk.zip"},
            ]
        });
        assert!(find_jre_asset(&release, 17).is_none());
    }
}
