# 动态版本列表功能设计文档

**日期**: 2026-07-01
**项目位置**: `D:\object\opx`
**技术栈**: Rust + Tauri 2 + Vue 3 + TypeScript
**阶段**: 软件仓库扩展 — 动态拉取远程版本列表

## 概述

JRE/MySQL/Redis/Nginx 这 4 个软件当前 catalog 版本列表是 provider 代码里硬编码的固定版本。本次新增"动态拉取远程版本列表"能力：用户点"刷新 catalog"时，后端调用各软件的远程 API/GitHub Releases 爬取最新版本列表，与内置版本合并展示。MinIO/RustFS 保持现有固定版本（内置 + latest）。

### 范围边界

**包含**：
- SoftwareProvider trait 加可选方法 `fetch_remote_versions()`
- JRE/MySQL/Redis/Nginx 4 个 provider 实现 fetch_remote_versions
- catalog 合并逻辑：内置版本 + 动态版本去重（内置优先）
- 前端"刷新 catalog"按钮 loading 状态 + 错误提示（i18n）
- 失败容错：单个软件拉取失败不影响其他

**不包含**：
- 版本列表缓存（每次刷新都重新拉，YAGNI）
- MinIO/RustFS 动态拉取（用户已确认固定）
- 远程 catalog.json 配置文件机制（与本次方案不冲突，可共存）
- 版本号过滤 UI（各 provider 自己实现 LTS/稳定版过滤）

### 关键约束（用户确认）

1. **范围**：JRE/MySQL/Redis/Nginx 动态拉取，MinIO/RustFS 固定
2. **触发时机**：手动点"刷新 catalog"按钮，不自动
3. **版本合并**：内置 + 动态共存，同 version 号去重（内置优先）
4. **动态版本镜像**：单官方镜像（GitHub release / 官方 CDN）
5. **i18n**：所有用户可见文案走 i18n

## 架构与模块边界

### 整体流程

```
用户点"刷新 catalog"按钮
  │
  ▼ invoke refresh_catalog
后端 refresh_catalog 命令：
  1. build_builtin_catalog() — provider 内置版本（含 builtin 项）
  2. 并发调用各 provider 的 fetch_remote_versions()（若有）
     ├─ JRE: Adoptium GitHub Releases API
     ├─ MySQL: 爬 dev.mysql.com downloads 页面 HTML
     ├─ Redis: redis-windows GitHub Releases API
     └─ Nginx: 爬 nginx.org download 页面 HTML
  3. 合并：每个软件的内置版本 + 动态版本，同 version 号去重（内置优先）
  4. 返回合并后 catalog
  5. 前端更新展示
```

### 文件改动清单

```
src-tauri/src/
├── services/software_manager/
│   ├── catalog.rs                    # 新增 merge_versions 函数
│   └── providers/
│       ├── mod.rs                    # SoftwareProvider trait 加 fetch_remote_versions
│       ├── jre.rs                    # 实现 fetch_remote_versions
│       ├── mysql.rs                  # 实现 fetch_remote_versions
│       ├── redis.rs                  # 实现 fetch_remote_versions
│       └── nginx.rs                  # 实现 fetch_remote_versions
└── commands/software.rs              # refresh_catalog 调用 fetch_remote_versions

src/modules/software-manager/
├── pages/RepositoryPage.vue          # 刷新按钮 loading + 错误提示
└── stores/catalog.ts                 # loadCatalog 返回错误信息

src/locales/zh-CN.ts                 # 新增 i18n 键
src/locales/en-US.ts                 # 新增 i18n 键
```

## 数据模型

### SoftwareProvider trait 扩展

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

**设计决策**：返回 `Option<Vec<CatalogVersion>>` 而非 `Result`，因为：
- 拉取失败是预期场景（网络/限流/解析），不应传播错误
- None 表示"该软件无动态版本或不支持"，调用方简单跳过
- 错误细节通过 eprintln 日志记录，不暴露给前端

### CatalogVersion 复用

动态版本复用现有 `CatalogVersion` 结构，无需新增类型：

```rust
CatalogVersion {
    version: "17.0.16".to_string(),
    mirrors: vec![MirrorSource {
        name: "Adoptium GitHub".to_string(),
        url: "https://github.com/adoptium/temurin17-binaries/releases/download/jdk-17.0.16%2B7/...".to_string(),
        builtin: None,  // 动态版本无内置项
    }],
    archive: ArchiveInfo {
        format: ArchiveFormat::Zip,
        size: None,
        sha256: None,
    },
}
```

每个 provider 的 `fetch_remote_versions` 负责生成完整的 CatalogVersion（含 mirrors + archive.format）。

## 版本合并逻辑

### catalog.rs 新增 merge_versions

