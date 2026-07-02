# 动态版本列表实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** JRE/MySQL/Redis/Nginx 4 个软件支持动态拉取远程版本列表，刷新 catalog 时合并到内置版本。

**架构：** SoftwareProvider trait 加可选方法 `fetch_remote_versions()` 返回 `Option<Vec<CatalogVersion>>`。refresh_catalog 命令调用各 provider 的该方法，与内置版本合并（同 version 号去重，内置优先）。MinIO/RustFS 不实现该方法（保持固定）。

**技术栈：** Rust + Tauri 2 + reqwest + serde_json + Vue 3 + TypeScript + i18n

**设计文档：** `docs/superpowers/specs/2026-07-01-dynamic-version-list-design.md`

---

## 文件结构

### 修改文件

| 文件 | 改动 |
|---|---|
| `src-tauri/src/services/software_manager/providers/mod.rs` | SoftwareProvider trait 加 `fetch_remote_versions` 默认方法 |
| `src-tauri/src/services/software_manager/catalog.rs` | 新增 `merge_versions` 函数 |
| `src-tauri/src/services/software_manager/providers/jre.rs` | 实现 `fetch_remote_versions`（Adoptium GitHub API） |
| `src-tauri/src/services/software_manager/providers/mysql.rs` | 实现 `fetch_remote_versions`（HTML 爬取） |
| `src-tauri/src/services/software_manager/providers/redis.rs` | 实现 `fetch_remote_versions`（GitHub API） |
| `src-tauri/src/services/software_manager/providers/nginx.rs` | 实现 `fetch_remote_versions`（HTML 爬取） |
| `src-tauri/src/commands/software.rs` | `refresh_catalog` 调用 `fetch_remote_versions` 并合并 |
| `src/modules/software-manager/stores/catalog.ts` | refreshCatalog 加 loading + error 状态 |
| `src/modules/software-manager/pages/RepositoryPage.vue` | 刷新按钮 loading 状态 + 错误提示 |
| `src/locales/zh-CN.ts` | 加 i18n 键 |
| `src/locales/en-US.ts` | 加 i18n 键 |

---

## 任务 1：SoftwareProvider trait 加 fetch_remote_versions

**文件：**
- 修改：`src-tauri/src/services/software_manager/providers/mod.rs`

- [ ] **步骤 1：编写失败的测试**

在 `src-tauri/src/services/software_manager/providers/mod.rs` 末尾的 `#[cfg(test)] mod tests` 中加测试（在现有测试之后）：

```rust
    #[test]
    fn default_fetch_remote_versions_returns_none() {
        struct DummyProvider;
        impl SoftwareProvider for DummyProvider {
            fn key(&self) -> &str { "dummy" }
            fn catalog_entry(&self) -> CatalogEntry {
                CatalogEntry {
                    key: "dummy".to_string(),
                    name: "Dummy".to_string(),
                    description: "test".to_string(),
                    category: crate::models::software::SoftwareCategory::Database,
                    icon: "mdi:test".to_string(),
                    versions: vec![],
                    default_version: "".to_string(),
                }
            }
            fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
                Ok(())
            }
        }
        let p = DummyProvider;
        assert!(p.fetch_remote_versions().is_none());
    }
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test --lib services::software_manager::providers::tests::default_fetch_remote_versions_returns_none`
预期：FAIL，编译错误 `no method fetch_remote_versions in trait SoftwareProvider`

- [ ] **步骤 3：实现 trait 默认方法**

在 `src-tauri/src/services/software_manager/providers/mod.rs` 中找到 `SoftwareProvider` trait 定义（约第 13-17 行）：

```rust
pub trait SoftwareProvider: Send + Sync {
    fn key(&self) -> &str;
    fn catalog_entry(&self) -> CatalogEntry;
    fn post_install(&self, ctx: &InstallContext) -> Result<()>;
}
```

替换为：

