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

/// 以 builtin 为底，用 cache 补齐。
/// - 同 key：metadata 用 builtin（代码真相，避免旧缓存覆盖过期字段），
///   versions 合并 builtin 静态版本 + cache 动态版本（merge_versions 去重）。
///   这样 JDK/JRE 动态拉取的版本不会因 builtin 覆盖而丢失（回归修复）。
/// - cache 独有 key：直接加入（远程 catalog.json 合并历史）。
/// - builtin 独有 key：保留（修复「新增内置 provider 后旧缓存不失效」的根因）。
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
        match builtin_map.get_mut(&cache_entry.key) {
            Some(builtin_entry) => {
                // 合并版本：builtin 静态优先，cache 动态追加（同版本去重）
                let cache_versions = Some(cache_entry.versions);
                let merged = merge_versions(builtin_entry.versions.clone(), cache_versions);
                builtin_entry.versions = merged;
            }
            None => {
                builtin_map.insert(cache_entry.key.clone(), cache_entry);
            }
        }
    }
    Catalog {
        entries: builtin_map.into_values().collect(),
        updated_at: builtin.updated_at,
    }
}

