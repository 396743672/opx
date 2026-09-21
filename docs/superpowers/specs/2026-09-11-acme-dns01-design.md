# 网站证书自动化（ACME / Let's Encrypt，DNS-01）设计

> 状态：已确认设计
> 日期：2026-09-11

## 背景与现状

- 网站管理已支持 SSL：`SslConfig { enabled, cert_path, key_path }`（`src-tauri/src/models/website.rs`），证书路径写入 `sites-data/certs/<domain>.{crt,key}`。
- 目前**只能用自签证书**：`commands/website.rs::generate_self_signed_cert` 用 `rcgen` 纯 Rust 生成（规避 Windows nginx 包无 openssl CLI）。
- 全仓**无任何 ACME 能力**；证书到期需人工处理。
- 已确认依赖：`reqwest`（json）、`tokio`、`sha2`、`chrono`、`serde_json` 已在；rustls/aws-lc-rs 已在依赖树中（reqwest 0.13 默认）。

## 已确认决策

1. **验证方式 DNS-01**（不做 HTTP-01）。
2. **v1 仅支持 Cloudflare**，但以 trait 抽象 DNS 服务商，预留扩展。
3. **凭据存 `settings.json` 明文**，UI 标注风险（不接入 keyring）。
4. **手动申请 + 内置自动续期**。
5. **v1 仅对 `server_name` 精确域名签发**（不做通配符）。

## 设计

### 1. 依赖与会话

- 新增依赖：`instant-acme`（异步纯 Rust ACME 客户端，复用已在树中的 rustls/aws-lc-rs）与 `base64`。
- ACME 账户凭据持久化到 `<data_dir>/acme-account.json`（首次注册后复用，避免重复注册触发 LE 限流）。

### 2. DNS 服务商抽象（可扩展）

`src-tauri/src/services/acme/dns/mod.rs`：

```rust
pub trait DnsProvider: Send + Sync {
    fn id(&self) -> &str;
    /// 找到 domain 所属 zone（通常为注册域名），用于拼接记录全名。
    async fn find_zone(&self, domain: &str) -> anyhow::Result<String>;
    /// 在 zone 内创建 TXT 记录（fqdn 为完整记录名，如 `_acme-challenge.example.com`）。
    async fn create_txt(&self, fqdn: &str, value: &str) -> anyhow::Result<()>;
    /// 删除指定 TXT 记录（尽力而为）。
    async fn delete_txt(&self, fqdn: &str, value: &str) -> anyhow::Result<()>;
}

/// 按 id 取服务商实现；v1 仅 "cloudflare"。
pub fn provider_for(id: &str, token: &str) -> Option<Box<dyn DnsProvider>>;
```

`src-tauri/src/services/acme/dns/cloudflare.rs`：`reqwest` + Bearer Token：
- `find_zone`：`GET /client/v4/zones` 取账户全部 zone，对 `domain` 做**最长后缀匹配**（如 `a.b.example.com` → `example.com`）。
- `create_txt`：`POST /client/v4/zones/{zone_id}/dns_records`，`type=TXT`、`name=fqdn`、`content=value`、`ttl=120`。
- `delete_txt`：按 `name` + `content` 列出记录再 `DELETE`（失败仅告警）。

### 3. 签发流程（`services/acme/mod.rs::issue_certificate`）

入参：`domain: &str`、设置（provider/token/staging）、`AppHandle`（进度事件）。

1. 校验 `server_name` 非空且为合法域名；读设置 `dns_provider`/`cloudflare_api_token`/`acme_use_staging`。
2. 建/复用 ACME 账户（`acme-account.json`）→ `new_order([Identifier::Dns(domain)])`。
3. 遍历 `order.authorizations()`，对每个 `authz.challenge(ChallengeType::Dns01)`：
   - `let value = challenge.key_authorization()?.dns_value()`；
   - `create_txt("_acme-challenge.<domain>", value)`；
   - 固定等待 ~15s（DNS TTL 传播），随后由 ACME 轮询重试兜底；
   - `challenge.set_ready().await`。
4. `order.poll_ready(&RetryPolicy::default())` → `order.finalize()` → `order.poll_certificate(...)` 得到 PEM 链。
5. 私钥与证书写入 `sites-data/certs/<domain>.{key,crt}`（私钥 PEM 由 `finalize` 返回）。
6. 更新 `Site.ssl`：`enabled=true`、`cert_path`/`key_path`、`acme=true`、`cert_expires_at`（签发时按 LE 90 天 + 当前时间计算并落 Rfc3339，**不引入 x509 解析依赖**）。
7. 重新生成该站点 nginx conf 并 reload（复用既有 `regenerate`/reload 路径）。
8. `delete_txt` 清理（无论成败都尽力清理）。

失败处理：任一步失败即返回错误信息（含 ACME/Cloudflare 的响应体摘要），并**尽力清理已创建的 TXT**；不修改 `Site.ssl`。

