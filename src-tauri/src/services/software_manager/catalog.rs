use std::collections::HashMap;

use crate::models::software::Catalog;

use super::providers::all_providers;

/// 构建内置 catalog：聚合所有 provider 的 catalog_entry()
pub fn build_builtin_catalog() -> Catalog {
    let entries = all_providers()
        .into_iter()
        .map(|p| p.catalog_entry())
        .collect();
    Catalog {
        entries,
        updated_at: None,
    }
}

/// 从 mirror_url 拉取远程 catalog，10 秒超时，失败返回 None
pub async fn fetch_remote_catalog(mirror_url: &str) -> Option<Catalog> {
    let url = format!("{}/catalog.json", mirror_url.trim_end_matches('/'));
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?
        .get(&url)
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    response.json::<Catalog>().await.ok()
}

/// 合并内置 catalog 与远程 catalog：
/// - 以 key 为单位，远程条目整体覆盖内置同名 key
/// - 远程出现新 key（内置没有）→ 直接加入
/// - 远程为 None → 仅用内置
pub fn merge_catalogs(builtin: Catalog, remote: Option<Catalog>) -> Catalog {
    let remote = match remote {
        Some(r) => r,
        None => return builtin,
    };

    let mut builtin_map: HashMap<String, _> = builtin
        .entries
        .into_iter()
        .map(|e| (e.key.clone(), e))
        .collect();

    for remote_entry in remote.entries {
        builtin_map.insert(remote_entry.key.clone(), remote_entry);
    }

    Catalog {
        entries: builtin_map.into_values().collect(),
        updated_at: remote.updated_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::software::{
        ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, MirrorSource,
        SoftwareCategory,
    };

    fn make_entry(key: &str, name: &str) -> CatalogEntry {
        CatalogEntry {
            key: key.to_string(),
            name: name.to_string(),
            description: "test".to_string(),
            category: SoftwareCategory::Database,
            icon: "mdi:test".to_string(),
            versions: vec![CatalogVersion {
                version: "1.0".to_string(),
                mirrors: vec![MirrorSource {
                    name: "test".to_string(),
                    url: "https://example.com/test.zip".to_string(),
                }],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            }],
            default_version: "1.0".to_string(),
        }
    }

    #[test]
    fn build_builtin_catalog_has_four_entries() {
        let catalog = build_builtin_catalog();
        assert!(catalog.entries.len() >= 4, "应至少包含 4 个内置软件");
        let keys: Vec<_> = catalog.entries.iter().map(|e| e.key.as_str()).collect();
        assert!(keys.contains(&"mysql"));
        assert!(keys.contains(&"jre"));
        assert!(keys.contains(&"redis"));
        assert!(keys.contains(&"nginx"));
    }

    #[test]
    fn merge_with_none_returns_builtin() {
        let builtin = Catalog {
            entries: vec![make_entry("mysql", "MySQL")],
            updated_at: None,
        };
        let merged = merge_catalogs(builtin.clone(), None);
        assert_eq!(merged.entries.len(), 1);
        assert_eq!(merged.entries[0].key, "mysql");
        assert_eq!(merged.updated_at, None);
    }

    #[test]
    fn merge_remote_overrides_builtin_by_key() {
        let builtin = Catalog {
            entries: vec![
                make_entry("mysql", "MySQL-builtin"),
                make_entry("redis", "Redis-builtin"),
            ],
            updated_at: None,
        };
        let remote = Catalog {
            entries: vec![make_entry("mysql", "MySQL-remote")],
            updated_at: Some("2026-06-30".to_string()),
        };
        let merged = merge_catalogs(builtin, Some(remote));
        assert_eq!(merged.entries.len(), 2);
        let mysql = merged
            .entries
            .iter()
            .find(|e| e.key == "mysql")
            .unwrap();
        assert_eq!(mysql.name, "MySQL-remote");
        let redis = merged
            .entries
            .iter()
            .find(|e| e.key == "redis")
            .unwrap();
        assert_eq!(redis.name, "Redis-builtin");
        assert_eq!(merged.updated_at.as_deref(), Some("2026-06-30"));
    }

    #[test]
    fn merge_remote_adds_new_key() {
        let builtin = Catalog {
            entries: vec![make_entry("mysql", "MySQL")],
            updated_at: None,
        };
        let remote = Catalog {
            entries: vec![make_entry("custom-tool", "CustomTool")],
            updated_at: None,
        };
        let merged = merge_catalogs(builtin, Some(remote));
        assert_eq!(merged.entries.len(), 2);
        let keys: Vec<_> = merged.entries.iter().map(|e| e.key.as_str()).collect();
        assert!(keys.contains(&"mysql"));
        assert!(keys.contains(&"custom-tool"));
    }
}