```rust
/// 合并内置版本与动态版本：
/// - 同 version 号去重，内置优先（保留内置的 mirrors，含 builtin 项）
/// - 动态版本追加在内置版本之后
/// - 内置为空时直接用动态版本
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

### refresh_catalog 改造

```rust
pub async fn refresh_catalog(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
) -> Result<Vec<CatalogEntry>, String> {
    let builtin = catalog::build_builtin_catalog();

    // 并发拉取各软件的远程版本
    let providers = all_providers();
    let mut merged_entries = Vec::new();

    for entry in &builtin.entries {
        // 找到对应的 provider
        let provider = providers.iter().find(|p| p.key() == entry.key);

        // 调用 fetch_remote_versions（若 provider 实现了）
        let remote_versions = provider.and_then(|p| p.fetch_remote_versions());

        let merged_versions = catalog::merge_versions(entry.versions.clone(), remote_versions);

        merged_entries.push(CatalogEntry {
            versions: merged_versions,
            ..entry.clone()
        });
    }

    let merged = Catalog {
        entries: merged_entries,
        updated_at: Some(chrono::Local::now().to_rfc3339()),
    };

    manager.set_catalog(merged.clone());
    let _ = app.emit("catalog-refreshed", merged.entries.clone());
    Ok(merged.entries)
}
```

**注意**：`all_providers()` 返回 `Vec<Box<dyn SoftwareProvider>>`，每次调用创建新实例。fetch_remote_versions 是网络 IO，会阻塞。为简单起见，本次**串行**调用各 provider（非并发），4 个软件总耗时 ~40 秒（每个 10 秒超时）。若性能不可接受，后续可改为并发（tokio::join_all）。

## 4 个 provider 的实现

### JRE provider

**来源**：Adoptium GitHub Releases API
- URL: `https://api.github.com/repos/adoptium/adoptium-supported-versions/releases?per_page=20`
- 解析 JSON：`[{tag_name: "jdk-17.0.16+7", ...}, ...]`
- 过滤：只取 LTS（8/11/17/21 主版本号）
- 生成 CatalogVersion：
  - version = "17.0.16"（从 tag_name 解析）
  - mirrors = [Adoptium GitHub release URL]
  - archive.format = Zip

**URL 生成**：`https://github.com/adoptium/temurin{major}-binaries/releases/download/{tag}/{asset_name}`
- tag 如 `jdk-17.0.16+7`
- asset_name 如 `OpenJDK17U-jre_x64_windows_hotspot_17.0.16_7.zip`

**简化**：API 返回的 release 对象含 `assets` 数组，直接遍历找 Windows x64 JRE zip 的 browser_download_url，无需手动拼接。

### MySQL provider

**来源**：爬 `https://dev.mysql.com/downloads/mysql/` HTML
- 正则提取版本号：`mysql-([0-9]+\.[0-9]+\.[0-9]+)-winx64.zip`
- 过滤：只取 8.x（最低 8.0.36+）
- 生成 CatalogVersion：
  - version = "8.0.42"
  - mirrors = [cdn.mysql.com URL]
  - archive.format = Zip

**URL 生成**：`https://cdn.mysql.com/archives/mysql-{major}.{minor}/mysql-{version}-winx64.zip`

**脆弱性**：HTML 结构变更会导致正则失配。失败返回 None，eprintln 日志。备份方案：硬编码几个稳定版本（8.0.36/8.4.0）作为 fallback。

### Redis provider

**来源**：redis-windows GitHub Releases API
- URL: `https://api.github.com/repos/redis-windows/redis-windows/releases?per_page=20`
- 解析 JSON：`[{tag_name: "8.8.0", assets: [{browser_download_url: "...cygwin.zip"}]}, ...]`
- 过滤：取所有版本（用户之前确认偶数次稳定版规则，但动态列表让用户自选，不强制过滤）
- 生成 CatalogVersion：
  - version = "8.8.0"
  - mirrors = [redis-windows GitHub URL]
  - archive.format = Zip

**asset 选择**：每个 release 有多个 asset（cygwin/msys2/with-Service），优先选 `cygwin.zip`（与现有硬编码一致）。

### Nginx provider

**来源**：爬 `https://nginx.org/en/download.html` HTML
- 正则提取版本号：`nginx-([0-9]+\.[0-9]+\.[0-9]+)\.zip`
- 过滤：取主线版本（1.31.x），可选包含稳定版（1.30.x）
- 生成 CatalogVersion：
  - version = "1.31.3"
  - mirrors = [nginx.org URL, 华为镜像 URL]
  - archive.format = Zip

**URL 生成**：
- 官方：`https://nginx.org/download/nginx-{version}.zip`
- 华为：`https://mirrors.huaweicloud.com/nginx/nginx-{version}.zip`

**脆弱性**：同 MySQL，HTML 变更返回 None。

## GitHub API 限流处理

未认证 GitHub API 限流 60 次/小时。每次刷新调 2 个软件（JRE + Redis）的 API，即 2 次。刷新频率低可接受。

**限流响应**：HTTP 403 + `X-RateLimit-Remaining: 0` 头。检测到时返回 None，eprintln 提示。

**未来扩展**：若需提高限流，可在 settings.json 加 GitHub token 字段，请求时加 Authorization header。本次 YAGNI 不做。

## 前端 UI 改造

### RepositoryPage 刷新按钮

当前刷新按钮无 loading 状态。改造：

