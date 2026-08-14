use std::collections::HashMap;

use crate::models::software::{Catalog, CatalogVersion};
use crate::utils::paths;

use super::providers::all_providers;

/// 缓存文件的路径
fn cache_path() -> std::path::PathBuf {
    paths::config_dir().join("catalog-cache.json")
}

/// ponytail: 加载缓存的目录
pub fn load_catalog_cache() -> Option<Catalog> {
    let p = cache_path();
    if !p.exists() { return None; }
    std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str(&s).ok())
}

/// ponytail: 保存目录到缓存
pub fn save_catalog_cache(catalog: &Catalog) {
    if let Ok(json) = serde_json::to_string_pretty(catalog) {
        let _ = std::fs::write(cache_path(), json);
    }
}

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

/// 合并内置版本与动态版本：
/// - 同 version 号去重，内置优先（保留内置的 mirrors，含 builtin 项）
/// - 动态版本追加在内置版本之后
/// - 内置为空时直接用动态版本
/// - 动态为 None 时返回内置
pub fn merge_versions(
    builtin_versions: Vec<CatalogVersion>,
    remote_versions: Option<Vec<CatalogVersion>>,
) -> Vec<CatalogVersion> {
    let remote = match remote_versions {
        Some(r) => r,
        None => return builtin_versions,
    };

    let mut existing: std::collections::HashSet<String> = builtin_versions
        .iter()
        .map(|v| v.version.clone())
        .collect();

    let mut merged = builtin_versions;
    for v in remote {
        if !existing.contains(&v.version) {
            existing.insert(v.version.clone());
            merged.push(v);
        }
    }
    merged
}

/// 以 builtin 为底，用 cache 补齐 builtin 缺失的 key。
/// builtin（代码定义的真相）优先：同 key 用 builtin 的 entry，避免旧缓存
/// 把新增/更新后的软件覆盖回过期版本。cache 中只补充 builtin 没有的额外 key
/// （典型来自远程 catalog.json 合并历史）。旧缓存漏了 builtin 新增 key 时，
/// 新 key 由 builtin 补回 —— 修复「新增内置 provider 后旧缓存不失效」的根因。
pub fn merge_with_builtin(builtin: Catalog, cache: Option<Catalog>) -> Catalog {
    let cache = match cache {
        Some(c) => c,
        None => return builtin,
    };
    let mut builtin_map: HashMap<String, _> = builtin
        .entries
        .into_iter()
        .map(|e| (e.key.clone(), e))
        .collect();
    for cache_entry in cache.entries {
        builtin_map
            .entry(cache_entry.key.clone())
            .or_insert(cache_entry);
    }
    Catalog {
        entries: builtin_map.into_values().collect(),
        updated_at: builtin.updated_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::software::{CatalogEntry, SoftwareCategory};

    fn entry(key: &str) -> CatalogEntry {
        CatalogEntry {
            key: key.to_string(),
            name: key.to_string(),
            description: String::new(),
            description_i18n: None,
            category: SoftwareCategory::Database,
            icon: "mdi:database".to_string(),
            versions: vec![],
            default_version: String::new(),
        }
    }

    fn catalog(keys: &[&str]) -> Catalog {
        Catalog {
            entries: keys.iter().map(|k| entry(k)).collect(),
            updated_at: None,
        }
    }

    #[test]
    fn merge_with_builtin_returns_builtin_when_cache_none() {
        let builtin = catalog(&["mysql", "redis"]);
        let merged = merge_with_builtin(builtin.clone(), None);
        let keys: Vec<String> = merged.entries.iter().map(|e| e.key.clone()).collect();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"mysql".to_string()));
        assert!(keys.contains(&"redis".to_string()));
    }

    #[test]
    fn merge_with_builtin_adds_builtin_new_keys_missing_from_cache() {
        // builtin 新增了 postgresql/mongodb/nacos，旧缓存只有 7 个旧软件
        let builtin = catalog(&["mysql", "jre", "jdk", "redis", "nginx", "minio", "rustfs", "postgresql", "mongodb", "nacos"]);
        let cache = catalog(&["mysql", "jre", "jdk", "redis", "nginx", "minio", "rustfs"]);
        let merged = merge_with_builtin(builtin, Some(cache));
        let keys: Vec<String> = merged.entries.iter().map(|e| e.key.clone()).collect();
        assert!(keys.contains(&"postgresql".to_string()), "新增 key 必须由 builtin 补回");
        assert!(keys.contains(&"mongodb".to_string()));
        assert!(keys.contains(&"nacos".to_string()));
        assert_eq!(keys.len(), 10);
    }

    #[test]
    fn merge_with_builtin_builtin_wins_on_same_key() {
        // builtin 与 cache 同名 key 时用 builtin 的 entry（避免旧缓存覆盖回过期版本）
        let mut builtin_entry = entry("mysql");
        builtin_entry.icon = "mdi:new-icon".to_string();
        let builtin = Catalog { entries: vec![builtin_entry], updated_at: None };
        let mut cache_entry = entry("mysql");
        cache_entry.icon = "mdi:old-icon".to_string();
        let cache = Catalog { entries: vec![cache_entry], updated_at: None };
        let merged = merge_with_builtin(builtin, Some(cache));
        assert_eq!(merged.entries[0].icon, "mdi:new-icon", "builtin 应优先于缓存");
    }

    #[test]
    fn merge_with_builtin_adds_cache_only_keys() {
        // cache 中有 builtin 没有的 key（来自远程合并历史）应保留
        let builtin = catalog(&["mysql"]);
        let cache = catalog(&["mysql", "custom-remote"]);
        let merged = merge_with_builtin(builtin, Some(cache));
        let keys: Vec<String> = merged.entries.iter().map(|e| e.key.clone()).collect();
        assert!(keys.contains(&"custom-remote".to_string()));
        assert_eq!(keys.len(), 2);
    }
}
