# R6 网站证书与代理增强 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 增强网站管理的反向代理生成能力（upstream 负载均衡、自定义 proxy_set_header、子路径改写）并补齐 HTTPS 证书配置引导（listen 443 ssl + 80→443 跳转 + 纯 Rust 自签证书生成）。

**Architecture:** 后端在 `nginx_conf.rs` 纯函数里扩展 `generate_server_block`/`generate_location`，从 `Site`/`Location` 模型生成含 upstream/自定义头/子路径/SSL 的 nginx server 块；模型在 `models/website.rs`（Rust）与 `src/models/website.ts`（前端）同步加字段。新增 `generate_self_signed_cert` 命令用 rcgen 生成自签 PEM。前端在 `LocationEditor.vue` 加反代增强输入、`SiteEditDialog.vue` 加 SSL 区。保存仍走 `save_website` → regenerate → `nginx -t` → reload 现有链路。

**Tech Stack:** Rust (tauri 2), rcgen 0.14（自签证书）, Vue 3 + TypeScript, Pinia。测试用 Rust `#[cfg(test)]`（沿用项目现有模式）。

## Global Constraints

- 分支：`feat/roadmap2-r6`（已从 dev 创建）。
- Rust edition 2021；`tauri = "2"`（feature tray-icon）。
- 前端模型 `SiteLocation`/`SslConfig` 必须与后端 Rust 模型 `Location`/`SslConfig` 字段名一一对应（serde 直传 JSON）。
- 自签证书用纯 Rust rcgen，不依赖系统 openssl（Windows 官方 nginx 无 openssl.exe）。
- 80→443 跳转用 `return 301 https://$host$request_uri;`，不硬编码域名。
- 所有新增字段 serde `#[serde(default)]` / 前端可选（`?`），兼容旧数据。
- 文案走 i18n：`src/locales/zh-CN.ts` 与 `en-US.ts` 成对新增。
- Windows 路径：command 用 `std::os::windows::process::CommandExt` + `CREATE_NO_WINDOW`（沿用 website.rs 现有 `run_nginx` 模式）。

---

### Task 1: 模型扩展（后端 Rust + 前端 TS）

**Files:**
- Modify: `src-tauri/src/models/website.rs`
- Modify: `src/models/website.ts`

**Interfaces:**
- Produces:
  - Rust `Location` 新增字段 `upstreams: Vec<UpstreamTarget>`、`proxy_headers: Vec<ProxyHeader>`、`proxy_subpath: Option<String>`
  - Rust 新结构 `UpstreamTarget { pub addr: String }`、`ProxyHeader { pub name: String, pub value: String }`
  - TS `SiteLocation` 新增 `upstreams?: UpstreamTarget[]`、`proxy_headers?: ProxyHeader[]`、`proxy_subpath?: string | null`
  - TS 新接口 `UpstreamTarget { addr: string }`、`ProxyHeader { name: string, value: string }`

- [ ] **Step 1: 扩展 Rust 模型**