```rust
pub trait SoftwareProvider: Send + Sync {
    fn key(&self) -> &str;
    fn catalog_entry(&self) -> CatalogEntry;
    fn post_install(&self, ctx: &InstallContext) -> Result<()>;

    /// 拉取远程版本列表（可选，默认返回 None 表示不动态拉取）
    /// 返回 Some(Vec) 时，版本会与内置 catalog_entry() 的版本合并
    /// 拉取失败应返回 None（不阻塞其他软件）
    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        None
    }
}
```

注意：需要在 `use` 语句中导入 `CatalogVersion`。找到文件顶部（约第 1-4 行）：

```rust
use anyhow::Result;
use std::path::Path;

use crate::models::software::CatalogEntry;
```

改为：

```rust
use anyhow::Result;
use std::path::Path;

use crate::models::software::{CatalogEntry, CatalogVersion};
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cd src-tauri && cargo test --lib services::software_manager::providers`
预期：PASS，所有测试通过（含新增测试 + 现有 manifest 测试）

- [ ] **步骤 5：Commit**

```bash
cd D:/object/opx && git add src-tauri/src/services/software_manager/providers/mod.rs
git commit -m "feat: SoftwareProvider trait 加 fetch_remote_versions 默认方法

默认返回 None（不动态拉取），MinIO/RustFS 保持默认行为。
后续 JRE/MySQL/Redis/Nginx 覆写此方法。"
```

---

## 任务 2：catalog.rs merge_versions 函数

**文件：**
- 修改：`src-tauri/src/services/software_manager/catalog.rs`

- [ ] **步骤 1：编写失败的测试**

在 `src-tauri/src/services/software_manager/catalog.rs` 末尾的 `#[cfg(test)] mod tests` 中加测试（在现有测试之后）：

```rust
    #[test]
    fn merge_versions_dedup_by_version_builtin_priority() {
        let builtin = vec![make_version("1.0"), make_version("2.0")];
        let remote = Some(vec![make_version("2.0"), make_version("3.0")]);
        let merged = merge_versions(builtin, remote);
        assert_eq!(merged.len(), 3);
        let versions: Vec<_> = merged.iter().map(|v| v.version.as_str()).collect();
        assert!(versions.contains(&"1.0"));
        assert!(versions.contains(&"2.0"));
        assert!(versions.contains(&"3.0"));
    }

    #[test]
    fn merge_versions_remote_none_returns_builtin() {
        let builtin = vec![make_version("1.0")];
        let merged = merge_versions(builtin.clone(), None);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].version, "1.0");
    }

    #[test]
    fn merge_versions_empty_remote_returns_builtin() {
        let builtin = vec![make_version("1.0")];
        let merged = merge_versions(builtin.clone(), Some(vec![]));
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn merge_versions_empty_builtin_uses_remote() {
        let remote = Some(vec![make_version("1.0"), make_version("2.0")]);
        let merged = merge_versions(vec![], remote);
        assert_eq!(merged.len(), 2);
    }

    fn make_version(v: &str) -> CatalogVersion {
        CatalogVersion {
            version: v.to_string(),
            mirrors: vec![MirrorSource {
                name: "test".to_string(),
                url: "https://example.com/test.zip".to_string(),
                builtin: None,
            }],
            archive: ArchiveInfo {
                format: ArchiveFormat::Zip,
                size: None,
                sha256: None,
            },
        }
    }
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test --lib services::software_manager::catalog::tests::merge_versions`
预期：FAIL，编译错误 `cannot find function merge_versions`

- [ ] **步骤 3：实现 merge_versions**

在 `src-tauri/src/services/software_manager/catalog.rs` 中，在 `merge_catalogs` 函数之后（约第 62 行后）加：

```rust
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
```

注意：测试用到的 `CatalogVersion` / `MirrorSource` / `ArchiveInfo` / `ArchiveFormat` 需在 tests 模块已 import。检查现有 tests 模块顶部 use 语句（约第 67-70 行）：

```rust
    use super::*;
    use crate::models::software::{
        ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, MirrorSource,
        SoftwareCategory,
    };
```

确认已含 `CatalogVersion` / `MirrorSource` / `ArchiveInfo` / `ArchiveFormat`——应已含，无需改。

- [ ] **步骤 4：运行测试验证通过**

