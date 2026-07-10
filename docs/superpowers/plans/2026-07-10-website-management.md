# 网站管理模块 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 新增"网站管理"模块，用已装 nginx 部署静态页面并配反向代理规则，站点 = server 块，保存即生成 conf 并可 reload。

**架构：** 后端 `WebsiteManager` 持久化 `websites.json`；纯函数 `nginx_conf` 生成 server 块与注入 include；命令层重建 `<nginx>/conf/sites/*.conf` 并 `nginx -t` 校验后 `reload`。前端卡片列表 + 站点编辑对话框（含 location 编辑器）。

**技术栈：** Tauri v2 / Rust（serde、已有 `zip` 依赖、`utils::archive::extract_zip`）/ Vue 3 + TS + Pinia + vue-i18n。

**规格：** `docs/superpowers/specs/2026-07-10-website-management-design.md`

---

## 文件结构

**后端**
- 创建 `src-tauri/src/models/website.rs` — Site / Location / SslConfig / 枚举 / WebsiteList
- 修改 `src-tauri/src/models/mod.rs` — 声明 `pub mod website;`
- 创建 `src-tauri/src/services/website_manager/mod.rs` — `WebsiteManager`（CRUD + 持久化）
- 创建 `src-tauri/src/services/website_manager/nginx_conf.rs` — 纯函数：生成 server 块 + include 注入
- 修改 `src-tauri/src/services/mod.rs` — 声明 `pub mod website_manager;`
- 创建 `src-tauri/src/commands/website.rs` — Tauri 命令 + nginx 应用/reload
- 修改 `src-tauri/src/commands/mod.rs` — 声明 `pub mod website;`
- 修改 `src-tauri/src/lib.rs` — `manage(WebsiteManager)` + 注册命令

**前端**
- 创建 `src/models/website.ts` — TS 类型镜像
- 创建 `src/modules/website-manager/pages/WebsiteListPage.vue` — 卡片列表
- 创建 `src/modules/website-manager/components/LocationEditor.vue` — location 编辑器
- 创建 `src/modules/website-manager/components/SiteEditDialog.vue` — 站点编辑对话框
- 修改 `src/router/index.ts` — 加 `/websites` 路由
- 修改 `src/layouts/Sidebar.vue` — 菜单项插在 `/software` 与 `/springboot` 之间
- 修改 `src/locales/zh-CN.ts`、`src/locales/en-US.ts` — 文案

---

## 任务 1：后端数据模型

**文件：**
- 创建：`src-tauri/src/models/website.rs`
- 修改：`src-tauri/src/models/mod.rs:1`（模块声明区）

- [ ] **步骤 1：编写模型 + 失败测试**

`src-tauri/src/models/website.rs`：
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LocationKind {
    Static,
    Proxy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StaticSource {
    Dir,
    Upload,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub path: String,
    pub kind: LocationKind,
    #[serde(default)]
    pub source: Option<StaticSource>,
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default = "default_true")]
    pub spa_fallback: bool,
    #[serde(default)]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SslConfig {
    pub enabled: bool,
    #[serde(default)]
    pub cert_path: Option<String>,
    #[serde(default)]
    pub key_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub server_name: Option<String>,
    pub listen: u16,
    #[serde(default)]
    pub ssl: SslConfig,
    pub enabled: bool,
    #[serde(default)]
    pub locations: Vec<Location>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebsiteList {
    pub websites: Vec<Site>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_json_roundtrip_preserves_fields() {
        let site = Site {
            id: "s1".to_string(),
            name: "官网".to_string(),
            server_name: Some("www.demo.com".to_string()),
            listen: 80,
            ssl: SslConfig::default(),
            enabled: true,
            locations: vec![
                Location {
                    path: "/".to_string(),
                    kind: LocationKind::Static,
                    source: Some(StaticSource::Upload),
                    root: Some("sites-data/s1/root".to_string()),
                    spa_fallback: true,
                    target: None,
                },
                Location {
                    path: "/api".to_string(),
                    kind: LocationKind::Proxy,
                    source: None,
                    root: None,
                    spa_fallback: true,
                    target: Some("http://127.0.0.1:8080".to_string()),
                },
            ],
        };
        let json = serde_json::to_string(&site).unwrap();
        let back: Site = serde_json::from_str(&json).unwrap();
        assert_eq!(back.locations.len(), 2);
        assert_eq!(back.locations[0].kind, LocationKind::Static);
        assert_eq!(back.locations[1].target.as_deref(), Some("http://127.0.0.1:8080"));
        assert!(back.enabled);
    }

    #[test]
    fn location_defaults_spa_fallback_true_when_absent() {
        let json = r#"{"path":"/","kind":"Static"}"#;
        let loc: Location = serde_json::from_str(json).unwrap();
        assert!(loc.spa_fallback);
        assert!(loc.root.is_none());
    }
}
```

`src-tauri/src/models/mod.rs` 顶部模块声明区加入（与现有 `pub mod software;` 并列）：
```rust
pub mod website;
```

- [ ] **步骤 2：运行测试验证通过**

运行：`cd src-tauri && cargo test --lib models::website`
预期：2 个测试 PASS。

- [ ] **步骤 3：Commit**

```bash
git add src-tauri/src/models/website.rs src-tauri/src/models/mod.rs
git commit -m "feat(website): 新增站点数据模型"
```

---

## 任务 2：nginx 配置生成（纯函数）

**文件：**
- 创建：`src-tauri/src/services/website_manager/nginx_conf.rs`
- 修改：`src-tauri/src/services/mod.rs:1`（模块声明区，加 `pub mod website_manager;`）
- 创建（占位，任务 3 填充）：`src-tauri/src/services/website_manager/mod.rs`

> 注：本任务先让 `website_manager` 模块可编译。先建 `mod.rs` 仅含 `pub mod nginx_conf;`，任务 3 再补 `WebsiteManager`。

- [ ] **步骤 1：建模块声明**

`src-tauri/src/services/mod.rs` 加入（与 `pub mod software_manager;` 并列）：
```rust
pub mod website_manager;
```

`src-tauri/src/services/website_manager/mod.rs`（本任务只放这一行）：
```rust
pub mod nginx_conf;
```

- [ ] **步骤 2：编写生成函数 + 测试**

`src-tauri/src/services/website_manager/nginx_conf.rs`：
```rust
use crate::models::website::{Location, LocationKind, Site};

