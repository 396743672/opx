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
            // 关掉环境变量代理探测（ALL_PROXY 等）：reqwest 默认会读，宿主若设了
            // 不支持 CONNECT 的 HTTP 代理，公网直连被劫持后必失败
            .no_proxy()
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
                            let ver = version_from_asset(bin, major);
                            let link = bin["binary"]["package"]["link"].as_str();
                            if let Some(url) = link {
                                versions.push(CatalogVersion {
                                    version: ver,
                                    mirrors: vec![MirrorSource { name: "i18n:official".to_string(), url: url.to_string(), builtin: None }],
                                    archive: ArchiveInfo { format: archive_format_for_url(url), size: None, sha256: None },
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

/// 从 Adoptium asset JSON 提取可展示、可反查的版本号。
///
/// 上游 `version.semver` 自四段式版本号（如 25.0.5.1+2）起会错误编码为
/// `25.0.5+100000002.0.LTS`（第 4 段被塞进 build 号），既不可读也无法反查 API；
/// 顶层 `release_name`（如 `jdk-25.0.5.1+2`）才是规范值，剥前缀后使用。
pub(super) fn version_from_asset(bin: &serde_json::Value, major: u32) -> String {
    if let Some(name) = bin.get("release_name").and_then(|v| v.as_str()) {
        let stripped = name
            .strip_prefix("jdk-")
            .or_else(|| name.strip_prefix("jre-"))
            .unwrap_or(name);
        if !stripped.is_empty() {
            return stripped.to_string();
        }
    }
    bin.get("version")
        .and_then(|v| v.get("semver"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| major.to_string())
}

/// 按下载 URL 后缀判定归档格式（跨平台）。
///
/// Adoptium Linux/macOS 发 `.tar.gz`、Windows 发 `.zip`；硬编码 Zip 会让
/// Linux 包被当作 ZIP 校验，报 `Could not find central directory end`。
pub(super) fn archive_format_for_url(url: &str) -> ArchiveFormat {
    let lower = url.to_ascii_lowercase();
    if lower.contains(".tar.gz") || lower.contains(".tgz") {
        ArchiveFormat::TarGz
    } else if lower.contains(".tar.xz") || lower.contains(".txz") {
        ArchiveFormat::TarXz
    } else {
        ArchiveFormat::Zip
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_uses_release_name_not_malformed_semver() {
        // 实调 Adoptium API 的原始返回：semver 畸形、release_name 正确。
        let asset = serde_json::json!({
            "release_name": "jdk-25.0.5.1+2",
            "version": {
                "semver": "25.0.5+100000002.0.LTS",
                "openjdk_version": "25.0.5.1+2-LTS"
            }
        });
        assert_eq!(version_from_asset(&asset, 25), "25.0.5.1+2");
    }

    #[test]
    fn version_strips_jre_prefix_too() {
        // Adoptium 现统一用 jdk- 前缀，但保留 jre- 兼容以防上游变更。
        let asset = serde_json::json!({ "release_name": "jre-25.0.5.1+2" });
        assert_eq!(version_from_asset(&asset, 25), "25.0.5.1+2");
    }

    #[test]
    fn version_falls_back_to_semver_when_release_name_missing() {
        let asset = serde_json::json!({
            "version": { "semver": "21.0.2+13-LTS" }
        });
        assert_eq!(version_from_asset(&asset, 21), "21.0.2+13-LTS");
    }

    #[test]
    fn version_falls_back_to_major_when_both_missing() {
        assert_eq!(version_from_asset(&serde_json::json!({}), 17), "17");
    }

    #[test]
    fn archive_format_follows_url_suffix() {
        assert_eq!(
            archive_format_for_url("https://github.com/.../OpenJDK25U-jdk_x64_linux_hotspot_25.0.5.1_2.tar.gz"),
            ArchiveFormat::TarGz
        );
        assert_eq!(
            archive_format_for_url("https://github.com/.../OpenJDK25U-jdk_x64_windows_hotspot_25.0.5.1_2.zip"),
            ArchiveFormat::Zip
        );
    }
}