事件：签发过程 emit `acme-progress { domain, phase, message }`（phase: `creating-order` / `waiting-dns` / `validating` / `downloading` / `done` / `error`），供前端进度显示。

### 4. 自动续期（`services/acme/renew_scheduler.rs`）

- 常驻循环，复用 `backup_scheduler::run_scheduler` 的模式（`lib.rs` 内 `tauri::async_runtime::spawn`）。
- 每小时醒来一次；对每个 `ssl.acme == true` 的站点，计算 `needs_renewal(cert_expires_at, now, 30 天)`；需续期则调用同一 `issue_certificate`，成功后 reload + 审计 `acme_renew`（含域名与结果）。
- 纯函数 `needs_renewal(expires: Option<&str>, now: DateTime<Local>, days: i64) -> bool`：
  - `expires` 为 `None` 或不可解析 → **返回 `true`**（无法确认有效期，重签一次；自愈且只会发生在用户已开启 ACME 的站点）
  - 已过期或距到期 `< days` 天 → `true`；否则 `false`

### 5. 模型与前端

- `SslConfig` 增：

  ```rust
  #[serde(default)]
  pub acme: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cert_expires_at: Option<String>,
  ```

  前端 `src/models/website.ts` 同步。旧数据因 `#[serde(default)]` 兼容。
- 站点编辑对话框（`SiteEditDialog.vue`）SSL 区：
  - 证书来源：自签 / Let's Encrypt（DNS-01）
  - 选 ACME 时显示「申请证书 / 重新申请」按钮 + 进度（监听 `acme-progress`）+ 到期时间；未配置 DNS 凭据时给出引导
- 设置页（`SettingsPage.vue`）新增「DNS 服务商」区：
  - 服务商下拉（v1 仅 Cloudflare）
  - API Token（密码输入框）
  - 明文存储风险提示文案
  - 「使用 Let's Encrypt 测试环境（staging）」开关（避免调试触发生产限流）

### 6. 命令

- `issue_site_certificate(site_id) -> Result<(), String>`（手动申请/重新申请，异步长任务，前端按 `acme-progress` 显示进度）
- 设置沿用既有 `get_settings`/`save_settings`（扩展字段）

### 7. 测试

- `cargo test --lib` 单测：
  - `dns` zone 后缀匹配（`a.b.example.com` → `example.com`；无匹配返回错误）
  - `_acme-challenge.<domain>` FQDN 组装
  - `needs_renewal`：过期/缺失/不可解析/未到期/即将到期（<30 天）
  - `SslConfig` 旧 JSON（无 `acme`/`cert_expires_at`）反序列化默认值
- 网络路径（真实 Cloudflare + 真实域名）靠手测：
  1. 配好 Token（域名在 Cloudflare）→ 站点 server_name 填该域名 → 申请证书
  2. 浏览器访问 `https://<域名>` 证书链有效、`nginx -t` 通过
  3. 观察到期时间显示；把 `cert_expires_at` 手改为更早 → 触发续期（可缩短调度间隔临时验证）
- 前端 `npx vue-tsc --noEmit`

## 边界（不做）

- 不做 HTTP-01 / TLS-ALPN-01。
- 不做 Cloudflare 之外的 DNS 服务商（仅留 trait 扩展点）。
- 不做通配符（`*.example.com`）证书。
- 不接入 Windows 凭据管理器（按决策：settings.json 明文 + UI 风险提示）。
- 不做证书吊销、不做多域名（SAN）合并申请（v1 单域名）。

## 风险与提示

- **DNS-01 传播延迟**：固定 15s 等待 + ACME 轮询重试兜底；若失败提示用户稍后重试。
- **LE 限流**：提供 staging 开关；失败信息保留 ACME 返回摘要。
- **明文 Token**：UI 明确标注；settings.json 权限依赖用户环境。

## 改动文件清单

- `src-tauri/Cargo.toml`（新增 `instant-acme`、`base64`）
- `src-tauri/src/services/acme/mod.rs`（签发流程）
- `src-tauri/src/services/acme/dns/mod.rs`（trait + 注册表）
- `src-tauri/src/services/acme/dns/cloudflare.rs`（Cloudflare 实现）
- `src-tauri/src/services/acme/renew_scheduler.rs`（自动续期循环）
- `src-tauri/src/services/mod.rs`（注册模块）
- `src-tauri/src/models/website.rs`（`SslConfig` 增字段）
- `src-tauri/src/commands/website.rs`（`issue_site_certificate` 命令）
- `src-tauri/src/lib.rs`（注册命令 + 启动续期循环）
- `src-tauri/src/models/settings.rs`（DNS 服务商设置字段）
- `src/models/website.ts`、`src/models/settings.ts`
- `src/modules/website-manager/components/SiteEditDialog.vue`
- `src/modules/settings/pages/SettingsPage.vue`
- `src/locales/zh-CN.ts`、`src/locales/en-US.ts`