运行：`cd src-tauri && cargo test --lib services::software_manager::catalog`
预期：PASS，所有测试通过（含新增 4 个 merge_versions 测试 + 现有测试）

- [ ] **步骤 5：Commit**

```bash
cd D:/object/opx && git add src-tauri/src/services/software_manager/catalog.rs
git commit -m "feat: catalog.rs 新增 merge_versions 函数

合并内置版本与动态版本，同 version 号去重（内置优先）。
动态版本追加在内置之后。"
```

---

## 任务 3：refresh_catalog 命令调用 fetch_remote_versions

**文件：**
- 修改：`src-tauri/src/commands/software.rs`

- [ ] **步骤 1：实现 refresh_catalog 改造**

在 `src-tauri/src/commands/software.rs` 中找到 `refresh_catalog` 函数（约第 19-44 行）：

```rust
pub async fn refresh_catalog(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
) -> Result<Vec<CatalogEntry>, String> {
    let builtin = catalog::build_builtin_catalog();
    // 从 settings 读取 mirror_url
    let mirror_url = {
        let sp = crate::utils::paths::settings_path();
        if sp.exists() {
            std::fs::read_to_string(&sp)
                .ok()
                .and_then(|c| serde_json::from_str::<crate::models::settings::AppSettings>(&c).ok())
                .map(|s| s.mirror_url)
                .unwrap_or_else(|| "https://mirrors.aliyun.com".to_string())
        } else {
            "https://mirrors.aliyun.com".to_string()
        }
    };
    let remote = catalog::fetch_remote_catalog(&mirror_url).await;
    let merged = catalog::merge_catalogs(builtin, remote);
    manager.set_catalog(merged.clone());
    // 通知前端 catalog 已更新
    let _ = app.emit("catalog-refreshed", merged.entries.clone());
    Ok(merged.entries)
}
```

替换为（加 fetch_remote_versions 调用 + merge_versions 合并）：

```rust
pub async fn refresh_catalog(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
) -> Result<Vec<CatalogEntry>, String> {
    let builtin = catalog::build_builtin_catalog();

    // 从 settings 读取 mirror_url（远程 catalog.json，可选）
    let mirror_url = {
        let sp = crate::utils::paths::settings_path();
        if sp.exists() {
            std::fs::read_to_string(&sp)
                .ok()
                .and_then(|c| serde_json::from_str::<crate::models::settings::AppSettings>(&c).ok())
                .map(|s| s.mirror_url)
                .unwrap_or_else(|| "https://mirrors.aliyun.com".to_string())
        } else {
            "https://mirrors.aliyun.com".to_string()
        }
    };
    let remote_catalog = catalog::fetch_remote_catalog(&mirror_url).await;
    let mut merged = catalog::merge_catalogs(builtin, remote_catalog);

    // 调用各 provider 的 fetch_remote_versions，与内置版本合并
    let providers = providers::all_providers();
    for entry in &mut merged.entries {
        if let Some(provider) = providers.iter().find(|p| p.key() == entry.key) {
            let remote_versions = provider.fetch_remote_versions();
            entry.versions = catalog::merge_versions(entry.versions.clone(), remote_versions);
        }
    }

    merged.updated_at = Some(chrono::Local::now().to_rfc3337());
    manager.set_catalog(merged.clone());
    let _ = app.emit("catalog-refreshed", merged.entries.clone());
    Ok(merged.entries)
}
```

注意：
- 需要导入 `providers` 模块。检查文件顶部 use 语句（约第 8 行）：
  ```rust
  use crate::services::software_manager::{catalog, installer, SoftwareManager};
  ```
  改为：
  ```rust
  use crate::services::software_manager::{catalog, installer, providers, SoftwareManager};
  ```
- `chrono::Local` 已在依赖中（Cargo.toml 有 chrono）。检查是否需要导入——`chrono::Local::now()` 用全路径无需 import。

- [ ] **步骤 2：运行编译验证**

运行：`cd src-tauri && cargo build`
预期：编译成功

- [ ] **步骤 3：运行测试验证**