在 `src-tauri/src/models/website.rs` 的 `LocationKind` 之后、`Location` 之前加入：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamTarget {
    pub addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyHeader {
    pub name: String,
    pub value: String,
}
```

在 `Location` 结构体末尾追加字段（`target` 之后）：

```rust
    /// 多后端负载均衡目标（与 target 二选一，非空时生成 upstream 块）
    #[serde(default)]
    pub upstreams: Vec<UpstreamTarget>,
    /// 自定义 proxy_set_header（追加在默认头之后）
    #[serde(default)]
    pub proxy_headers: Vec<ProxyHeader>,
    /// 子路径改写：非空时 proxy_pass 目标拼接该子路径
    #[serde(default)]
    pub proxy_subpath: Option<String>,
```

- [ ] **Step 2: 扩展前端模型**

在 `src/models/website.ts` 中，`LocationKind` 定义后加入：

```ts
export interface UpstreamTarget {
  addr: string
}
export interface ProxyHeader {
  name: string
  value: string
}
```

在 `SiteLocation` 接口末尾（`target` 之后）加入：

```ts
  upstreams?: UpstreamTarget[] | null
  proxy_headers?: ProxyHeader[] | null
  proxy_subpath?: string | null
```

- [ ] **Step 3: 前端 `emptySite()` 默认加入空数组（保持模型一致）**

`src/models/website.ts` 中 `emptySite()` 的初始 locations 两处（Static 与 Proxy）各加：

```ts
      upstreams: [],
      proxy_headers: [],
      proxy_subpath: null,
```

- [ ] **Step 4: 类型检查**

Run: `npx vue-tsc --noEmit`
Expected: 无错误输出。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/models/website.rs src/models/website.ts
git commit -m "feat(website): 模型新增 upstream/自定义头/子路径字段"
```

---

### Task 2: nginx 反代生成增强（上游/头/子路径）+ 单测

**Files:**
- Modify: `src-tauri/src/services/website_manager/nginx_conf.rs`

**Interfaces:**
- Consumes: Task 1 的 `Location.upstreams/proxy_headers/proxy_subpath`、`UpstreamTarget`、`ProxyHeader`。
- Produces: 增强后的 `generate_server_block(site: &Site) -> String` 与 `generate_location(loc) -> String`，行为：
  - 多 upstream：server 块首部（`server {` 与第一个 location 之间）生成 `upstream <name> { server <addr>; ... }`，location 用 `proxy_pass http://<name>;`
  - upstream 名 = `site_{short8}_{sanatized_path}`，避免含 `_` 冲突用 `_` 分隔
  - 单后端 + `proxy_subpath`：`proxy_pass {target}{subpath}`
  - 多 upstream + `proxy_subpath`：`proxy_pass http://{name}{subpath}`
  - 自定义头：默认 proxy 头之后追加 `proxy_set_header {name} {value};`

- [ ] **Step 1: 写失败单测**

在 `nginx_conf.rs` 末尾追加（`Site`/`Location` 已在文件顶部 use）：

```rust
#[cfg(test)]
mod tests {
    use crate::models::website::{Location, LocationKind, Site, StaticSource};

    fn base_site() -> Site {
        Site {
            id: "abcdef1234567890".into(),
            name: "demo".into(),
            server_name: Some("demo.local".into()),
            listen: 8080,
            ssl: Default::default(),
            enabled: true,
            locations: vec![],
            custom_conf: false,
        }
    }

    fn proxy_loc() -> Location {
        Location {
            path: "/api".into(),
            kind: LocationKind::Proxy,
            source: None,
            root: None,
            spa_fallback: false,
            target: None,
            upstreams: vec![crate::models::website::UpstreamTarget { addr: "127.0.0.1:8081".into() }],
            proxy_headers: vec![crate::models::website::ProxyHeader { name: "X-Auth".into(), value: "token".into() }],
            proxy_subpath: Some("/upstream/api".into()),
        }
    }

    #[test]
    fn multiple_upstream_generates_upstream_block() {
        let mut s = base_site();
        s.locations = vec![Location {
            upstreams: vec![
                crate::models::website::UpstreamTarget { addr: "127.0.0.1:8081".into() },
                crate::models::website::UpstreamTarget { addr: "127.0.0.1:8082".into() },
            ],
            target: None,
            ..proxy_loc()
        }];
        let out = super::generate_server_block(&s);
        assert!(out.contains("upstream site_abcdef12_api {"), "{}", out);
        assert!(out.contains("server 127.0.0.1:8081;"));
        assert!(out.contains("server 127.0.0.1:8082;"));
        assert!(out.contains("proxy_pass http://site_abcdef12_api;"));
    }

    #[test]
    fn subpath_and_headers_applied() {
        let mut s = base_site();
        s.locations = vec![proxy_loc()];
        let out = super::generate_server_block(&s);
        assert!(out.contains("proxy_pass http://site_abcdef12_api/upstream/api;"), "{}", out);
        assert!(out.contains("proxy_set_header X-Auth token;"));
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test --lib nginx_conf::tests -- --nocapture`
Expected: FAIL（编译错误，`upstreams`/`proxy_headers`/`proxy_subpath` 字段已在 Task 1 存在但 `generate_location` 尚未生成对应输出）。

- [ ] **Step 3: 实现生成增强**

重写 `generate_server_block` 以收集 upstream 块，并改造 `generate_location`。替换函数体：

```rust
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

    // 收集所有 upstream 块（多后端 location）
    let short_id: String = site.id.chars().take(8).collect();
    let mut emitted_upstreams: Vec<String> = Vec::new();
    for loc in &site.locations {
        if loc.kind == LocationKind::Proxy && loc.upstreams.len() > 1 {
            let name = format!("site_{}_{}", short_id, sanitize_path(&loc.path));
            if !emitted_upstreams.contains(&name) {
                let mut b = format!("upstream {} {{\n", name);
                for t in &loc.upstreams {
                    b.push_str(&format!("    server {};\n", t.addr));
                }
                b.push_str("}\n");
                out.push_str(&b);
                emitted_upstreams.push(name);
            }
        }
    }

    for loc in &site.locations {
        out.push_str(&generate_location(loc));
    }
    out.push_str("}\n");
    out
}

fn sanitize_path(path: &str) -> String {
    let s: String = path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if s.is_empty() { "root".to_string() } else { s }
}
```

改造 `generate_location` 的 Proxy 分支（在 `LocationKind::Proxy` 的 match 内替换为），并将 `generate_location` 签名改为 `fn generate_location(loc: &Location, short_id: &str) -> String`：

```rust
        LocationKind::Proxy => {
            let subpath = loc
                .proxy_subpath
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let proxy_target: Option<String> = if loc.upstreams.len() > 1 {
                Some(format!("http://site_{}_{}", short_id, sanitize_path(&loc.path)))
            } else {
                loc.target.clone()
            };
            if let Some(base) = proxy_target {
                let base = match subpath {
                    Some(sp) => format!("{}{}", base.trim_end_matches('/'), sp),
                    None => base,
                };
                s.push_str(&format!("        proxy_pass {};\n", base));
                s.push_str("        proxy_http_version 1.1;\n");
                s.push_str("        proxy_set_header Host $host;\n");
                s.push_str("        proxy_set_header X-Real-IP $remote_addr;\n");
                s.push_str("        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n");
                s.push_str("        proxy_set_header X-Forwarded-Proto $scheme;\n");
                s.push_str("        proxy_set_header Upgrade $http_upgrade;\n");
                s.push_str("        proxy_set_header Connection $connection_upgrade;\n");
                for h in &loc.proxy_headers {
                    if !h.name.trim().is_empty() {
                        s.push_str(&format!("        proxy_set_header {} {};\n", h.name, h.value));
                    }
                }
            }
        }
```

`generate_server_block` 中 `generate_location` 的调用改为 `out.push_str(&generate_location(loc, &short_id));`；其函数签名同步改为 `fn generate_location(loc: &Location, short_id: &str) -> String`。

- [ ] **Step 4: 运行测试确认通过**

Run: `cd src-tauri && cargo test --lib nginx_conf::tests -- --nocapture`
Expected: PASS（两个测试通过）。

- [ ] **Step 5: 全量后端测试**

Run: `cd src-tauri && cargo test`
Expected: 全部通过（原有测试不受影响）。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/services/website_manager/nginx_conf.rs
git commit -m "feat(website): nginx 反代支持 upstream 负载均衡/自定义头/子路径改写"
```

---

### Task 3: nginx SSL 生成（listen 443 ssl + 80→443 跳转）+ 单测

**Files:**
- Modify: `src-tauri/src/services/website_manager/nginx_conf.rs`

**Interfaces:**
- Consumes: `site.ssl: SslConfig`（已有字段 `enabled/cert_path/key_path`）。
- Produces: `generate_server_block` 在 `ssl.enabled` 时：
  - 主 server 块 `listen {port} ssl;` + `ssl_certificate`/`ssl_certificate_key` + 基础 ssl 参数
  - 额外生成 80 端口跳转 server 块 `return 301 https://$host$request_uri;`

- [ ] **Step 1: 写失败单测**

在 `nginx_conf.rs` 的 `mod tests` 内追加：

```rust
    #[test]
    fn ssl_enabled_adds_ssl_listen_and_redirect() {
        let mut s = base_site();
        s.listen = 443;
        s.ssl = crate::models::website::SslConfig {
            enabled: true,
            cert_path: Some("sites-data/certs/demo.crt".into()),
            key_path: Some("sites-data/certs/demo.key".into()),
        };
        let out = super::generate_server_block(&s);
        assert!(out.contains("listen 443 ssl;"), "{}", out);
        assert!(out.contains("ssl_certificate sites-data/certs/demo.crt;"));
        assert!(out.contains("ssl_certificate_key sites-data/certs/demo.key;"));
        assert!(out.contains("listen 80;"));
        assert!(out.contains("return 301 https://$host$request_uri;"));
    }

    #[test]
    fn ssl_disabled_no_redirect_block() {
        let mut s = base_site();
        s.ssl = Default::default();
        let out = super::generate_server_block(&s);
        assert!(!out.contains(" ssl;"), "{}", out);
        assert!(!out.contains("return 301"));
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test --lib nginx_conf::tests`
Expected: FAIL（当前输出无 ssl 相关）。

- [ ] **Step 3: 实现 SSL 生成**

改造 `generate_server_block`。在 `server_name` 输出之后、upstream 收集之前，处理 listen 与 ssl：

```rust
    // SSL 主块端口；启用 ssl 时用 ssl 指令
    if site.ssl.enabled {
        out.push_str(&format!("    listen {} ssl;\n", site.listen));
        if let Some(c) = &site.ssl.cert_path {
            out.push_str(&format!("    ssl_certificate {};\n", c));
        }
        if let Some(k) = &site.ssl.key_path {
            out.push_str(&format!("    ssl_certificate_key {};\n", k));
        }
        out.push_str("    ssl_protocols TLSv1.2 TLSv1.3;\n");
        out.push_str("    ssl_session_cache shared:SSL:10m;\n");
    } else {
        out.push_str(&format!("    listen {};\n", site.listen));
    }
```

删除原先的 `out.push_str(&format!("    listen {};\n", site.listen));` 一行。

在 `generate_server_block` 全部 server 块输出结束后（`out.push_str("}\n");` 之后）、`return out` 之前，追加 80→443 跳转块：

```rust
    // 启用 SSL 时追加 80 → 443 跳转块
    if site.ssl.enabled {
        out.push_str("server {\n");
        out.push_str("    listen 80;\n");
        out.push_str(&format!("    server_name {};\n", server_name));
        out.push_str("    return 301 https://$host$request_uri;\n");
        out.push_str("}\n");
    }
```

> 港口冲突：若 80 被其他站点占用，regenerate 的 `nginx -t` 会报错提示（既有错误处理）。

- [ ] **Step 4: 运行测试确认通过**

Run: `cd src-tauri && cargo test --lib nginx_conf::tests -- --nocapture`
Expected: PASS（新 2 个 + 既有全部通过）。

- [ ] **Step 5: 全量后端测试**

Run: `cd src-tauri && cargo test`
Expected: 全部通过。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/services/website_manager/nginx_conf.rs
git commit -m "feat(website): nginx 支持 listen ssl 与 80→443 跳转"
```

---

### Task 4: 自签证书命令（rcgen）

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/commands/website.rs`
- Modify: `src-tauri/src/lib.rs`（invoke_handler 注册）

**Interfaces:**
- Consumes: `crate::utils::paths`（`data_dir` 或 `tmp_dir` 用于定位 sites-data/certs）；`resolve_nginx` 获取 `install_path`（现有函数）。
- Produces: `#[tauri::command] pub fn generate_self_signed_cert(app: AppHandle, sm: State<Arc<SoftwareManager>>, domain: String) -> Result<CertPaths, String>`，返回 `{ cert_path, key_path }`（可写 JSON 结构）。证书/私钥写入 `{install_path}/sites-data/certs/{domain}.crt` / `.key`。

- [ ] **Step 1: 添加 rcgen 依赖**

`src-tauri/Cargo.toml` 的 `[dependencies]` 追加：

```toml
rcgen = "0.14"
```

Run: `cd src-tauri && cargo build`
Expected: 依赖编译成功。

- [ ] **Step 2: 实现命令**

在 `src-tauri/src/commands/website.rs` 末尾追加：

```rust
#[derive(serde::Serialize)]
pub struct CertPaths {
    pub cert_path: String,
    pub key_path: String,
}

/// 用 rcgen 生成自签证书（CN/SAN=domain），写入 sites-data/certs/ 并返回路径。
/// 纯 Rust 实现，避免依赖系统 openssl（Windows 官方 nginx 无 openssl.exe）。
pub fn generate_self_signed_cert(
    sm: State<'_, Arc<SoftwareManager>>,
    domain: String,
) -> Result<CertPaths, String> {
    use std::path::Path;

    let domain = domain.trim();
    if domain.is_empty() {
        return Err("域名不能为空".to_string());
    }
    let nginx = resolve_nginx(&sm)?;
    let cert_dir = Path::new(&nginx.install_path).join("sites-data").join("certs");
    std::fs::create_dir_all(&cert_dir).map_err(|e| e.to_string())?;

    let CertifiedKey { cert, signing_key } = rcgen::generate_simple_self_signed(vec![domain.to_string()])
        .map_err(|e| format!("证书生成失败：{}", e))?;

    let cert_path = cert_dir.join(format!("{}.crt", domain));
    let key_path = cert_dir.join(format!("{}.key", domain));
    std::fs::write(&cert_path, cert.pem()).map_err(|e| e.to_string())?;
    std::fs::write(&key_path, signing_key.serialize_pem()).map_err(|e| e.to_string())?;

    Ok(CertPaths {
        cert_path: cert_path.to_string_lossy().to_string(),
        key_path: key_path.to_string_lossy().to_string(),
    })
}
```

顶部 import 追加：

```rust
use rcgen::CertifiedKey;
```

- [ ] **Step 3: 注册命令**

`src-tauri/src/lib.rs` 的 `invoke_handler` 数组内追加：

```rust
            commands::website::generate_self_signed_cert,
```

- [ ] **Step 4: 编译**

Run: `cd src-tauri && cargo build`
Expected: 编译通过。

- [ ] **Step 5: 集成验证（生成 PEM 可解析）**

在 `commands/website.rs` 追加 `#[cfg(test)] mod tests`：

```rust
#[cfg(test)]
mod tests {
    use rcgen::CertifiedKey;

    #[test]
    fn self_signed_pem_is_valid() {
        let CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_pem = cert.pem();
        let key_pem = signing_key.serialize_pem();
        assert!(cert_pem.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(key_pem.starts_with("-----BEGIN PRIVATE KEY-----") || key_pem.starts_with("-----BEGIN EC PRIVATE KEY-----"));
        // PEM 可被 rcgen 反解析（roundtrip 完整性）
        let parsed = rcgen::CertificateParams::from_ca_cert_pem(&cert_pem);
        assert!(parsed.is_ok());
    }
}
```

Run: `cd src-tauri && cargo test --lib commands::website::tests`
Expected: PASS。

- [ ] **Step 6: 全量后端测试 + Commit**

Run: `cd src-tauri && cargo test`
Expected: 全部通过。

```bash
git add src-tauri/Cargo.toml src-tauri/src/commands/website.rs src-tauri/src/lib.rs
git commit -m "feat(website): 新增 rcgen 自签证书生成命令"
```

---

### Task 5: 前端反代增强 UI（LocationEditor）

**Files:**
- Modify: `src/modules/website-manager/components/LocationEditor.vue`

**Interfaces:**
- Consumes: Task 1 TS 模型 `UpstreamTarget`/`ProxyHeader`/`SiteLocation.upstreams/proxy_headers/proxy_subpath`。
- Produces: Proxy location 编辑区新增：
  - 子路径输入框（绑 `l.proxy_subpath`）
  - 后端列表：单后端 target 输入 + 多后端 upstream 行（addr 输入、删除、添加）
  - 自定义头键值对行（name+value、删除、添加）

- [ ] **Step 1: Proxy 编辑区扩展**

`LocationEditor.vue` 中 `<div v-else class="space-y-1">`（Proxy 分支，第 38-41 行）替换为：

```html
<div v-else class="space-y-2">
  <input v-model="l.target" class="input w-full font-mono" placeholder="http://127.0.0.1:8080" />

  <div class="text-[11px] text-muted-foreground">{{ $t('proxyUpstreams') }}</div>
  <div v-for="(u, ui) in l.upstreams || []" :key="ui" class="flex gap-2">
    <input v-model="u.addr" class="input flex-1 font-mono" placeholder="127.0.0.1:8081" />
    <button class="btn danger" @click="removeUpstream(l, ui)"><Icon icon="mdi:delete" /></button>
  </div>
  <button class="btn w-full" @click="addUpstream(l)"><Icon icon="mdi:plus" /> {{ $t('addUpstream') }}</button>

  <div class="text-[11px] text-muted-foreground">{{ $t('proxySubpath') }}</div>
  <input v-model="l.proxy_subpath" class="input w-full font-mono" placeholder="/api" />

  <div class="text-[11px] text-muted-foreground">{{ $t('proxyHeaders') }}</div>
  <div v-for="(h, hi) in l.proxy_headers || []" :key="hi" class="flex gap-2">
    <input v-model="h.name" class="input flex-1 font-mono" placeholder="X-Custom-Header" />
    <input v-model="h.value" class="input flex-1 font-mono" placeholder="value" />
    <button class="btn danger" @click="removeHeader(l, hi)"><Icon icon="mdi:delete" /></button>
  </div>
  <button class="btn w-full" @click="addHeader(l)"><Icon icon="mdi:plus" /> {{ $t('addHeader') }}</button>

  <div class="text-[11px] text-muted-foreground">{{ $t('proxyHint') }}</div>
</div>
```

- [ ] **Step 2: script 加辅助函数**

`<script setup>` 中 `add()`/`removeAt()` 之后追加：

```ts
import type { SiteLocation } from '@/models/website'
// （顶部已 import SiteLocation；若未导入 UpstreamTarget/ProxyHeader 类型，在此补）

function ensureProxyArrays(l: SiteLocation) {
  if (!l.upstreams) l.upstreams = []
  if (!l.proxy_headers) l.proxy_headers = []
  if (!l.proxy_subpath) l.proxy_subpath = null
}
function addUpstream(l: SiteLocation) {
  ensureProxyArrays(l)
  l.upstreams!.push({ addr: '' })
  emit('update:modelValue', model)
}
function removeUpstream(l: SiteLocation, i: number) {
  l.upstreams?.splice(i, 1)
  emit('update:modelValue', model)
}
function addHeader(l: SiteLocation) {
  ensureProxyArrays(l)
  l.proxy_headers!.push({ name: '', value: '' })
  emit('update:modelValue', model)
}
function removeHeader(l: SiteLocation, i: number) {
  l.proxy_headers?.splice(i, 1)
  emit('update:modelValue', model)
}
```

> `add()` 已在 `model.push(...)` 时无 upstream/headers 字段——因 serde 已 default、TS 可选，直接运行即可；new location 对象建议补充 `upstreams: []`、`proxy_headers: []`、`proxy_subpath: null`。

- [ ] **Step 3: 类型检查**

Run: `npx vue-tsc --noEmit`
Expected: 无错误。

- [ ] **Step 4: Commit**

```bash
git add src/modules/website-manager/components/LocationEditor.vue
git commit -m "feat(website): LocationEditor 支持 upstream/自定义头/子路径编辑"
```

---

### Task 6: 前端 SSL 区 UI（SiteEditDialog）

**Files:**
- Modify: `src/modules/website-manager/components/SiteEditDialog.vue`

**Interfaces:**
- Consumes: `form.ssl: SslConfig`（enabled/cert_path/key_path）；invoke `generate_self_signed_cert`。
- Produces: SSL 区 UI（启用 https 开关 + cert/key 只读展示 + 「生成自签证书」按钮 + 域名输入）。

- [ ] **Step 1: 读当前 SiteEditDialog template 的端口区**

在 `SiteEditDialog.vue` 的 listen port 输入（约 33-35 行）之后追加 SSL 区块：

```html
      <label class="lbl flex items-center gap-2 mt-2">
        <input v-model="form.ssl.enabled" type="checkbox" /> {{ $t('enableHttps') }}
      </label>
      <div v-if="form.ssl.enabled" class="space-y-2">
        <label class="lbl">{{ $t('sslDomain') }}</label>
        <input v-model="sslDomain" class="input w-full font-mono" placeholder="example.com" />
        <button class="btn primary w-full" :disabled="genCertBusy" @click="genSelfSigned">
          <Icon :icon="genCertBusy ? 'mdi:loading' : 'mdi:shield-key'" class="spinning" v-if="genCertBusy" />
          <Icon v-else icon="mdi:shield-key" /> {{ $t('generateCert') }}
        </button>
        <label class="lbl">{{ $t('sslCertPath') }}</label>
        <div class="input readonly font-mono">{{ form.ssl.cert_path || '-' }}</div>
        <label class="lbl">{{ $t('sslKeyPath') }}</label>
        <div class="input readonly font-mono">{{ form.ssl.key_path || '-' }}</div>
      </div>
```

- [ ] **Step 2: script 加状态与生成函数**

`<script setup>` 中追加：

```ts
import { ref, watch, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
// （顶部通常已 import ref/onMounted/invoke，按需合并）

const sslDomain = ref<string>('')
const genCertBusy = ref(false)

async function genSelfSigned() {
  const d = sslDomain.value.trim()
  if (!d) return
  genCertBusy.value = true
  try {
    const paths = await invoke<{ cert_path: string; key_path: string }>('generate_self_signed_cert', {
      domain: d,
    })
    form.value.ssl.cert_path = paths.cert_path
    form.value.ssl.key_path = paths.key_path
  } catch (e) {
    console.error('generate cert failed:', e)
  } finally {
    genCertBusy.value = false
  }
}
```

> `form` 是 `ref<Site>`（现有变量），访问 `form.value.ssl`。若当前 script 用 `reactive(form)` 则改为 `form.ssl`。实现时按该文件现状（line 103 `const props = withDefaults(...)`，`form` 定义方式见 `SiteEditDialog.vue` 现有 `const form = reactive(...)` 或 `ref`）对应调整访问方式。

- [ ] **Step 3: 类型检查**

Run: `npx vue-tsc --noEmit`
Expected: 无错误。

- [ ] **Step 4: Commit**

```bash
git add src/modules/website-manager/components/SiteEditDialog.vue
git commit -m "feat(website): SiteEditDialog 新增 HTTPS 配置与自签证书生成"
```

---

### Task 7: i18n 文案

**Files:**
- Modify: `src/locales/zh-CN.ts`
- Modify: `src/locales/en-US.ts`

**Interfaces:**
- Produces keys（成对）：
  - `proxyUpstreams`：后端负载均衡
  - `addUpstream`：添加后端
  - `proxySubpath`：子路径
  - `proxyHeaders`：自定义请求头
  - `addHeader`：添加请求头
  - `enableHttps`：启用 HTTPS
  - `sslDomain`：证书域名
  - `generateCert`：生成自签证书
  - `sslCertPath` / `sslKeyPath`：证书路径 / 私钥路径

- [ ] **Step 1: zh-CN.ts 追加**

在合适位置（如 `site`/`listenPort` 相关 key 附近）追加：

```ts
  enableHttps: '启用 HTTPS',
  sslDomain: '证书域名（SAN）',
  generateCert: '生成自签证书',
  sslCertPath: '证书路径 (cert)',
  sslKeyPath: '私钥路径 (key)',
  proxyUpstreams: '后端负载均衡（多个视为一组 upstream）',
  addUpstream: '添加后端',
  proxySubpath: '子路径改写（可选）',
  proxyHeaders: '自定义请求头',
  addHeader: '添加请求头',
```

- [ ] **Step 2: en-US.ts 追加**

```ts
  enableHttps: 'Enable HTTPS',
  sslDomain: 'Certificate domain (SAN)',
  generateCert: 'Generate self-signed cert',
  sslCertPath: 'Certificate path (cert)',
  sslKeyPath: 'Private key path (key)',
  proxyUpstreams: 'Backend load balancing (multiple = one upstream group)',
  addUpstream: 'Add backend',
  proxySubpath: 'Sub-path rewrite (optional)',
  proxyHeaders: 'Custom request headers',
  addHeader: 'Add header',
```

- [ ] **Step 3: 类型检查 + Commit**

Run: `npx vue-tsc --noEmit`
Expected: 无错误。

```bash
git add src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat(website): 补充 HTTPS/代理增强 i18n 文案"
```

---

### Task 8: 全量验证

**Files:** 无代码改动。

- [ ] **Step 1: 后端全量测试**

Run: `cd src-tauri && cargo test`
Expected: 全部通过。

- [ ] **Step 2: 前端构建**

Run: `npm run build`
Expected: 构建成功（无 TS 错误）。

- [ ] **Step 3: 实机验证（需 GUI）**

Run: `npm run tauri dev`
人工验证：
1. 网站编辑 → Proxy location：配置 target 单后端 → 保存 → conf 含单 proxy_pass；配置多个 upstream → conf 生成 upstream 块；配子路径/自定义头 → 生成对应行。
2. SiteEditDialog：勾选「启用 HTTPS」→ 填域名 → 点「生成自签证书」→ cert/key 路径回填 → 保存 → conf 含 `listen 443 ssl;` + 80 跳转块。
3. nginx reload 无报错。

- [ ] **Step 4: 收尾**

按项目流程：合并 `feat/roadmap2-r6` → dev → 推送 → 删分支。更新 `docs/superpowers/specs/2026-08-21-website-ssl-proxy-design.md` 完成状态、更新 memory `roadmap-2-progress.md`。
