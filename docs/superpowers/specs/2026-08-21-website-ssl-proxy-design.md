# R6 网站证书与代理增强 设计文档

日期：2026-08-21
分支：feat/roadmap2-r6

## 背景

网站管理现有能力：`Site` 模型（含 `ssl: SslConfig` 但未接入 conf 生成）、`nginx_conf.rs` 已能生成 proxy/static server 块并支持 conf 查看/编辑、保存时 regenerate 到 `conf/sites/*.conf` 并 reload。

R6 补齐两方向：
1. **反向代理生成增强**：upstream 负载均衡、自定义 `proxy_set_header`、子路径改写（用户确认「全部都要」）。
2. **HTTPS 证书引导**：`listen 443 ssl` 生成、cert/key 路径配置、自签证书一键生成（用户确认 80→443 自动跳转 + 用纯 Rust `rcgen` 生成，规避 Windows 无 openssl CLI 问题）。

## 现状核对

- `models/website.rs`：`SslConfig { enabled, cert_path, key_path }` 字段已存在；`Site.listen` 为单个 `u16`；`Location { path, kind, root, target, ... }` 无 upstream/自定义头/子路径字段。
- `nginx_conf.rs`：`generate_server_block` / `generate_location` 仅支持单后端 `proxy_pass` 与静态 root，不含 upstream、自定义头、子路径、SSL。
- 命令流：`save_website` → `regenerate` → `sync_site_files` 用 `generate_server_block` 写 conf → `nginx -t` 校验 → reload。
- **关键约束**：项目安装 Windows 官方 nginx（nginx.org zip），该包**不含 openssl.exe**，只内置 TLS 运行库。故自签证书不能用「调系统 openssl」方案，需纯 Rust。

## 一、反向代理增强

### 数据模型扩展（`models/website.rs`）

`Location` 新增字段（均 serde default，兼容旧数据）：

```rust
#[serde(default)]
pub upstreams: Vec<UpstreamTarget>,   // 多后端负载均衡
#[serde(default)]
pub proxy_headers: Vec<ProxyHeader>,  // 自定义 proxy_set_header
#[serde(default)]
pub proxy_subpath: Option<String>,    // 子路径改写

#[derive(Serialize, Deserialize)]
pub struct UpstreamTarget { pub addr: String }  // 如 "127.0.0.1:8080"

#[derive(Serialize, Deserialize)]
pub struct ProxyHeader { pub name: String, pub value: String }
```

### 生成策略（`nginx_conf.rs`）

`generate_location` Proxy 分支增强：

- **单后端**（`target` 有值、upstreams 空）：维持现状 `proxy_pass <target>;`。
- **多后端**（`upstreams` 非空）：在 server 块内、location 之前生成

  ```
  upstream site_<shortid>_<path> {
      server 127.0.0.1:8080;
      server 127.0.0.1:8090;
  }
  ```
  location 用 `proxy_pass http://site_<shortid>_<path>;`。upstream 名须全局唯一，用 站点 short_id + 路径 sanatize 拼接。
  > ponytail: 简单轮询，nginx 默认 round-robin；不做权重/健康检查，需时后续加。

- **子路径改写**：`proxy_subpath` 有值时，`proxy_pass` 目标拼接子路径（nginx 的 proxy_pass 带 URI 即改写），例 `proxy_pass http://backend:/api;`（多后端 → `http://<upstream名>/api`）。
- **自定义头**：`proxy_headers` 每项追加 `proxy_set_header <name> <value>;`，追加在默认头之后。

`generate_server_block` 返回的 server 块需在 `}` 之前收集所有 upstream 块（扫描 locations）。

### 前端

`LocationEditor.vue` 增加：
- 后端列表：单后端（现有 target 输入）+「添加后端」→ upstream 行列表（addr 输入、删除）
- 自定义头：键值对行编辑（name + value、删除、添加）
- 子路径：文本输入

交互参照现有 Location 编辑模式。数据保存仍走 `save_website` 整站保存。

## 二、HTTPS 证书引导

### 生成（`nginx_conf.rs`）

`generate_server_block` 支持 `site.ssl.enabled`：

- 主 server 块：`listen <site.listen> ssl;` + `server_name` + `ssl_certificate <cert_path>;` + `ssl_certificate_key <key_path>;` + 常用 ssl 参数（`ssl_protocols TLSv1.2 TLSv1.3; ssl_session_cache shared:SSL:10m;`）。（Windows nginx 默认编译含 `http_ssl_module`，可用。）
- **80→443 跳转块**（用户选定）：额外生成
  ```
  server {
      listen 80;
      server_name <server_name>;
      return 301 https://$host$request_uri;
  }
  ```
  > 采用 `return 301 https://$host$request_uri;` 避免硬编码域名，SSL listen 端口由用户在 `Site.listen` 配置（通常 443）。80 被其他站点占用时 `nginx -t` 会拦截报错提示。

### 自签证书命令（新增）

- 新增 `commands/website.rs`：`generate_self_signed_cert(domain: String) -> Result<CertPaths, String>`
- 用 `rcgen` crate 生成自签 X.509 证书（CN=domain，SAN=domain），导出证书 PEM + 私钥 PEM
- 写入 `sites-data/certs/<domain>.crt` / `<domain>.key`，返回两条路径
- CertificateSigningParams：`KeyPair::generate()` + `params.subject_alt_names`

### 前端

`SiteEditDialog.vue` 增加 SSL 区：
- 「启用 HTTPS」开关（绑 `site.ssl.enabled`）
- cert 路径、key 路径输入（或只读展示自签结果）
- 「生成自签证书」按钮：填域名 → 调 `generate_self_signed_cert` → 成功回填 cert/key 路径
- 保存后走 `save_website` 正常 regenerate

## 错误处理

- 保留现有 `regenerate` 的 `nginx -t` 校验 + reload（SSL/upstream 配置错误由 nginx 报错并回滚提示）。
- 自签证书生成失败（rcgen 出错 / 写盘失败）返回用户可读中文错误。

## 测试

- `nginx_conf.rs` 纯函数单测：
  - 多 upstream 生成 upstream 块 + proxy_pass 指向 upstream 名
  - 自定义头透传
  - 子路径改写拼接
  - ssl.enabled 生成 listen 443 ssl + cert 指令 + 80 跳转块
  - 兼容：无 upstream/无 ssl 的旧结构输出不变
- rcgen 自签：生成产物为合法 PEM（可解析、含 SAN）——在命令侧做集成验证或最小单测。

## 不做（YAGNI）

- SSL 证书自动续期（acme/Let's Encrypt）——需外网/域名验证，留待后续。
- upstream 权重/主动健康检查——nginx 默认轮询已够。
- 多 nginx 实例选择（Site.nginx_id）——沿用现有「单 nginx」假设。

## 涉及文件

- `src-tauri/src/models/website.rs`（模型扩展）
- `src-tauri/src/services/website_manager/nginx_conf.rs`（生成增强 + SSL）
- `src-tauri/src/commands/website.rs`（自签证书命令）
- `src-tauri/Cargo.toml`（加 `rcgen`）
- `src/modules/website-manager/components/LocationEditor.vue`（反代增强 UI）
- `src/modules/website-manager/components/SiteEditDialog.vue`（SSL 区 UI）
- `src/locales/zh-CN.ts` / `en-US.ts`（文案）