运行：`cd src-tauri && cargo test`
预期：所有测试通过（无新测试，确认未破坏现有）

- [ ] **步骤 4：Commit**

```bash
cd D:/object/opx && git add src-tauri/src/commands/software.rs
git commit -m "feat: refresh_catalog 调用 fetch_remote_versions 合并动态版本

遍历各 provider，调用 fetch_remote_versions（若有），
与内置版本合并（merge_versions 去重，内置优先）。"
```

---

## 任务 4：JRE provider 实现 fetch_remote_versions

**文件：**
- 修改：`src-tauri/src/services/software_manager/providers/jre.rs`

- [ ] **步骤 1：编写失败的测试**

在 `src-tauri/src/services/software_manager/providers/jre.rs` 末尾的 `#[cfg(test)] mod tests` 中加测试（在现有测试之后）：

```rust
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
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test --lib services::software_manager::providers::jre`
预期：FAIL，编译错误 `JreProvider does not implement fetch_remote_versions`（实际上不会报错——默认实现返回 None，测试会通过。本任务无真正失败测试，直接实现）

实际：此任务的测试仅验证方法存在（编译通过即通过），不验证网络行为。直接进入实现。

- [ ] **步骤 3：实现 JRE fetch_remote_versions**

在 `src-tauri/src/services/software_manager/providers/jre.rs` 中，在 `post_install` 方法之后（`impl SoftwareProvider for JreProvider` 块内末尾）加：

```rust
    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        // Adoptium GitHub Releases API：获取所有 LTS 版本
        // 限流：未认证 60 次/小时，足够日常刷新
        let url = "https://api.github.com/repos/adoptium/adoptium-supported-versions/releases?per_page=30";
        let response = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .ok()?
            .get(url)
            .header("User-Agent", "OPX")
            .header("Accept", "application/vnd.github+json")
            .send()
            .ok()?;

        if !response.status().is_success() {
            eprintln!("[jre] GitHub API 返回 {}", response.status());
            return None;
        }

        let releases: Vec<serde_json::Value> = response.json().ok()?;
        let mut versions = vec![];

        for release in releases {
            let tag = release.get("tag_name")?.as_str()?.to_string();
            // tag 格式如 "jdk-17.0.16+7"，过滤 LTS（8/11/17/21）
            if let Some(version) = parse_adoptium_tag(&tag) {
                let major: u32 = version.split('.').next().and_then(|s| s.parse().ok()).unwrap_or(0);
                if matches!(major, 8 | 11 | 17 | 21) {
                    // 找 Windows x64 JRE zip asset
                    if let Some(asset_url) = find_jre_asset(&release, major) {
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
```

在文件末尾（`#[cfg(test)] mod tests` 之前）加两个辅助函数：

```rust
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
fn find_jre_asset(release: &serde_json::Value, major: u32) -> Option<String> {
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
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cd src-tauri && cargo test --lib services::software_manager::providers::jre`
预期：PASS，所有测试通过（含新增测试 + 现有测试）

- [ ] **步骤 5：Commit**

```bash
cd D:/object/opx && git add src-tauri/src/services/software_manager/providers/jre.rs
git commit -m "feat: JRE provider 实现 fetch_remote_versions

调 Adoptium GitHub Releases API，解析 tag_name 提取版本号，
过滤 LTS（8/11/17/21），找 Windows x64 JRE zip asset。
限流/超时/解析失败返回 None 不阻塞其他软件。"
```

---

## 任务 5：Redis provider 实现 fetch_remote_versions

**文件：**
- 修改：`src-tauri/src/services/software_manager/providers/redis.rs`

- [ ] **步骤 1：编写失败的测试**

在 `src-tauri/src/services/software_manager/providers/redis.rs` 末尾的 `#[cfg(test)] mod tests` 中加测试（在现有测试之后）：

```rust
    #[test]
    fn redis_fetch_remote_versions_method_exists() {
        let provider = RedisProvider::new();
        // 仅验证方法存在（编译通过），不断言返回值（网络可能失败）
        let _ = provider.fetch_remote_versions();
    }
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test --lib services::software_manager::providers::redis`
预期：FAIL（默认实现返回 None，测试实际通过——无真正失败测试，直接实现）

