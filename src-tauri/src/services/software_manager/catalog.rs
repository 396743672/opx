use std::collections::HashSet;

use crate::models::software::{Catalog, CatalogEntry, CatalogVersion};
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

/// 从 mirror_url 拉取远程 catalog，10 秒超时，失败返回 None。
/// 注：镜像 URL 的 github 改写（github_proxy_url）在下载侧 utils::download 里做，
/// 这里只负责把 mirror_url 当普通地址拉取。
pub async fn fetch_remote_catalog(mirror_url: &str) -> Option<Catalog> {
    let url = format!("{}/catalog.json", mirror_url.trim_end_matches('/'));
    // 走共享工厂：reqwest 默认读 ALL_PROXY 等环境变量，会把直连可通的镜像请求
    // 交给环境里的 HTTP 代理（不支持 CONNECT 到 443）而失败
    let response = crate::utils::http::builder()
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

    // 以 builtin 顺序为骨架，保持卡片/列表位置稳定（前端渲染依赖稳定顺序）。
    let mut entries: Vec<CatalogEntry> = builtin.entries;
    let mut builtin_keys: HashSet<String> = entries.iter().map(|e| e.key.clone()).collect();

    for remote_entry in remote.entries {
        if let Some(b) = entries.iter_mut().find(|e| e.key == remote_entry.key) {
            *b = remote_entry;
        } else if !builtin_keys.contains(&remote_entry.key) {
            builtin_keys.insert(remote_entry.key.clone());
            entries.push(remote_entry);
        }
    }

    Catalog {
        entries,
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

    // 以 builtin 顺序为骨架，保持位置稳定；cache 独有 key 追加到尾部。
    let mut entries: Vec<CatalogEntry> = builtin.entries;
    let mut builtin_keys: HashSet<String> = entries.iter().map(|e| e.key.clone()).collect();

    for cache_entry in cache.entries {
        match entries.iter_mut().find(|e| e.key == cache_entry.key) {
            Some(builtin_entry) => {
                // 合并版本：builtin 静态优先，cache 动态追加（同版本去重）
                let cache_versions = Some(cache_entry.versions);
                let merged = merge_versions(builtin_entry.versions.clone(), cache_versions);
                builtin_entry.versions = merged;
            }
            None => {
                if !builtin_keys.contains(&cache_entry.key) {
                    builtin_keys.insert(cache_entry.key.clone());
                    entries.push(cache_entry);
                }
            }
        }
    }

    Catalog {
        entries,
        updated_at: builtin.updated_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str) -> CatalogEntry {
        CatalogEntry {
            key: key.to_string(),
            name: key.to_string(),
            description: String::new(),
            description_i18n: None,
            category: crate::models::software::SoftwareCategory::Database,
            icon: String::new(),
            versions: vec![],
            default_version: String::new(),
        }
    }

    #[test]
    fn merge_with_builtin_keeps_builtin_order_and_appends_new() {
        let builtin = Catalog {
            entries: vec![entry("mysql"), entry("redis"), entry("nginx")],
            updated_at: None,
        };
        // 缓存顺序与 builtin 不同、且包含 builtin 没有的 key
        let cache = Catalog {
            entries: vec![
                {
                    let mut e = entry("nginx"); // 已存在，合并版本（空，不影响顺序）
                    e.versions = vec![CatalogVersion {
                        version: "1.31.2".into(),
                        mirrors: vec![],
                        archive: crate::models::software::ArchiveInfo {
                            format: crate::models::software::ArchiveFormat::Zip,
                            size: None,
                            sha256: None,
                        },
                    }];
                    e
                },
                entry("custom_extra"),
                entry("redis"),
            ],
            updated_at: None,
        };
        let merged = merge_with_builtin(builtin, Some(cache));
        let keys: Vec<&str> = merged.entries.iter().map(|e| e.key.as_str()).collect();
        // builtin 顺序保持，cache 独有 key 追加尾部
        assert_eq!(keys, vec!["mysql", "redis", "nginx", "custom_extra"]);
        // 合并后 nginx 带动态版本
        let nginx = merged.entries.iter().find(|e| e.key == "nginx").unwrap();
        assert_eq!(nginx.versions[0].version, "1.31.2");
    }

    #[test]
    fn merge_catalogs_keeps_builtin_order_and_appends_new() {
        let builtin = Catalog {
            entries: vec![entry("kafka"), entry("nacos")],
            updated_at: None,
        };
        let remote = Catalog {
            entries: vec![
                entry("nacos"), // 已存在 → 原位替换
                {
                    let mut e = entry("kafka");
                    e.versions = vec![CatalogVersion {
                        version: "4.3.1".into(),
                        mirrors: vec![],
                        archive: crate::models::software::ArchiveInfo {
                            format: crate::models::software::ArchiveFormat::TarGz,
                            size: None,
                            sha256: None,
                        },
                    }];
                    e
                },
                entry("influxdb_custom"),
            ],
            updated_at: None,
        };
        let merged = merge_catalogs(builtin, Some(remote));
        let keys: Vec<&str> = merged.entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, vec!["kafka", "nacos", "influxdb_custom"]);
        let kafka = merged.entries.iter().find(|e| e.key == "kafka").unwrap();
        assert_eq!(kafka.versions[0].version, "4.3.1");
    }
}