/// 生成单个站点的 nginx server 块（保存到 conf/sites/<id>.conf）
pub fn generate_server_block(site: &Site) -> String {
    let server_name = site
        .server_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("_");

    let mut out = String::new();
    out.push_str("server {\n");
    out.push_str(&format!("    listen {};\n", site.listen));
    out.push_str(&format!("    server_name {};\n", server_name));
    for loc in &site.locations {
        out.push_str(&generate_location(loc));
    }
    out.push_str("}\n");
    out
}

fn generate_location(loc: &Location) -> String {
    let mut s = format!("    location {} {{\n", loc.path);
    match loc.kind {
        LocationKind::Static => {
            if let Some(root) = &loc.root {
                // nginx 用正斜杠；含空格加引号，避免 Windows 路径转义问题
                s.push_str(&format!("        root \"{}\";\n", root.replace('\\', "/")));
            }
            s.push_str("        index index.html;\n");
            if loc.spa_fallback {
                s.push_str("        try_files $uri $uri/ /index.html;\n");
            }
        }
        LocationKind::Proxy => {
            if let Some(target) = &loc.target {
                s.push_str(&format!("        proxy_pass {};\n", target));
                s.push_str("        proxy_set_header Host $host;\n");
                s.push_str("        proxy_set_header X-Real-IP $remote_addr;\n");
                s.push_str("        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n");
                s.push_str("        proxy_set_header X-Forwarded-Proto $scheme;\n");
            }
        }
    }
    s.push_str("    }\n");
    s
}

/// 确保主配置 http {} 块内含 `include sites/*.conf;`，缺失则注入一次（幂等）
pub fn ensure_include(nginx_conf: &str) -> String {
    if nginx_conf.contains("sites/*.conf") {
        return nginx_conf.to_string();
    }
    // 在第一个 http { 的左花括号之后插入
    let http_idx = nginx_conf.find("http {").or_else(|| nginx_conf.find("http{"));
    if let Some(idx) = http_idx {
        if let Some(brace_off) = nginx_conf[idx..].find('{') {
            let pos = idx + brace_off + 1;
            let mut out = String::with_capacity(nginx_conf.len() + 32);
            out.push_str(&nginx_conf[..pos]);
            out.push_str("\n    include sites/*.conf;\n");
            out.push_str(&nginx_conf[pos..]);
            return out;
        }
    }
    nginx_conf.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::website::{Location, LocationKind, Site, SslConfig, StaticSource};

    fn site_with(locations: Vec<Location>, server_name: Option<&str>) -> Site {
        Site {
            id: "s1".to_string(),
            name: "t".to_string(),
            server_name: server_name.map(|s| s.to_string()),
            listen: 80,
            ssl: SslConfig::default(),
            enabled: true,
            locations,
        }
    }

    fn static_loc() -> Location {
        Location {
            path: "/".to_string(),
            kind: LocationKind::Static,
            source: Some(StaticSource::Upload),
            root: Some("C:\\nginx\\sites-data\\s1\\root".to_string()),
            spa_fallback: true,
            target: None,
        }
    }

    fn proxy_loc() -> Location {
        Location {
            path: "/api".to_string(),
            kind: LocationKind::Proxy,
            source: None,
            root: None,
            spa_fallback: false,
            target: Some("http://127.0.0.1:8080".to_string()),
        }
    }

    #[test]
    fn static_block_has_root_and_spa_fallback_with_forward_slashes() {
        let block = generate_server_block(&site_with(vec![static_loc()], Some("www.demo.com")));
        assert!(block.contains("listen 80;"));
        assert!(block.contains("server_name www.demo.com;"));
        assert!(block.contains("root \"C:/nginx/sites-data/s1/root\";"));
        assert!(block.contains("try_files $uri $uri/ /index.html;"));
        assert!(!block.contains('\\'), "路径应已转为正斜杠");
    }

    #[test]
    fn proxy_block_has_proxy_pass_and_headers() {
        let block = generate_server_block(&site_with(vec![proxy_loc()], None));
        assert!(block.contains("server_name _;"), "空域名应生成 _");
        assert!(block.contains("location /api {"));
        assert!(block.contains("proxy_pass http://127.0.0.1:8080;"));
        assert!(block.contains("proxy_set_header Host $host;"));
    }

    #[test]
    fn mixed_block_has_both_locations() {
        let block = generate_server_block(&site_with(vec![static_loc(), proxy_loc()], Some("admin.demo.com")));
        assert!(block.contains("location / {"));
        assert!(block.contains("location /api {"));
    }

    #[test]
    fn ensure_include_injects_once_and_is_idempotent() {
        let conf = "worker_processes auto;\nhttp {\n    server_tokens off;\n}\n";
        let once = ensure_include(conf);
        assert!(once.contains("include sites/*.conf;"));
        let twice = ensure_include(&once);
        assert_eq!(once, twice, "已有 include 时不应重复注入");
    }
}
```

- [ ] **步骤 3：运行测试验证通过**

运行：`cd src-tauri && cargo test --lib website_manager::nginx_conf`
预期：4 个测试 PASS。

- [ ] **步骤 4：Commit**

```bash
git add src-tauri/src/services/mod.rs src-tauri/src/services/website_manager/
git commit -m "feat(website): nginx server 块生成与 include 注入"
```

---

## 任务 3：WebsiteManager（CRUD + 持久化）

**文件：**
- 修改：`src-tauri/src/services/website_manager/mod.rs`（在 `pub mod nginx_conf;` 下追加）

- [ ] **步骤 1：编写 WebsiteManager**

`src-tauri/src/services/website_manager/mod.rs` 追加：
```rust
use std::path::PathBuf;
use std::sync::RwLock;