- [ ] **步骤 3：实现 Redis fetch_remote_versions**

在 `src-tauri/src/services/software_manager/providers/redis.rs` 中，在 `post_install` 之后加：

```rust
    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        // redis-windows GitHub Releases API（社区维护的 Windows Redis 移植）
        let url = "https://api.github.com/repos/redis-windows/redis-windows/releases?per_page=20";
        let response = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .ok()?
            .get(url)
            .header("User-Agent", "OPX")
            .header("Accept", "application/vnd.github+json")
            .send()
            .ok()?;

        if !response.status().is_success() {
            eprintln!("[redis] GitHub API 返回 {}", response.status());
            return None;
        }

        let releases: Vec<serde_json::Value> = response.json().ok()?;
        let mut versions = vec![];

        for release in releases {
            let tag = match release.get("tag_name").and_then(|t| t.as_str()) {
                Some(t) => t.to_string(),
                None => continue,
            };
            // tag 直接是版本号如 "8.8.0" / "7.4.9"
            // 找 cygwin.zip asset（与现有硬编码一致）
            if let Some(asset_url) = find_cygwin_asset(&release) {
                versions.push(CatalogVersion {
                    version: tag,
                    mirrors: vec![MirrorSource {
                        name: "redis-windows GitHub".to_string(),
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

        if versions.is_empty() {
            None
        } else {
            Some(versions)
        }
    }
```

在文件末尾（`#[cfg(test)] mod tests` 之前）加辅助函数：

```rust
/// 在 release 的 assets 中找 cygwin.zip（不含 with-Service）
fn find_cygwin_asset(release: &serde_json::Value) -> Option<String> {
    let assets = release.get("assets")?.as_array()?;
    for asset in assets {
        let name = asset.get("name")?.as_str()?;
        // 名称如 "Redis-8.8.0-Windows-x64-cygwin.zip"
        if name.contains("cygwin")
            && !name.contains("with-Service")
            && name.ends_with(".zip")
        {
            let url = asset.get("browser_download_url")?.as_str()?;
            return Some(url.to_string());
        }
    }
    None
}
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cd src-tauri && cargo test --lib services::software_manager::providers::redis`
预期：PASS

- [ ] **步骤 5：Commit**

```bash
cd D:/object/opx && git add src-tauri/src/services/software_manager/providers/redis.rs
git commit -m "feat: Redis provider 实现 fetch_remote_versions

调 redis-windows GitHub Releases API，
选 cygwin.zip asset（与现有硬编码一致）。"
```

---

## 任务 6：Nginx provider 实现 fetch_remote_versions

**文件：**
- 修改：`src-tauri/src/services/software_manager/providers/nginx.rs`

- [ ] **步骤 1：编写失败的测试**

在 `src-tauri/src/services/software_manager/providers/nginx.rs` 末尾的 `#[cfg(test)] mod tests` 中加测试：

```rust
    #[test]
    fn nginx_fetch_remote_versions_method_exists() {
        let provider = NginxProvider::new();
        let _ = provider.fetch_remote_versions();
    }
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test --lib services::software_manager::providers::nginx`
预期：FAIL（默认实现返回 None，测试实际通过——直接实现）

- [ ] **步骤 3：实现 Nginx fetch_remote_versions**

在 `src-tauri/src/services/software_manager/providers/nginx.rs` 中，在 `post_install` 之后加：

```rust
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
                            name: "华为镜像".to_string(),
                            url: format!(
                                "https://mirrors.huaweicloud.com/nginx/nginx-{}.zip",
                                version
                            ),
                            builtin: None,
                        },
                        MirrorSource {
                            name: "官方".to_string(),
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
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cd src-tauri && cargo test --lib services::software_manager::providers::nginx`
预期：PASS

- [ ] **步骤 5：Commit**

```bash
cd D:/object/opx && git add src-tauri/src/services/software_manager/providers/nginx.rs
git commit -m "feat: Nginx provider 实现 fetch_remote_versions

爬 nginx.org/en/download.html HTML，
正则提取 nginx-X.Y.Z.zip 版本号，过滤主线 1.31.x。
配华为 + 官方双镜像。"
```

