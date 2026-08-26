use anyhow::Result;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, MirrorSource,
    SoftwareCategory,
};

use super::{InstallContext, SoftwareProvider};

/// Node.js Runtime provider：仅安装不启动（类 JRE 定位），供未来 Node 应用管理消费。
pub struct NodeProvider;

impl NodeProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NodeProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// 解析 nodejs.org/dist/index.json：仅保留 LTS 且纯 X.Y.Z 版本（去 v 前缀），升序排列。
/// JSON 形如 [{"version":"v22.14.0","lts":"Jod"}, ...]；lts 可为字符串或布尔。
pub fn parse_node_index(json: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(json) else {
        return out;
    };
    for item in arr {
        let is_lts = match item.get("lts") {
            Some(v) if v.is_boolean() => v.as_bool().unwrap_or(false),
            Some(v) if v.is_string() => true,
            _ => false,
        };
        if !is_lts {
            continue;
        }
        let Some(v) = item.get("version").and_then(|v| v.as_str()) else {
            continue;
        };
        let clean = v.strip_prefix('v').unwrap_or(v);
        if !clean.chars().all(|c| c.is_ascii_digit() || c == '.') {
            continue; // 排除 rc/beta 等非纯版本
        }
        if clean.split('.').count() == 3 && !out.contains(&clean.to_string()) {
            out.push(clean.to_string());
        }
    }
    // 数字分段升序
    out.sort_by(|a, b| {
        let pa: Vec<u64> = a.split('.').filter_map(|x| x.parse().ok()).collect();
        let pb: Vec<u64> = b.split('.').filter_map(|x| x.parse().ok()).collect();
        pa.cmp(&pb)
    });
    out
}

impl SoftwareProvider for NodeProvider {
    fn key(&self) -> &str {
        "node"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        #[allow(unused_mut)]
        let mut versions = vec![];

        // Windows 目标（本项目），加载官方 win-x64 zip
        #[cfg(windows)]
        for ver in ["20.11.1", "22.14.0"] {
            versions.push(CatalogVersion {
                version: ver.to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "i18n:nodejsOrg".to_string(),
                        url: format!("https://nodejs.org/dist/v{ver}/node-v{ver}-win-x64.zip"),
                        builtin: None,
                    },
                    MirrorSource {
                        name: "i18n:npmmirror".to_string(),
                        url: format!(
                            "https://npmmirror.com/mirrors/node/v{ver}/node-v{ver}-win-x64.zip"
                        ),
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

        CatalogEntry {
            key: "node".to_string(),
            name: "Node.js".to_string(),
            description: "JavaScript 运行时（Node.js LTS）".to_string(),
            description_i18n: Some("catalogDesc.node".to_string()),
            category: SoftwareCategory::Runtime,
            icon: "mdi:language-nodejs".to_string(),
            versions,
            default_version: "22.14.0".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
        Ok(())
    }

    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        let url = "https://nodejs.org/dist/index.json";
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .ok()?;
        let resp = client.get(url).header("User-Agent", "OPX").send().ok()?;
        if !resp.status().is_success() {
            eprintln!("[node] index.json 返回 {}", resp.status());
            return None;
        }
        let text = resp.text().ok()?;
        let versions = parse_node_index(&text);
        if versions.is_empty() {
            return None;
        }
        // 动态版本取最新 20 个维持列表可读性
        Some(
            versions
                .into_iter()
                .rev()
                .take(20)
                .map(|v| CatalogVersion {
                    version: v.clone(),
                    mirrors: vec![
                        MirrorSource {
                            name: "i18n:nodejsOrg".to_string(),
                            url: format!("https://nodejs.org/dist/v{v}/node-v{v}-win-x64.zip"),
                            builtin: None,
                        },
                        MirrorSource {
                            name: "i18n:npmmirror".to_string(),
                            url: format!(
                                "https://npmmirror.com/mirrors/node/v{v}/node-v{v}-win-x64.zip"
                            ),
                            builtin: None,
                        },
                    ],
                    archive: ArchiveInfo {
                        format: ArchiveFormat::Zip,
                        size: None,
                        sha256: None,
                    },
                })
                .collect(),
        )
    }

    fn start_command(&self, _ctx: &super::StartContext) -> Result<super::StartCommand> {
        Err(anyhow::anyhow!(
            "Node.js 不参与启停管理（作为 Runtime 依赖供未来模块使用）"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_node_index_keeps_lts_pure() {
        let json = r#"[
            {"version":"v22.14.0","lts":"Jod"},
            {"version":"v20.11.1","lts":"Iron"},
            {"version":"v23.0.0","lts":false},
            {"version":"v22.14.0-rc.1","lts":false}
        ]"#;
        let vs = parse_node_index(json);
        assert_eq!(vs, vec!["20.11.1".to_string(), "22.14.0".to_string()]);
    }

    #[test]
    fn parse_node_index_preserves_order_unique() {
        let json = r#"[
            {"version":"v22.14.0","lts":true},
            {"version":"v20.11.1","lts":"Iron"},
            {"version":"v20.11.1","lts":"Iron"}
        ]"#;
        let vs = parse_node_index(json);
        assert_eq!(vs, vec!["20.11.1".to_string(), "22.14.0".to_string()]);
    }
}