use anyhow::Result;

use crate::models::website::{Site, WebsiteList};
use crate::utils::paths;

pub struct WebsiteManager {
    websites: RwLock<WebsiteList>,
}

impl WebsiteManager {
    pub fn new() -> Self {
        Self {
            websites: RwLock::new(Self::load().unwrap_or_default()),
        }
    }

    fn store_path() -> PathBuf {
        paths::config_dir().join("websites.json")
    }

    fn load() -> Result<WebsiteList> {
        let p = Self::store_path();
        if !p.exists() {
            return Ok(WebsiteList::default());
        }
        Ok(serde_json::from_str(&std::fs::read_to_string(&p)?)?)
    }

    fn save_list(list: &WebsiteList) -> Result<()> {
        std::fs::write(Self::store_path(), serde_json::to_string_pretty(list)?)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<Site> {
        self.websites.read().unwrap().websites.clone()
    }

    pub fn get(&self, id: &str) -> Option<Site> {
        self.websites.read().unwrap().websites.iter().find(|s| s.id == id).cloned()
    }

    /// 按 id 更新，无则追加
    pub fn upsert(&self, site: Site) -> Result<()> {
        let mut l = self.websites.write().unwrap();
        if let Some(existing) = l.websites.iter_mut().find(|s| s.id == site.id) {
            *existing = site;
        } else {
            l.websites.push(site);
        }
        Self::save_list(&l)
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let mut l = self.websites.write().unwrap();
        l.websites.retain(|s| s.id != id);
        Self::save_list(&l)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let mut l = self.websites.write().unwrap();
        if let Some(s) = l.websites.iter_mut().find(|s| s.id == id) {
            s.enabled = enabled;
        }
        Self::save_list(&l)
    }
}

impl Default for WebsiteManager {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **步骤 2：验证编译**

运行：`cd src-tauri && cargo build --lib`
预期：编译通过（`paths::config_dir` 已存在，供 installed.json 使用）。
> ponytail: 未给 `WebsiteManager` 加文件系统单测（依赖全局 `config_dir`）；CRUD 是直读写 `websites.json` 的直白逻辑，nginx 生成逻辑已在任务 2 测。add when 需要注入路径做集成测试时。

- [ ] **步骤 3：Commit**

```bash
git add src-tauri/src/services/website_manager/mod.rs
git commit -m "feat(website): WebsiteManager 增删改查与持久化"
```

---

## 任务 4：Tauri 命令 + nginx 应用/reload + 注册

**文件：**
- 创建：`src-tauri/src/commands/website.rs`
- 修改：`src-tauri/src/commands/mod.rs`（加 `pub mod website;`）
- 修改：`src-tauri/src/lib.rs:53`（manage 区）与 `src-tauri/src/lib.rs:174`（invoke_handler 区）

- [ ] **步骤 1：编写命令**

`src-tauri/src/commands/website.rs`：
```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::State;

use crate::models::software::SoftwareStatus;
use crate::models::website::Site;
use crate::services::software_manager::SoftwareManager;
use crate::services::website_manager::{nginx_conf, WebsiteManager};
use crate::utils::archive;

/// 解析目标 nginx（软件管理里已安装的第一个 nginx 实例）
/// ponytail: 单 nginx 假设；多实例选择留待后续（Site 加 nginx_id）
fn resolve_nginx(sm: &SoftwareManager) -> Result<crate::models::software::InstalledSoftware, String> {
    sm.get_installed()
        .into_iter()
        .find(|s| s.key == "nginx")
        .ok_or_else(|| "请先在软件管理中安装 nginx".to_string())
}

#[cfg(windows)]
fn run_nginx(install_path: &Path, args: &[&str]) -> std::io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new(install_path.join("nginx.exe"))
        .args(args)
        .current_dir(install_path)
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output()
}

#[cfg(not(windows))]
fn run_nginx(install_path: &Path, args: &[&str]) -> std::io::Result<std::process::Output> {
    std::process::Command::new("nginx")
        .args(args)
        .current_dir(install_path)
        .output()
}

/// 从当前站点列表重建 conf/sites/*.conf（幂等，天然处理删除/下线）；
/// 确保主配置 include；reload=true 且 nginx 运行时先 -t 校验再 reload。
fn regenerate(sm: &SoftwareManager, wm: &WebsiteManager, reload: bool) -> Result<(), String> {
    let nginx = resolve_nginx(sm)?;
    let base = PathBuf::from(&nginx.install_path);
    let conf_dir = base.join("conf");
    let sites_dir = conf_dir.join("sites");
    std::fs::create_dir_all(&sites_dir).map_err(|e| e.to_string())?;

    // 清空旧站点 conf，从启用列表重建
    if let Ok(rd) = std::fs::read_dir(&sites_dir) {
        for entry in rd.flatten() {
            if entry.path().extension().map(|x| x == "conf").unwrap_or(false) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    for site in wm.list().into_iter().filter(|s| s.enabled) {
        let block = nginx_conf::generate_server_block(&site);
        std::fs::write(sites_dir.join(format!("{}.conf", site.id)), block)
            .map_err(|e| e.to_string())?;
    }

    // 确保主配置 include（幂等）
    let main_conf = conf_dir.join("nginx.conf");
    if let Ok(content) = std::fs::read_to_string(&main_conf) {
        let updated = nginx_conf::ensure_include(&content);
        if updated != content {
            std::fs::write(&main_conf, updated).map_err(|e| e.to_string())?;
        }
    }

    if reload && nginx.status == SoftwareStatus::Running {
        let test = run_nginx(&base, &["-t"]).map_err(|e| e.to_string())?;
        if !test.status.success() {
            return Err(format!(
                "nginx 配置校验失败：{}",
                String::from_utf8_lossy(&test.stderr)
            ));
        }
        let rl = run_nginx(&base, &["-s", "reload"]).map_err(|e| e.to_string())?;
        if !rl.status.success() {
            return Err(format!(
                "nginx reload 失败：{}",
                String::from_utf8_lossy(&rl.stderr)
            ));
        }
    }
    Ok(())
}

#[tauri::command]
pub fn list_websites(wm: State<'_, Arc<WebsiteManager>>) -> Vec<Site> {
    wm.list()
}

#[tauri::command]
pub fn save_website(
    sm: State<'_, Arc<SoftwareManager>>,
    wm: State<'_, Arc<WebsiteManager>>,
    site: Site,
    apply: bool,
) -> Result<(), String> {
    wm.upsert(site).map_err(|e| e.to_string())?;
    regenerate(&sm, &wm, apply)
}

#[tauri::command]
pub fn delete_website(
    sm: State<'_, Arc<SoftwareManager>>,
    wm: State<'_, Arc<WebsiteManager>>,
    id: String,
) -> Result<(), String> {
    wm.remove(&id).map_err(|e| e.to_string())?;
    regenerate(&sm, &wm, true)
}

#[tauri::command]
pub fn set_website_enabled(
    sm: State<'_, Arc<SoftwareManager>>,
    wm: State<'_, Arc<WebsiteManager>>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    wm.set_enabled(&id, enabled).map_err(|e| e.to_string())?;
    regenerate(&sm, &wm, true)
}

/// 上传静态包：解压 zip 到 <nginx>/sites-data/<id>/<sanitized-path>/，返回该目录路径供前端写入 location.root
#[tauri::command]
pub fn upload_site_bundle(
    sm: State<'_, Arc<SoftwareManager>>,
    id: String,
    loc_path: String,
    zip_path: String,
) -> Result<String, String> {
    let nginx = resolve_nginx(&sm)?;
    let base = PathBuf::from(&nginx.install_path);
    let sub = sanitize_seg(&loc_path);
    let dest = base.join("sites-data").join(&id).join(&sub);
    // 重新上传：先清空目标目录
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    // 复用带路径穿越防护的解压（enclosed_name 已过滤 ..）
    archive::extract_zip(Path::new(&zip_path), &dest, |_, _| {}).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().replace('\\', "/"))
}

/// 把 location path 转成安全目录段："/" -> "root"，其余非字母数字转 '_'
fn sanitize_seg(path: &str) -> String {
    let cleaned: String = path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "root".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_seg;

    #[test]
    fn sanitize_seg_maps_root_and_paths() {
        assert_eq!(sanitize_seg("/"), "root");
        assert_eq!(sanitize_seg("/api"), "api");
        assert_eq!(sanitize_seg("/a/b"), "a_b");
    }
}
```

- [ ] **步骤 2：注册模块与命令**

`src-tauri/src/commands/mod.rs` 加入：
```rust
pub mod website;
```

`src-tauri/src/lib.rs` 的 `.setup` 内、注册 `SoftwareManager` 之后（约第 55 行 `app.manage(...)` 附近）追加：
```rust
            app.manage(std::sync::Arc::new(
                crate::services::website_manager::WebsiteManager::new(),
            ));
```

`src-tauri/src/lib.rs` 的 `invoke_handler(tauri::generate_handler![...])` 列表内、`save_startup_settings,` 之后追加：
```rust
            commands::website::list_websites,
            commands::website::save_website,
            commands::website::delete_website,
            commands::website::set_website_enabled,
            commands::website::upload_site_bundle,
```

- [ ] **步骤 3：运行测试 + 编译验证**

运行：`cd src-tauri && cargo test --lib commands::website && cargo build`
预期：`sanitize_seg_maps_root_and_paths` PASS；整体编译通过。

- [ ] **步骤 4：Commit**

```bash
git add src-tauri/src/commands/website.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(website): 站点命令与 nginx 生效/reload"
```

---

## 任务 5：前端类型镜像

**文件：**
- 创建：`src/models/website.ts`

- [ ] **步骤 1：编写类型**

`src/models/website.ts`：
```typescript
// 与 Rust src-tauri/src/models/website.rs 对齐（serde 无 tag 的 unit 枚举 → 字符串）
export type LocationKind = 'Static' | 'Proxy'
export type StaticSource = 'Dir' | 'Upload'

export interface SiteLocation {
  path: string
  kind: LocationKind
  source?: StaticSource | null
  root?: string | null
  spa_fallback: boolean
  target?: string | null
}

export interface SslConfig {
  enabled: boolean
  cert_path?: string | null
  key_path?: string | null
}

export interface Site {
  id: string
  name: string
  server_name?: string | null
  listen: number
  ssl: SslConfig
  enabled: boolean
  locations: SiteLocation[]
}

/** 新建空站点（前端生成 id） */
export function emptySite(): Site {
  return {
    id: crypto.randomUUID(),
    name: '',
    server_name: '',
    listen: 80,
    ssl: { enabled: false, cert_path: null, key_path: null },
    enabled: true,
    locations: [
      { path: '/', kind: 'Static', source: 'Upload', root: '', spa_fallback: true, target: null },
    ],
  }
}
```

- [ ] **步骤 2：类型检查**

运行：`npx vue-tsc --noEmit`
预期：无错误。

- [ ] **步骤 3：Commit**

```bash
git add src/models/website.ts
git commit -m "feat(website): 前端站点类型"
```

---

## 任务 6：路由、菜单、文案

**文件：**
- 修改：`src/router/index.ts:31`（`/springboot` 路由之前插入）
- 修改：`src/layouts/Sidebar.vue:83`（management 分组 `/software` 之后插入）
- 修改：`src/locales/zh-CN.ts`、`src/locales/en-US.ts`

- [ ] **步骤 1：加路由**

`src/router/index.ts` 在 `/software` 与 `/springboot` 之间插入：
```typescript
  {
    path: '/websites',
    name: 'websites',
    component: () => import('@/modules/website-manager/pages/WebsiteListPage.vue'),
    meta: { title: 'websiteManagement' },
  },
```

- [ ] **步骤 2：加菜单项**

`src/layouts/Sidebar.vue` management 分组 items 中，`/software` 之后、`/springboot` 之前插入：
```typescript
      { path: '/websites', titleKey: 'websiteManagement', icon: 'mdi:web-box' },
```

- [ ] **步骤 3：加文案**

`src/locales/zh-CN.ts`（与 `softwareManagement` 邻近处，及新键）：
```typescript
    websiteManagement: '网站管理',
    websiteList: '站点列表',
    newSite: '新建站点',
    editSite: '编辑站点',
    siteName: '站点名称',
    serverNameLabel: '域名（可空）',
    serverNameHint: '留空则只按端口访问',
    listenPort: '监听端口',
    httpsReserved: '启用 HTTPS（证书路径…）— 即将支持',
    routeRules: '路由规则',
    addRoute: '添加路由规则',
    routePath: '路径',
    typeStatic: '静态',
    typeProxy: '反代',
    sourceDir: '指向目录',
    sourceUpload: '上传部署',
    staticRoot: '静态目录',
    uploadZip: '上传 zip',
    proxyTarget: '后端地址',
    proxyHint: '仅转发，不启停该后端',
    saveAndApply: '保存并生效',
    online: '上线',
    offline: '下线',
    running: '运行中',
    stopped: '已下线',
    noSites: '还没有站点',
    noSitesDesc: '点击"新建站点"用 nginx 部署你的页面',
```

`src/locales/en-US.ts` 对应英文：
```typescript
    websiteManagement: 'Website Management',
    websiteList: 'Sites',
    newSite: 'New Site',
    editSite: 'Edit Site',
    siteName: 'Site Name',
    serverNameLabel: 'Domain (optional)',
    serverNameHint: 'Leave empty to access by port only',
    listenPort: 'Listen Port',
    httpsReserved: 'Enable HTTPS (cert path…) — coming soon',
    routeRules: 'Route Rules',
    addRoute: 'Add Route',
    routePath: 'Path',
    typeStatic: 'Static',
    typeProxy: 'Proxy',
    sourceDir: 'Point to Dir',
    sourceUpload: 'Upload',
    staticRoot: 'Static Dir',
    uploadZip: 'Upload zip',
    proxyTarget: 'Backend',
    proxyHint: 'Forward only; backend not managed here',
    saveAndApply: 'Save & Apply',
    online: 'Online',
    offline: 'Offline',
    running: 'Running',
    stopped: 'Offline',
    noSites: 'No sites yet',
    noSitesDesc: 'Click "New Site" to deploy pages with nginx',
```

- [ ] **步骤 4：类型检查（WebsiteListPage 尚未建，暂验证 locale/router/sidebar 无语法错误）**

运行：`npx vue-tsc --noEmit`
预期：仅报 `@/modules/website-manager/pages/WebsiteListPage.vue` 找不到（任务 7 创建后消失）；无其它错误。
> 若该 import 缺失导致检查中断，可在任务 7 完成后一并跑。

- [ ] **步骤 5：Commit**

```bash
git add src/router/index.ts src/layouts/Sidebar.vue src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat(website): 路由、菜单与文案"
```

---

## 任务 7：站点列表页（卡片网格）

**文件：**
- 创建：`src/modules/website-manager/pages/WebsiteListPage.vue`

- [ ] **步骤 1：编写列表页**

`src/modules/website-manager/pages/WebsiteListPage.vue`：
```vue
<template>
  <div class="animate-fade-in">
    <PageHeader icon="mdi:web-box" :title="$t('websiteManagement')" :subtitle="$t('websiteList')">
      <template #actions>
        <button class="btn primary" @click="openNew">
          <Icon icon="mdi:plus" /> {{ $t('newSite') }}
        </button>
      </template>
    </PageHeader>

    <EmptyState
      v-if="!loading && sites.length === 0"
      icon="mdi:web-box"
      :title="$t('noSites')"
      :description="$t('noSitesDesc')"
    />

    <div v-else class="grid grid-cols-1 md:grid-cols-2 gap-4">
      <div
        v-for="s in sites"
        :key="s.id"
        class="rounded-lg border border-border bg-card p-4 shadow-card"
      >
        <div class="flex items-center justify-between mb-2">
          <div class="font-semibold flex items-center gap-2">
            <Icon icon="mdi:web" class="text-primary" /> {{ s.name }}
          </div>
          <span
            class="text-xs px-2 py-0.5 rounded-full"
            :class="s.enabled ? 'bg-green-100 text-green-700' : 'bg-muted text-muted-foreground'"
          >
            {{ s.enabled ? $t('running') : $t('stopped') }}
          </span>
        </div>
        <div class="text-sm font-mono text-sky-600 mb-2">{{ addr(s) }}</div>
        <div class="flex flex-wrap gap-1 mb-3">
          <span
            v-for="(l, i) in s.locations"
            :key="i"
            class="text-[11px] font-mono px-1.5 py-0.5 rounded"
            :class="l.kind === 'Proxy' ? 'bg-amber-100 text-amber-800' : 'bg-indigo-50 text-indigo-700'"
          >
            {{ l.path }} {{ l.kind === 'Proxy' ? '→ ' + (l.target || '') : $t('typeStatic') }}
          </span>
        </div>
        <div class="flex gap-2">
          <button class="btn" @click="openEdit(s)"><Icon icon="mdi:pencil" /> {{ $t('editSite') }}</button>
          <button class="btn" @click="toggle(s)">
            {{ s.enabled ? $t('offline') : $t('online') }}
          </button>
          <button class="btn danger" @click="remove(s)"><Icon icon="mdi:delete" /></button>
        </div>
      </div>
    </div>

    <SiteEditDialog v-if="editing" :site="editing" @close="editing = null" @saved="onSaved" />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import PageHeader from '@/components/PageHeader.vue'
import EmptyState from '@/components/EmptyState.vue'
import SiteEditDialog from '../components/SiteEditDialog.vue'
import { emptySite, type Site } from '@/models/website'

const sites = ref<Site[]>([])
const loading = ref(false)
const editing = ref<Site | null>(null)

function addr(s: Site): string {
  return s.server_name && s.server_name.trim() ? `${s.server_name}:${s.listen}` : `:${s.listen}`
}

async function load() {
  loading.value = true
  try {
    sites.value = await invoke<Site[]>('list_websites')
  } catch (e) {
    console.error(e)
  } finally {
    loading.value = false
  }
}

function openNew() {
  editing.value = emptySite()
}
function openEdit(s: Site) {
  editing.value = JSON.parse(JSON.stringify(s))
}
function onSaved() {
  editing.value = null
  load()
}

async function toggle(s: Site) {
  try {
    await invoke('set_website_enabled', { id: s.id, enabled: !s.enabled })
    load()
  } catch (e) {
    window.alert(String(e))
  }
}

async function remove(s: Site) {
  if (!confirm(`删除站点「${s.name}」？`)) return
  try {
    await invoke('delete_website', { id: s.id })
    load()
  } catch (e) {
    window.alert(String(e))
  }
}

onMounted(load)
</script>
```

- [ ] **步骤 2：类型检查**

运行：`npx vue-tsc --noEmit`
预期：仅报缺 `SiteEditDialog`（任务 8 创建）；无其它错误。

- [ ] **步骤 3：Commit**

```bash
git add src/modules/website-manager/pages/WebsiteListPage.vue
git commit -m "feat(website): 站点列表卡片页"
```

---

## 任务 8：Location 编辑器 + 站点编辑对话框

**文件：**
- 创建：`src/modules/website-manager/components/LocationEditor.vue`
- 创建：`src/modules/website-manager/components/SiteEditDialog.vue`

- [ ] **步骤 1：编写 LocationEditor**

`src/modules/website-manager/components/LocationEditor.vue`：
```vue
<template>
  <div class="space-y-2">
    <div v-for="(l, i) in model" :key="i" class="rounded-md border border-border p-3">
      <div class="flex items-center gap-2 mb-2">
        <input v-model="l.path" class="input flex-1 font-mono" :placeholder="$t('routePath')" />
        <div class="seg">
          <button :class="l.kind === 'Static' ? 'on' : 'off'" @click="l.kind = 'Static'">{{ $t('typeStatic') }}</button>
          <button :class="l.kind === 'Proxy' ? 'on' : 'off'" @click="l.kind = 'Proxy'">{{ $t('typeProxy') }}</button>
        </div>
        <button class="btn danger" @click="removeAt(i)"><Icon icon="mdi:delete" /></button>
      </div>

      <div v-if="l.kind === 'Static'" class="space-y-2">
        <div class="seg">
          <button :class="l.source === 'Dir' ? 'on' : 'off'" @click="l.source = 'Dir'">{{ $t('sourceDir') }}</button>
          <button :class="l.source === 'Upload' ? 'on' : 'off'" @click="l.source = 'Upload'">{{ $t('sourceUpload') }}</button>
        </div>
        <div class="flex gap-2">
          <input v-model="l.root" class="input flex-1 font-mono" :placeholder="$t('staticRoot')" />
          <button v-if="l.source === 'Upload'" class="btn" @click="upload(l)"><Icon icon="mdi:upload" /> {{ $t('uploadZip') }}</button>
        </div>
      </div>

      <div v-else class="space-y-1">
        <input v-model="l.target" class="input w-full font-mono" placeholder="http://127.0.0.1:8080" />
        <div class="text-[11px] text-muted-foreground">{{ $t('proxyHint') }}</div>
      </div>
    </div>

    <button class="btn w-full" @click="add"><Icon icon="mdi:plus" /> {{ $t('addRoute') }}</button>
  </div>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import type { SiteLocation } from '@/models/website'

const props = defineProps<{ modelValue: SiteLocation[]; siteId: string }>()
const emit = defineEmits<{ 'update:modelValue': [SiteLocation[]] }>()

// 直接在数组元素上双向绑定；数组引用不变，操作后 emit 通知父级
const model = props.modelValue

function add() {
  model.push({ path: '/', kind: 'Static', source: 'Upload', root: '', spa_fallback: true, target: null })
  emit('update:modelValue', model)
}
function removeAt(i: number) {
  model.splice(i, 1)
  emit('update:modelValue', model)
}

async function upload(l: SiteLocation) {
  const file = await open({ filters: [{ name: 'zip', extensions: ['zip'] }] })
  if (typeof file !== 'string') return
  try {
    l.root = await invoke<string>('upload_site_bundle', {
      id: props.siteId,
      locPath: l.path,
      zipPath: file,
    })
  } catch (e) {
    window.alert(String(e))
  }
}
</script>

<style scoped>
.seg { display: inline-flex; border: 1px solid var(--color-border); border-radius: 6px; overflow: hidden; }
.seg button { padding: 4px 10px; font-size: 12px; }
.seg .on { background: var(--color-primary); color: var(--color-primary-foreground); }
.seg .off { background: var(--color-card); color: var(--color-muted-foreground); }
.input { height: 32px; padding: 0 10px; background: var(--color-muted); border: 1px solid transparent; border-radius: 6px; color: var(--color-foreground); font-size: 13px; outline: none; }
.input:focus { border-color: var(--color-primary); background: var(--color-card); }
.btn { display: inline-flex; align-items: center; gap: 4px; height: 32px; padding: 0 10px; border-radius: 6px; border: 1px solid var(--color-border); background: var(--color-card); color: var(--color-foreground); font-size: 13px; cursor: pointer; }
.btn.danger { color: #e5484d; }
</style>
```

- [ ] **步骤 2：编写 SiteEditDialog**

`src/modules/website-manager/components/SiteEditDialog.vue`：
```vue
<template>
  <Teleport to="body">
    <div class="overlay">
      <div class="dialog">
        <div class="head">
          <div class="title"><Icon icon="mdi:web" /> {{ form.name || $t('newSite') }}</div>
          <button class="x" @click="$emit('close')"><Icon icon="mdi:close" /></button>
        </div>

        <div class="body">
          <label class="lbl">{{ $t('siteName') }}</label>
          <input v-model="form.name" class="input w-full mb-3" />

          <div class="flex gap-3 mb-3">
            <div class="flex-1">
              <label class="lbl">{{ $t('serverNameLabel') }}</label>
              <input v-model="form.server_name" class="input w-full font-mono" placeholder="app.demo.com" />
              <div class="hint">{{ $t('serverNameHint') }}</div>
            </div>
            <div style="width:130px">
              <label class="lbl">{{ $t('listenPort') }}</label>
              <input v-model.number="form.listen" type="number" class="input w-full font-mono" />
            </div>
          </div>

          <label class="flex items-center gap-2 mb-3 opacity-50 cursor-not-allowed">
            <input type="checkbox" disabled />
            <span class="text-sm">{{ $t('httpsReserved') }}</span>
          </label>

          <label class="lbl">{{ $t('routeRules') }}</label>
          <LocationEditor v-model="form.locations" :site-id="form.id" />
        </div>

        <div class="foot">
          <button class="btn" @click="$emit('close')">{{ $t('cancel') }}</button>
          <button class="btn" :disabled="saving" @click="save(false)">{{ $t('save') }}</button>
          <button class="btn primary" :disabled="saving" @click="save(true)">{{ $t('saveAndApply') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import LocationEditor from './LocationEditor.vue'
import type { Site } from '@/models/website'

const props = defineProps<{ site: Site }>()
const emit = defineEmits<{ close: []; saved: [] }>()

const form = ref<Site>(props.site)
const saving = ref(false)

async function save(apply: boolean) {
  saving.value = true
  try {
    await invoke('save_website', { site: form.value, apply })
    emit('saved')
  } catch (e) {
    window.alert(String(e))
  } finally {
    saving.value = false
  }
}
</script>

<style scoped>
.overlay { position: fixed; inset: 0; z-index: 50; display: flex; align-items: center; justify-content: center; background: oklch(0 0 0 / 0.5); backdrop-filter: blur(4px); }
.dialog { width: 640px; max-height: 90vh; overflow-y: auto; border-radius: 10px; border: 1px solid var(--color-border); background: var(--color-card); box-shadow: 0 8px 24px oklch(0 0 0 / 0.45); }
.head { display: flex; justify-content: space-between; align-items: center; padding: 14px 18px; border-bottom: 1px solid var(--color-border); }
.title { font-weight: 600; display: flex; gap: 8px; align-items: center; }
.title svg { color: var(--color-primary); }
.x { border: none; background: transparent; color: var(--color-muted-foreground); cursor: pointer; }
.body { padding: 16px 18px; }
.foot { display: flex; justify-content: flex-end; gap: 8px; padding: 12px 18px; border-top: 1px solid var(--color-border); }
.lbl { display: block; font-size: 12px; color: var(--color-muted-foreground); margin-bottom: 4px; }
.hint { font-size: 11px; color: var(--color-muted-foreground); margin-top: 3px; }
.input { height: 32px; padding: 0 10px; background: var(--color-muted); border: 1px solid transparent; border-radius: 6px; color: var(--color-foreground); font-size: 13px; outline: none; box-sizing: border-box; }
.input:focus { border-color: var(--color-primary); background: var(--color-card); }
.btn { display: inline-flex; align-items: center; gap: 6px; height: 32px; padding: 0 12px; border-radius: 6px; border: 1px solid var(--color-border); background: var(--color-card); color: var(--color-foreground); font-size: 13px; cursor: pointer; }
.btn.primary { background: var(--color-primary); color: var(--color-primary-foreground); border-color: var(--color-primary); }
.btn:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
```

- [ ] **步骤 3：类型检查通过**

运行：`npx vue-tsc --noEmit`
预期：无错误（此时所有 import 都已就位）。

- [ ] **步骤 4：Commit**

```bash
git add src/modules/website-manager/components/
git commit -m "feat(website): 站点编辑对话框与 location 编辑器"
```

---

## 任务 9：端到端冒烟验证

**文件：** 无（手动验证）

- [ ] **步骤 1：整体构建**

运行：`cd src-tauri && cargo build && cd .. && npx vue-tsc --noEmit`
预期：均通过。

- [ ] **步骤 2：手动冒烟（需已装并运行 nginx）**

运行：`npm run tauri:dev`
验证：
1. 侧边栏出现"网站管理"，位于"软件管理"与"SpringBoot"之间。
2. 新建静态站点（端口 8081，上传一个 dist.zip），保存并生效 → 浏览器访问 `http://localhost:8081` 显示页面。
3. 加一条 `/api` 反代规则 → 生效后 `/api` 转发到后端地址。
4. 下线站点 → 该 conf 移除、reload；访问失效。
5. 未装 nginx 时保存 → 弹框"请先在软件管理中安装 nginx"。

- [ ] **步骤 3：Commit（若有微调）**

```bash
git add -A
git commit -m "test(website): 端到端冒烟修正"
```

---

## 自检结论

- **规格覆盖度：** 数据模型(任务1)、nginx 生成+include(任务2)、CRUD 持久化(任务3)、命令+reload+上传(任务4)、前端类型(任务5)、路由/菜单/文案(任务6)、卡片列表(任务7)、编辑对话框+location 编辑器(任务8)、冒烟(任务9)。规格各节均有对应任务。HTTPS 按规格仅"预留"（模型有 `SslConfig`、UI 置灰）。
- **占位符扫描：** 各步骤含完整代码/命令，无 TODO。
- **类型一致性：** Rust `Site/Location/LocationKind/StaticSource` ↔ TS 同名镜像；命令名 `list_websites/save_website/delete_website/set_website_enabled/upload_site_bundle` 在后端注册与前端 invoke 处一致；`save_website(site, apply)`、`upload_site_bundle(id, locPath, zipPath)` 参数前后统一（Rust snake_case ↔ Tauri 自动转 camelCase）。
- **已知取舍（ponytail）：** 单 nginx 假设；`regenerate` 每次全量重建 sites/*.conf（幂等、简单，站点量大时可改增量）；WebsiteManager 无文件系统单测。