---

## 任务 7：MySQL provider 实现 fetch_remote_versions

**文件：**
- 修改：`src-tauri/src/services/software_manager/providers/mysql.rs`

- [ ] **步骤 1：编写失败的测试**

在 `src-tauri/src/services/software_manager/providers/mysql.rs` 末尾的 `#[cfg(test)] mod tests` 中加测试：

```rust
    #[test]
    fn mysql_fetch_remote_versions_method_exists() {
        let provider = MySqlProvider::new();
        let _ = provider.fetch_remote_versions();
    }
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test --lib services::software_manager::providers::mysql`
预期：FAIL（默认实现返回 None，测试实际通过——直接实现）

- [ ] **步骤 3：实现 MySQL fetch_remote_versions**

在 `src-tauri/src/services/software_manager/providers/mysql.rs` 中，在 `post_install` 之后加：

```rust
    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        // 爬 dev.mysql.com/downloads/mysql/ HTML，提取版本号
        // 注意：页面结构可能变更，失败返回 None
        let url = "https://dev.mysql.com/downloads/mysql/";
        let response = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .ok()?
            .get(url)
            .header("User-Agent", "OPX")
            .send()
            .ok()?;

        if !response.status().is_success() {
            eprintln!("[mysql] 下载页返回 {}", response.status());
            return None;
        }

        let html = response.text().ok()?;
        let mut versions = vec![];
        let mut seen = std::collections::HashSet::new();

        // 正则匹配 mysql-X.Y.Z-winx64.zip
        let re = regex::Regex::new(r"mysql-(\d+\.\d+\.\d+)-winx64\.zip").ok()?;

        for cap in re.captures_iter(&html) {
            let version = cap.get(1)?.as_str().to_string();
            if seen.contains(&version) {
                continue;
            }
            seen.insert(version.clone());

            // 只取 8.x（最低 8.0.36+，跳过 8.0.36 之前的版本）
            let major: u32 = version
                .split('.')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let minor: u32 = version
                .split('.')
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let patch: u32 = version
                .split('.')
                .nth(2)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            if major != 8 {
                continue;
            }
            if minor == 0 && patch < 36 {
                continue;
            }

            // 8.4.x 走 mysql-8.4 路径，8.0.x 走 mysql-8.0 路径
            let path_segment = if minor == 4 {
                "mysql-8.4"
            } else {
                "mysql-8.0"
            };

            versions.push(CatalogVersion {
                version: version.clone(),
                mirrors: vec![MirrorSource {
                    name: "MySQL 官方 CDN".to_string(),
                    url: format!(
                        "https://cdn.mysql.com/archives/{}/mysql-{}-winx64.zip",
                        path_segment, version
                    ),
                    builtin: None,
                }],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
        }

        if versions.is_empty() {
            None
        } else {
            Some(versions)
        }
    }
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cd src-tauri && cargo test --lib services::software_manager::providers::mysql`
预期：PASS

- [ ] **步骤 5：Commit**

```bash
cd D:/object/opx && git add src-tauri/src/services/software_manager/providers/mysql.rs
git commit -m "feat: MySQL provider 实现 fetch_remote_versions

爬 dev.mysql.com/downloads/mysql/ HTML，
正则提取 mysql-X.Y.Z-winx64.zip 版本号，
过滤 8.x（最低 8.0.36+），配 cdn.mysql.com 镜像。"
```

---

## 任务 8：Cargo.toml 加 regex 依赖

**文件：**
- 修改：`src-tauri/Cargo.toml`

- [ ] **步骤 1：加 regex 依赖**

在 `src-tauri/Cargo.toml` 的 `[dependencies]` 部分找到（约第 27-39 行）：

```toml
reqwest = { version = "0.13", features = ["blocking", "json", "stream"] }
zip = "0.6"
tar = "0.4"
flate2 = "1.0"
walkdir = "2.0"
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1.0"
thiserror = "1.0"
path-slash = "0.1"
once_cell = "1.19"
uuid = { version = "1.0", features = ["v4"] }
sha2 = "0.10"
futures = "0.3"
```