```vue
<button
  class="btn"
  @click="refreshCatalog"
  :disabled="catalogStore.loading"
>
  <Icon :icon="catalogStore.loading ? 'mdi:loading' : 'mdi:refresh'" />
  {{ $t('refreshCatalog') }}
</button>
```

### 错误提示

刷新失败时显示 toast 或 inline 提示（i18n）：

```ts
async function refreshCatalog() {
  try {
    await catalogStore.refreshCatalog()
    // 成功无提示（catalog 自动更新）
  } catch (e) {
    // 失败：显示错误提示
    refreshError.value = t('refreshCatalogFailed')
  }
}
```

### catalog store 扩展

```ts
export const useCatalogStore = defineStore('catalog', () => {
  const entries = ref<CatalogEntry[]>([])
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

  // ...
})
```

## i18n 文案

### 新增键

```
refreshCatalogFailed: '刷新目录失败，请检查网络'
fetchingVersions: '正在获取版本列表'
networkVersion: '网络版本'
```

zh-CN：
```typescript
refreshCatalogFailed: '刷新目录失败，请检查网络',
fetchingVersions: '正在获取版本列表',
networkVersion: '网络版本',
```

en-US：
```typescript
refreshCatalogFailed: 'Failed to refresh catalog, please check network',
fetchingVersions: 'Fetching version list',
networkVersion: 'Network version',
```

### 已有键复用

- `refreshCatalog`: '刷新目录' / 'Refresh Catalog'（已存在）
- `loading`: '加载中' / 'Loading'（已存在）

## 错误处理

| 错误场景 | 处理方式 | 用户可见 |
|---|---|---|
| GitHub API 限流 (403) | 该软件返回 None，eprintln 日志 | 无（该软件保持内置版本） |
| GitHub API 超时 | 10 秒超时，返回 None | 无 |
| MySQL/Nginx HTML 爬取失败 | 返回 None，eprintln 日志 | 无 |
| HTML 结构变更（正则失配） | 返回空 Vec 或 None | 无（该软件无动态版本） |
| 整体 refresh_catalog 失败 | 前端显示 toast "刷新目录失败" | 是 |
| 单个软件 fetch_remote_versions panic | 不应发生；若发生，catch_unwind 防止崩溃 | 无 |

**关键原则**：单个软件拉取失败不影响其他软件。每个 provider 的 fetch_remote_versions 独立 try/catch（用 Result 内部处理，返回 Option）。

## 测试策略

### 后端单元测试

| 模块 | 测试内容 |
|---|---|
| `catalog.rs merge_versions` | 内置 + 动态去重；内置优先；动态为空返回内置；内置为空用动态；同 version 号合并 |
| `providers/jre.rs fetch_remote_versions` | mock GitHub API 返回，解析版本号正确；过滤非 LTS；限流返回 None |
| `providers/redis.rs fetch_remote_versions` | mock GitHub API，选 cygwin.zip asset |
| `providers/mysql.rs fetch_remote_versions` | mock HTML，正则提取版本号 |
| `providers/nginx.rs fetch_remote_versions` | mock HTML，正则提取版本号 |

mock HTTP 用 dev-dependency `mockito`（已在 Cargo.toml）。

### 手动验证

1. `npm run tauri:dev` 启动
2. 软件仓库页点"刷新目录"按钮
3. JRE 版本列表从 4 个（内置 2 + 硬编码 2）扩展到多个（含 GitHub 最新）
4. Redis 版本列表从 3 个扩展到多个
5. MySQL/Nginx 版本列表扩展（取决于 HTML 爬取成功）
6. 断网刷新 → 失败 toast，catalog 保持内置版本

## 设计决策总结

| 决策点 | 选择 | 理由 |
|---|---|---|
| trait 方法签名 | `Option<Vec<CatalogVersion>>` | 失败是预期，不传播错误 |
| 拉取时机 | 手动刷新触发 | 不自动，避免每次启动耗时 |
| 并发 vs 串行 | 串行（本次） | 简单；4 个软件 ~40 秒可接受；后续可改并发 |
| 版本合并 | 内置优先去重 | 内置项保留（含 builtin zip），动态版本补充 |
| 动态版本镜像 | 单官方镜像 | 简单，与现有"内置 + 网络"模式对齐 |
| 版本缓存 | 不缓存（YAGNI） | 每次刷新重新拉 |
| HTML 爬取容错 | 返回 None 不阻塞 | 单软件失败不影响其他 |
| MinIO/RustFS | 不实现 fetch_remote_versions | 用户已确认固定 |

## 规格自检

| 检查项 | 状态 | 说明 |
|---|---|---|
| 占位符扫描 | ✅ 无 | 无 TODO/待定 |
| 内部一致性 | ✅ 通过 | trait 签名、合并逻辑、UI 改造一致 |
| 范围检查 | ✅ 合适 | 4 个 provider + 合并 + UI，一个实现计划可覆盖 |
| 模糊性检查 | ✅ 通过 | 各软件的 URL/解析方式明确，i18n 键已列出 |

---

设计文档已编写完成，请审查。