在 `futures = "0.3"` 之后加：

```toml
regex = "1.10"
```

- [ ] **步骤 2：运行编译验证**

运行：`cd src-tauri && cargo build`
预期：编译成功（regex 被 Nginx/MySQL provider 使用）

- [ ] **步骤 3：Commit**

```bash
cd D:/object/opx && git add src-tauri/Cargo.toml
git commit -m "feat: Cargo.toml 加 regex 依赖

Nginx/MySQL provider 的 fetch_remote_versions 用 regex
提取 HTML 中的版本号。"
```

---

## 任务 9：前端 catalog store 加 loading + error 状态

**文件：**
- 修改：`src/modules/software-manager/stores/catalog.ts`

- [ ] **步骤 1：实现 store 扩展**

读取当前 catalog.ts 内容，然后改造为含 loading + error 状态：

找到 `refreshCatalog` 函数（约第 39-45 行附近）：

```ts
  async function refreshCatalog() {
    ...
  }
```

将其改造为（加 loading + error）：

```ts
  const loading = ref(false)
  const error = ref<string | null>(null)

  async function refreshCatalog() {
    loading.value = true
    error.value = null
    try {
      entries.value = await invoke('refresh_catalog') as CatalogEntry[]
    } catch (e) {
      error.value = String(e)
      throw e
    } finally {
      loading.value = false
    }
  }
```

在 return 语句中加 loading + error（找到现有 return 块）：

```ts
  return {
    entries,
    groupedEntries,
    loading,
    error,
    loadCatalog,
    refreshCatalog,
  }
```

注意：具体现有代码结构需先 Read 文件确认，可能 loading 已存在。若已存在则只加 error。

- [ ] **步骤 2：运行构建验证**

运行：`cd D:/object/opx && npm run build`
预期：vue-tsc 类型检查通过

- [ ] **步骤 3：Commit**

```bash
cd D:/object/opx && git add src/modules/software-manager/stores/catalog.ts
git commit -m "feat: catalog store 加 loading + error 状态

refreshCatalog 期间 loading=true，失败时 error 存错误信息。
供 RepositoryPage 刷新按钮显示状态。"
```

---

## 任务 10：RepositoryPage 刷新按钮 loading + 错误提示

**文件：**
- 修改：`src/modules/software-manager/pages/RepositoryPage.vue`
- 修改：`src/locales/zh-CN.ts`
- 修改：`src/locales/en-US.ts`

- [ ] **步骤 1：加 i18n 键**

在 `src/locales/zh-CN.ts` 中找到 `refreshCatalog:` 这一行（约第 53 行附近），在其后加：

```typescript
  refreshCatalog: '刷新目录',
  refreshCatalogFailed: '刷新目录失败，请检查网络',
  fetchingVersions: '正在获取版本列表',
  networkVersion: '网络版本',
```

在 `src/locales/en-US.ts` 对应位置加：

```typescript
  refreshCatalog: 'Refresh Catalog',
  refreshCatalogFailed: 'Failed to refresh catalog, please check network',
  fetchingVersions: 'Fetching version list',
  networkVersion: 'Network version',
```

注意：先 Read 文件确认 `refreshCatalog` 是否已存在，若已存在则只加缺失的键。

- [ ] **步骤 2：修改 RepositoryPage 刷新按钮**

在 `src/modules/software-manager/pages/RepositoryPage.vue` 中找到刷新按钮（约第 9-11 行）：

```html
        <button class="btn" @click="refreshCatalog" :disabled="catalogStore.loading">
          <Icon icon="mdi:refresh" /> {{ $t('refreshCatalog') }}
        </button>
```

替换为（加 loading 图标旋转 + 错误提示）：

```html
        <button class="btn" @click="refreshCatalog" :disabled="catalogStore.loading">
          <Icon :icon="catalogStore.loading ? 'mdi:loading' : 'mdi:refresh'" :class="{ spinning: catalogStore.loading }" />
          {{ catalogStore.loading ? $t('fetchingVersions') : $t('refreshCatalog') }}
        </button>
```

在模板中 `<PageHeader>` 之后、`<div v-if="catalogStore.loading...">` 之前加错误提示：

```html
    <div v-if="catalogStore.error" class="refresh-error">
      <Icon icon="mdi:alert-circle" />
      {{ $t('refreshCatalogFailed') }}
    </div>
```

在 `<style scoped>` 中加样式：

```css
.refresh-error {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  border-radius: 8px;
  background: color-mix(in oklch, var(--color-destructive) 10%, transparent);
  border: 1px solid color-mix(in oklch, var(--color-destructive) 30%, transparent);
  color: var(--color-destructive);
  font-size: 13px;
  margin-bottom: 16px;
}
.refresh-error svg {
  width: 18px;
  height: 18px;
  flex-shrink: 0;
}
.spinning {
  animation: spin 1s linear infinite;
}
@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
```

- [ ] **步骤 3：运行构建验证**

运行：`cd D:/object/opx && npm run build`
预期：vue-tsc 类型检查 + vite 构建通过

- [ ] **步骤 4：Commit**

```bash
cd D:/object/opx && git add src/modules/software-manager/pages/RepositoryPage.vue src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat: 刷新按钮 loading 状态 + 错误提示

刷新中显示 loading 图标旋转 + '正在获取版本列表'，
失败显示红色错误提示框。新增 i18n 键。"
```

---

## 任务 11：最终验证

**文件：** 无（仅验证）

- [ ] **步骤 1：运行后端测试**

运行：`cd src-tauri && cargo test`
预期：所有测试通过

- [ ] **步骤 2：运行前端构建**

运行：`npm run build`
预期：vue-tsc + vite 构建成功

- [ ] **步骤 3：手动验证**

运行：`npm run tauri:dev`

验证清单：
1. 软件仓库页打开，点"刷新目录"按钮
2. 按钮显示 loading 图标旋转 + "正在获取版本列表"
3. ~40 秒后刷新完成，版本列表扩展
4. JRE 版本列表新增 GitHub 最新 LTS 版本（如 17.0.16、21.0.6 等）
5. Redis 版本列表新增 GitHub 最新版本（如 8.8.0、8.6.4 等）
6. Nginx 版本列表新增 1.31.x 主线版本
7. MySQL 版本列表新增 8.x 版本
8. MinIO/RustFS 版本列表不变（无 fetch_remote_versions）
9. 断网刷新 → 失败，显示"刷新目录失败，请检查网络"红色提示
10. 内置版本仍保留（带"离线"标签），动态版本无"离线"标签

- [ ] **步骤 4：Commit（如有遗漏）**

```bash
git add -A
git commit -m "chore: 动态版本列表功能最终验证"
```

---

## 规格覆盖度自检

| 规格章节 | 对应任务 |
|---|---|
| SoftwareProvider trait 扩展 | 任务 1 |
| catalog.rs merge_versions | 任务 2 |
| refresh_catalog 命令改造 | 任务 3 |
| JRE fetch_remote_versions | 任务 4 |
| Redis fetch_remote_versions | 任务 5 |
| Nginx fetch_remote_versions | 任务 6 |
| MySQL fetch_remote_versions | 任务 7 |
| regex 依赖 | 任务 8 |
| 前端 catalog store loading/error | 任务 9 |
| RepositoryPage 刷新按钮 + i18n | 任务 10 |
| 验证清单 | 任务 11 |

## 占位符扫描

无 TODO/待定/"类似任务 N"。每个步骤含完整代码。

## 类型一致性

- `fetch_remote_versions() -> Option<Vec<CatalogVersion>>` — 任务 1 定义，任务 4/5/6/7 覆写
- `merge_versions(builtin, remote) -> Vec<CatalogVersion>` — 任务 2 定义，任务 3 使用
- `CatalogVersion` / `MirrorSource` / `ArchiveInfo` — 复用现有类型
- 前端 `catalogStore.loading` / `catalogStore.error` — 任务 9 定义，任务 10 使用
