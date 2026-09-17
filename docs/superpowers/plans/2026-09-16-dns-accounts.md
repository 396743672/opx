# DNS 账号独立化 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 把证书的 DNS 服务商凭证从全局设置抽成独立的「DNS 账号」实体，站点按 id 引用，支持四家服务商。

**架构：** `DnsProvider` trait 从 `acme::dns` 下沉为公用抽象（`list_zones` / `find_zone` / `get_value` / `set_value` / `delete_value`），DDNS 的四家实现只需 `impl DnsProvider`，`DdnsProvider::sync_record` 变成 blanket impl 的默认实现。新增 `DnsAccountManager` + `/dns-accounts` 页面。启动时按「`dns-accounts.json` 不存在」为信号把旧全局 token 迁移成一条账号。

**技术栈：** Rust（anyhow / serde / tokio / reqwest / hmac+sha2）、Tauri v2 命令、Vue 3 + TS + vue-i18n。

**规格：** `docs/superpowers/specs/2026-09-16-dns-accounts-design.md`

**验证命令（全计划通用）：**

```bash
cd src-tauri && cargo test 2>&1 | tail -30          # Rust 测试
cd src-tauri && cargo clippy --all-targets 2>&1 | grep -E "^(warning|error)" | head
npx vue-tsc --noEmit                                 # 前端类型检查
npm run build                                        # 前端构建（vue-tsc 查不出模板结构错误，必须跑这个）
```

---

## 关键约束（每个任务都要遵守）

1. **`cargo fmt` 会改写全部文件** —— 绝不要跑。需要格式化单文件时用 `cd src-tauri && rustfmt --edition 2021 <单个文件路径>`。
2. **TDD**：先写失败的测试，跑一次确认失败，再写实现，再跑确认通过，然后 commit。
3. **不要动签名层**。`ddns/{cloudflare,aliyun,dnspod,huawei}.rs` 里的 `*_canonical_request`、`*_authorization`、`hmac_sha256`、`hex`、`percent_encode`/`pe`、`CONTENT_TYPE`/`HOST`/`SERVICE`/`VERSION` 常量，以及它们的测试——**一行都不改**。本次只搬动 `impl` 与记录读写逻辑。
4. **DELETE 用 DELETE 方法，PUT 用 PUT**：`set_value` ≠ `delete_value`。别用「写空串」代替删除。

---

## 文件结构

**新建**

| 文件 | 职责 |
|---|---|
| `src-tauri/src/models/dns_account.rs` | `DnsAccount` 结构体 + 序列化测试 |
| `src-tauri/src/services/dns_account/mod.rs` | `DnsAccountManager`（CRUD + 落盘 + `is_referenced` 查询） |
| `src-tauri/src/services/dns_account/migrate.rs` | 旧全局 token → 账号的迁移（含「文件不存在」信号） |
| `src-tauri/src/commands/dns_account.rs` | 5 个 Tauri 命令 |
| `src/modules/dns-accounts/pages/DnsAccountsPage.vue` | 列表页 |
| `src/modules/dns-accounts/components/DnsAccountDialog.vue` | 新增/编辑对话框（含测试按钮） |
| `src/models/dns-account.ts` | 前端类型 + `emptyAccount()` |

**修改**

| 文件 | 改什么 |
|---|---|
| `src-tauri/src/services/acme/dns/mod.rs` | trait 重定义为 5 方法 + `provider_for_account` |
| `src-tauri/src/services/acme/dns/cloudflare.rs` | 改成 `impl DnsProvider`（补 `list_zones` / `delete_value`，`create_txt`→`set_value`） |
| `src-tauri/src/services/acme/mod.rs` | `AcmeSettings.account`；签发/清理走 `set_value`/`get_value`/`delete_value` |
| `src-tauri/src/services/acme/renew_scheduler.rs` | 按站点 `dns_account_id` 取账号 |
| `src-tauri/src/services/ddns/mod.rs` | trait 收窄 + blanket impl + `provider_for_account` |
| `src-tauri/src/services/ddns/{cloudflare,aliyun,dnspod,huawei}.rs` | `impl DnsProvider`，删 `sync_record`，拆出 `get_value`/`set_value`/`delete_value` |
| `src-tauri/src/models/settings.rs` | 删 `dns_provider` / `cloudflare_api_token` |
| `src-tauri/src/models/website.rs` | `SslConfig.dns_account_id` |
| `src-tauri/src/commands/config.rs` | 删 `test_dns_token` |
| `src-tauri/src/commands/website.rs` | 签发按账号；新增 `list_account_refs` |
| `src-tauri/src/lib.rs` | 迁移调用 + 注册 State + 注册命令 |
| `src-tauri/src/services/mod.rs` | `pub mod dns_account;` |
| `src-tauri/src/commands/mod.rs` | `pub mod dns_account;` |
| `src/modules/settings/pages/SettingsPage.vue` | 删 DNS 服务商区块 + 死代码 |
| `src/modules/website-manager/components/SiteEditDialog.vue` | 账号下拉替代 `hasDnsToken` |
| `src/router/index.ts`、`src/layouts/Sidebar.vue` | 新路由与菜单 |
| `src/models/{settings,website}.ts` | 同步类型 |
| `src/locales/{zh-CN,en-US}.ts` | 删旧键、加新键 |

**任务依赖顺序：** T1 → T2 → T3 → T4 → T5 → T6 → T7 → T8 → T9 → T10 → T11。

T2 是全局编译前提（trait 一变，四家实现与 acme 调用点全都要跟着改），所以 **T2 做完到 T6 结束之间，`cargo test`（全量）必然失败** —— 那是预期状态，不是本次改动引入的 bug。逐任务验证时用**单模块测试**：

```bash
cd src-tauri && cargo test --lib services::ddns    # T3/T4 用
cd src-tauri && cargo test --lib services::acme    # T2/T5/T6 用
cd src-tauri && cargo test --lib models            # T1 用
```

**但 T3 有个硬性前提**：T3 的 `Fake` 要实现 `DnsProvider`，而 T3 的测试步骤不能改 T2 的文件。所以 T3 开头**必须先确认** T2 已落盘：

```bash
grep -n "fn list_zones\|fn delete_value" src-tauri/src/services/acme/dns/mod.rs
```

两条都必须有输出才能开始 T3。缺任何一条 → **停下**，先把 T2 补完（T2 未完成时不要硬推 T3，否则会写出对不上 trait 的实现）。

同理 T4 依赖 T3（`sync_record` 的默认实现），T5/T6 依赖 T2。T7 结束时全量 `cargo test` 恢复绿。

---

## 任务 1：DnsAccount 模型

**文件：**
- 创建：`src-tauri/src/models/dns_account.rs`
- 修改：`src-tauri/src/models/mod.rs`（加 `pub mod dns_account;`）

- [ ] **步骤 1：写失败的测试**

创建 `src-tauri/src/models/dns_account.rs`，只含测试：

```rust
//! DNS 服务商账号（证书 DNS-01 用）。一份凭证可被多个站点引用。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsAccount {
    pub id: String,
    pub name: String,
    /// "cloudflare" | "aliyun" | "dnspod" | "huawei"
    pub provider: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub access_key_id: String,
    #[serde(default)]
    pub access_key_secret: String,
    #[serde(default)]
    pub zones: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tested_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 只填必填三字段必须能反序列化（老文件/手写文件容错）
    #[test]
    fn deserializes_with_required_fields_only() {
        let a: DnsAccount = serde_json::from_str(
            r#"{"id":"a1","name":"公司 CF","provider":"cloudflare"}"#,
        )
        .unwrap();
        assert_eq!(a.id, "a1");
        assert_eq!(a.provider, "cloudflare");
        assert!(a.token.is_empty());
        assert!(a.access_key_id.is_empty());
        assert!(a.access_key_secret.is_empty());
        assert!(a.zones.is_empty());
        assert!(a.tested_at.is_none());
    }

    /// tested_at 为 None 时不得写进 JSON（保持文件干净）
    #[test]
    fn omits_untested_timestamp_on_serialize() {
        let a = DnsAccount {
            id: "a1".into(),
            name: "n".into(),
            provider: "aliyun".into(),
            token: String::new(),
            access_key_id: "k".into(),
            access_key_secret: "s".into(),
            zones: vec!["example.com".into()],
            tested_at: None,
        };
        let json = serde_json::to_string(&a).unwrap();
        assert!(!json.contains("tested_at"), "未测试不应出现 tested_at：{}", json);
        assert!(json.contains("example.com"));
    }

    /// 往返一致
    #[test]
    fn round_trips() {
        let a = DnsAccount {
            id: "a1".into(),
            name: "公司 CF".into(),
            provider: "cloudflare".into(),
            token: "tok".into(),
            access_key_id: String::new(),
            access_key_secret: String::new(),
            zones: vec!["a.com".into(), "b.com".into()],
            tested_at: Some("2026-09-16T10:00:00+08:00".into()),
        };
        let back: DnsAccount =
            serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        assert_eq!(back.id, a.id);
        assert_eq!(back.zones, a.zones);
        assert_eq!(back.tested_at, a.tested_at);
    }
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test --lib models::dns_account`
预期：FAIL —— `error[E0583]: file not found for module dns_account`（因为 `models/mod.rs` 还没声明）。先在 `models/mod.rs` 加 `pub mod dns_account;` 再跑一次，此时应 PASS（结构体已在同一步写入）。

> 注：本任务是纯数据定义，结构体与测试一起写。这里「失败」指模块未声明导致的编译失败，用它确认 `mod.rs` 确实需要改。

- [ ] **步骤 3：补上 `models/mod.rs` 声明**

查看 `src-tauri/src/models/mod.rs`，按字母序在 `pub mod config;` 附近的合适位置加：

```rust
pub mod dns_account;
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cd src-tauri && cargo test --lib models::dns_account`
预期：PASS，3 个测试全绿。

- [ ] **步骤 5：Commit**

```bash
git add src-tauri/src/models/dns_account.rs src-tauri/src/models/mod.rs
git commit -m "feat(dns): DnsAccount 模型（含零字段容错与 tested_at 省略）"
```

---

## 任务 2：DnsProvider trait 重定义 + Cloudflare 改造

**本任务把 trait 改成 5 方法。做完后 `acme/mod.rs`、`ddns/*` 会编译失败——T3~T7 逐个修好。**

**文件：**
- 修改：`src-tauri/src/services/acme/dns/mod.rs`
- 修改：`src-tauri/src/services/acme/dns/cloudflare.rs`

- [ ] **步骤 1：写失败的测试**

在 `src-tauri/src/services/acme/dns/mod.rs` 的 `#[cfg(test)] mod tests` 里，保留现有的 `longest_zone_match_picks_most_specific_suffix` 与 `acme_challenge_fqdn_prefixes_label`，追加：

```rust
    /// 凭证缺失或服务商未知时必须返回 None（调用方据此报「请先配置凭证」）
    #[test]
    fn provider_for_account_rejects_missing_credentials() {
        let base = crate::models::dns_account::DnsAccount {
            id: "a1".into(),
            name: "n".into(),
            provider: "cloudflare".into(),
            token: String::new(),
            access_key_id: String::new(),
            access_key_secret: String::new(),
            zones: vec![],
            tested_at: None,
        };
        // cloudflare 需要 token
        assert!(provider_for_account(&base).is_none());

        let mut with_token = base.clone();
        with_token.token = "t".into();
        assert!(provider_for_account(&with_token).is_some());

        // 未知服务商
        let mut unknown = with_token.clone();
        unknown.provider = "nope".into();
        assert!(provider_for_account(&unknown).is_none());

        // aliyun / dnspod / huawei 的双凭证校验由 ddns::provider_for_account 负责
        // （见 T3）；此处只断「acme 侧不认识这三家时不得误放行」。
        // 正例断言（双凭证齐全 → Some）留到 T4 把三家改成 impl DnsProvider 后补，
        // 那时它们才结构上可能产出 Box<dyn DnsProvider>。
        for p in ["aliyun", "dnspod", "huawei"] {
            let mut half = base.clone();
            half.provider = p.into();
            half.access_key_id = "k".into(); // 只有 id，没有 secret
            assert!(
                provider_for_account(&half).is_none(),
                "{} 在 acme 侧未注册，不应放行",
                p
            );
            let mut full = half.clone();
            full.access_key_secret = "s".into();
            assert!(
                provider_for_account(&full).is_none(),
                "{} 在 acme 侧未注册，凭证齐全也不应放行",
                p
            );
        }
    }
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test --lib services::acme::dns`
预期：FAIL —— `error[E0425]: cannot find function 'provider_for_account'`。

- [ ] **步骤 3：重写 trait 与工厂函数**

把 `src-tauri/src/services/acme/dns/mod.rs` 的头部改写成：

```rust
//! DNS 服务商抽象（DNS-01 与 DDNS 共用）。
//!
//! 五个方法在两个 trait 上同名同签名，`DdnsProvider` 只多一个带默认实现的
//! `sync_record`，且对 `dyn DnsProvider` 走 blanket impl —— 故四个服务商
//! 实现文件只需 `impl DnsProvider`，不必各写一遍同步逻辑。
//!
//! 注意：需以 `Box<dyn DnsProvider>` 使用，而 trait 对象不支持 `async fn`（RPITIT 非
//! dyn 兼容），故统一返回 boxed future。

pub mod cloudflare;

use std::future::Future;
use std::pin::Pin;

use anyhow::Result;

use crate::models::dns_account::DnsAccount;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait DnsProvider: Send + Sync {
    fn id(&self) -> &str;
    /// 该账户下所有 zone 名（「测试」按钮与域名归属确认用）
    fn list_zones<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>>>;
    /// 返回 domain 所属 zone（注册域名），用于拼接记录全名。
    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, Result<String>>;
    /// 读 name+type 的当前值（无记录 → None）
    fn get_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str)
        -> BoxFuture<'a, Result<Option<String>>>;
    /// 无记录则新建，有则更新
    fn set_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str, value: &'a str)
        -> BoxFuture<'a, Result<()>>;
    /// 删除 name+type 的记录；不存在时视为成功（幂等）
    fn delete_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str) -> BoxFuture<'a, Result<()>>;
}

/// 按账号（服务商 + 凭证）取实现；凭证缺失或服务商未知返回 None。
pub fn provider_for_account(a: &DnsAccount) -> Option<Box<dyn DnsProvider>> {
    match a.provider.as_str() {
        "cloudflare" if !a.token.trim().is_empty() => {
            Some(Box::new(cloudflare::Cloudflare::new(a.token.clone())))
        }
        _ => None,
    }
}
```

（`longest_zone_match` 与 `acme_challenge_fqdn` 两个自由函数保持原样，不动。）

- [ ] **步骤 4：改造 Cloudflare 实现**

把 `src-tauri/src/services/acme/dns/cloudflare.rs` 的 `impl DnsProvider for Cloudflare` 整块替换。**保留** `API`、`DNS_PROPAGATION_WAIT_SECS`、`Cloudflare::new`、`get`、`json`、`zone_id` 原样不动。

```rust
impl DnsProvider for Cloudflare {
    fn id(&self) -> &str {
        "cloudflare"
    }

    fn list_zones<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            // ponytail: 分页上限，防账户 zone 过多时反复全量列举；20 * 50 = 1000 个 zone，
            // 超出请改用按 name 过滤查询
            const MAX_PAGES: u32 = 20;
            let mut names: Vec<String> = Vec::new();
            let mut page = 1u32;
            loop {
                let body = self
                    .get(&format!("{}/zones?per_page=50&page={}", API, page))
                    .await?;
                let arr = body["result"].as_array().cloned().unwrap_or_default();
                if arr.is_empty() {
                    break;
                }
                for z in &arr {
                    if let Some(n) = z["name"].as_str() {
                        names.push(n.to_string());
                    }
                }
                if arr.len() < 50 || page >= MAX_PAGES {
                    break;
                }
                page += 1;
            }
            Ok(names)
        })
    }

    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move {
            let names = self.list_zones().await?;
            longest_zone_match(domain, &names)
                .ok_or_else(|| anyhow!("该域名不在 Cloudflare 账户的 zone 中：{}", domain))
        })
    }

    fn get_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str)
        -> BoxFuture<'a, Result<Option<String>>>
    {
        Box::pin(async move {
            let zone = self.find_zone(fqdn.trim_start_matches("_acme-challenge.")).await?;
            let zone_id = self.zone_id(&zone).await?;
            let body = self
                .get(&format!(
                    "{}/zones/{}/dns_records?type={}&name={}",
                    API, zone_id, rtype, fqdn
                ))
                .await?;
            Ok(body["result"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|r| r["content"].as_str())
                .map(|s| s.to_string()))
        })
    }

    fn set_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str, value: &'a str)
        -> BoxFuture<'a, Result<()>>
    {
        Box::pin(async move {
            let zone = self.find_zone(fqdn.trim_start_matches("_acme-challenge.")).await?;
            let zone_id = self.zone_id(&zone).await?;
            // 读现有记录 id：有则 PUT，无则 POST（Cloudflare 无 UPSERT，需分两步）
            let list = self
                .get(&format!(
                    "{}/zones/{}/dns_records?type={}&name={}",
                    API, zone_id, rtype, fqdn
                ))
                .await?;
            let existing = list["result"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|r| r["id"].as_str())
                .map(|s| s.to_string());
            let body = serde_json::json!({
                "type": rtype, "name": fqdn, "content": value, "ttl": 120
            });
            let resp = match existing {
                Some(rid) => {
                    self.client
                        .put(format!("{}/zones/{}/dns_records/{}", API, zone_id, rid))
                        .bearer_auth(&self.token)
                        .json(&body)
                        .send()
                        .await?
                }
                None => {
                    self.client
                        .post(format!("{}/zones/{}/dns_records", API, zone_id))
                        .bearer_auth(&self.token)
                        .json(&body)
                        .send()
                        .await?
                }
            };
            Self::json(resp).await.map(|_| ())
        })
    }

    fn delete_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str)
        -> BoxFuture<'a, Result<()>>
    {
        Box::pin(async move {
            let zone = self.find_zone(fqdn.trim_start_matches("_acme-challenge.")).await?;
            let zone_id = self.zone_id(&zone).await?;
            let body = self
                .get(&format!(
                    "{}/zones/{}/dns_records?type={}&name={}",
                    API, zone_id, rtype, fqdn
                ))
                .await?;
            if let Some(records) = body["result"].as_array() {
                for r in records {
                    let Some(id) = r["id"].as_str() else { continue };
                    // 清理失败不阻断签发，但要留痕（否则 DNS 里会静默残留记录）
                    match self
                        .client
                        .delete(format!("{}/zones/{}/dns_records/{}", API, zone_id, id))
                        .bearer_auth(&self.token)
                        .send()
                        .await
                    {
                        Ok(resp) if resp.status().is_success() => {}
                        Ok(resp) => tracing::warn!(
                            status = %resp.status(), name = %fqdn,
                            "删除 DNS 记录失败（响应非成功）"
                        ),
                        Err(e) => tracing::warn!(
                            error = %e, name = %fqdn,
                            "删除 DNS 记录请求失败"
                        ),
                    }
                }
            }
            Ok(())
        })
    }
}
```

- [ ] **步骤 5：写一个纯函数测试覆盖「读写语义分流」**

Cloudflare 的网络路径无法单测，但要有一条测试锁住**本次最容易犯的错：把 get/set/delete 混成一个**。在 `cloudflare.rs` 末尾加：

```rust
#[cfg(test)]
mod tests {
    /// const 断言：API 基址必须是 v4（改错版本整条路径失效）
    #[test]
    fn api_base_is_v4() {
        assert_eq!(API, "https://api.cloudflare.com/client/v4");
        assert!(API.ends_with("/v4"));
    }

    /// DNS-01 的 TXT 名前缀必须是 `_acme-challenge.`：
    /// get/set/delete 三处都靠 trim_start_matches 反推 zone，
    /// 前缀写错会让 find_zone 拿整个 challenge 名去匹配 zone 而失败。
    #[test]
    fn challenge_prefix_is_stripped_for_zone_lookup() {
        let fqdn = crate::services::acme::dns::acme_challenge_fqdn("example.com");
        assert_eq!(fqdn, "_acme-challenge.example.com");
        let for_zone = fqdn.trim_start_matches("_acme-challenge.");
        assert_eq!(for_zone, "example.com");
    }
}
```

- [ ] **步骤 5.5：修掉这两处旧调用点（否则后续任务都在红 build 上开工）**

`provider_for` 被重命名为 `provider_for_account` 后，有两处调用点会报 `cannot find function provider_for`——**这不是「预期红」**，而是本步骤要修干净的东西。改完它们，`cargo test --lib` 只剩真正属于 T3/T4 的错误。

改 `src-tauri/src/services/acme/mod.rs`：

```rust
// 旧（第 15 行）
use dns::provider_for;
// 新
use dns::provider_for_account;
```

```rust
// 旧（第 50 行）
    let provider = provider_for(&settings.dns_provider, &settings.cloudflare_api_token)
        .ok_or_else(|| anyhow!("未配置 DNS 服务商或服务商不支持：{}", settings.dns_provider))?;
// 新（AcmeSettings 的字段改造是 T5 的事，这里用 account 字段的最小形态）
    let provider = provider_for_account(&settings.account)
        .ok_or_else(|| anyhow!("未配置 DNS 服务商或服务商不支持：{}", settings.account.provider))?;
```

改 `src-tauri/src/commands/config.rs`（`test_dns_token` 探针）：

```rust
// 旧（第 41 行）
        let p = crate::services::acme::dns::provider_for(&provider, &token)
            .ok_or_else(|| format!("不支持的服务商: {}", provider))?;
// 新
        let p = crate::services::acme::dns::provider_for_account(
            &crate::models::dns_account::DnsAccount {
                id: String::new(),
                name: String::new(),
                provider: provider.clone(),
                token: token.clone(),
                access_key_id: token.clone(),
                access_key_secret: token.clone(),
                zones: vec![],
                tested_at: None,
            },
        )
        .ok_or_else(|| format!("不支持的服务商: {}", provider))?;
```

**注意**：`acme/mod.rs` 里 `AcmeSettings` 的字段改造（`dns_provider`/`cloudflare_api_token` → `account`）**不在本步骤**——那是 T5。所以本步骤改完后 `acme/mod.rs` 仍会报「no field `account`」，这是**真正的**预期红，由 T5 修。本步骤只消灭「函数名找不到」这一类错误。

同时把 `create_txt` / `delete_txt` 的调用点改成新方法名（这两处也在 `acme/mod.rs` 与 `commands/config.rs`）：

```rust
// acme/mod.rs
provider.set_value(&fqdn, "TXT", &value)      // 原 provider.create_txt(&fqdn, &value)
provider.delete_value(f, "TXT")               // 原 provider.delete_txt(f, v)
// commands/config.rs
p.set_value(&fqdn, "TXT", &value)             // 原 p.create_txt(&fqdn, &value)
p.delete_value(&fqdn, "TXT")                  // 原 p.delete_txt(&fqdn, &value)
```

> 这几处是为了让 build 从「名字找不到」推进到「字段还没改」——后者才是 T5 的活。**不要**顺手把 `AcmeSettings` 也改了，那会让 T5 无从下手。

- [ ] **步骤 6：运行测试**

运行：`cd src-tauri && cargo test --lib services::acme::dns`
预期：`models::dns_account` 与 `acme::dns` 的测试 PASS。残留错误应**只有** `no field 'account' on type '&AcmeSettings'`（T5 修）与 `services::ddns` 的 trait 不匹配（T3 修）——**不要再有 `cannot find function provider_for`**。

- [ ] **步骤 7：Commit**

```bash
git add src-tauri/src/services/acme/dns/mod.rs src-tauri/src/services/acme/dns/cloudflare.rs src-tauri/src/services/acme/mod.rs src-tauri/src/commands/config.rs
git commit -m "feat(dns): DnsProvider trait 下沉为五方法（list_zones/get_value/set_value/delete_value）"
```

---

## 任务 3：DDNS trait 收窄 + blanket impl

**文件：**
- 修改：`src-tauri/src/services/ddns/mod.rs`

- [ ] **步骤 1：写失败的测试**

在 `src-tauri/src/services/ddns/mod.rs` 的 `mod tests` 里，保留现有三个测试，追加：

```rust
    /// sync_record 的默认实现必须走「读-比较-写」三分支：
    /// 无记录 → 新建文案；值不同 → 更新文案；值相同 → "未变"。
    /// 用一个只记账的假 provider 验证，不碰网络。
    struct Fake {
        cur: Option<String>,
        /// `Mutex` 而非 `RefCell`：`DnsProvider: Send + Sync`，`RefCell` 过不了 Send/Sync 约束。
        /// （原计划写的是 `RefCell`，导致 5 处 E0277，已修正。）
        writes: std::sync::Mutex<Vec<String>>,
    }

    impl crate::services::acme::dns::DnsProvider for Fake {
        fn id(&self) -> &str {
            "fake"
        }
        fn list_zones<'a>(
            &'a self,
        ) -> crate::services::acme::dns::BoxFuture<'a, anyhow::Result<Vec<String>>> {
            Box::pin(async { Ok(vec!["example.com".to_string()]) })
        }
        fn find_zone<'a>(
            &'a self,
            _domain: &'a str,
        ) -> crate::services::acme::dns::BoxFuture<'a, anyhow::Result<String>> {
            Box::pin(async { Ok("example.com".to_string()) })
        }
        fn get_value<'a>(
            &'a self,
            _fqdn: &'a str,
            _rtype: &'a str,
        ) -> crate::services::acme::dns::BoxFuture<'a, anyhow::Result<Option<String>>> {
            Box::pin(async move { Ok(self.cur.clone()) })
        }
        fn set_value<'a>(
            &'a self,
            _fqdn: &'a str,
            _rtype: &'a str,
            value: &'a str,
        ) -> crate::services::acme::dns::BoxFuture<'a, anyhow::Result<()>> {
            Box::pin(async move {
                self.writes.lock().unwrap().push(value.to_string());
                Ok(())
            })
        }
        fn delete_value<'a>(
            &'a self,
            _fqdn: &'a str,
            _rtype: &'a str,
        ) -> crate::services::acme::dns::BoxFuture<'a, anyhow::Result<()>> {
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn sync_record_default_impl_three_branches() {
        // 无记录 → 新建
        let p = Fake { cur: None, writes: Default::default() };
        let action = DdnsProvider::sync_record(&p, "a.example.com", "A", "1.2.3.4")
            .await
            .unwrap();
        assert_eq!(action, "A 记录新建 1.2.3.4");
        assert_eq!(p.writes.lock().unwrap().as_slice(), ["1.2.3.4"]);

        // 值不同 → 更新
        let p = Fake { cur: Some("9.9.9.9".into()), writes: Default::default() };
        let action = DdnsProvider::sync_record(&p, "a.example.com", "A", "1.2.3.4")
            .await
            .unwrap();
        assert_eq!(action, "A 记录更新 9.9.9.9 → 1.2.3.4");
        assert_eq!(p.writes.lock().unwrap().as_slice(), ["1.2.3.4"]);

        // 值相同 → 未变，且绝不写
        let p = Fake { cur: Some("1.2.3.4".into()), writes: Default::default() };
        let action = DdnsProvider::sync_record(&p, "a.example.com", "A", "1.2.3.4")
            .await
            .unwrap();
        assert_eq!(action, "未变");
        assert!(p.writes.lock().unwrap().is_empty(), "值未变时不应发起写入");
    }
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test --lib services::ddns`
预期：FAIL —— `Fake` 无法实现 `DnsProvider`（旧 trait 的 `create_txt`/`delete_txt` 与之不匹配），或 `sync_record` 尚不是默认方法无法用 `DdnsProvider::sync_record(&p, ...)` 调用。
（注：此时也会同时看到 4 处 `E0046`（四家 ddns 实现）与 2 处 `E0609`，那是 T4/T5 的入口，见步骤 5。）

- [ ] **步骤 3：收窄 trait 并加 blanket impl**

把 `src-tauri/src/services/ddns/mod.rs` 的 trait 段替换为：

```rust
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// DDNS 只多一个 `sync_record`（带默认实现的读-比较-写），其余五个方法
/// 与 `DnsProvider` 同名同签名；blanket impl 让 `dyn DnsProvider` 直接可用。
pub trait DdnsProvider: Send + Sync {
    fn id(&self) -> &str;
    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, anyhow::Result<String>>;
    fn get_value<'a>(
        &'a self,
        fqdn: &'a str,
        rtype: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<Option<String>>>;
    fn set_value<'a>(
        &'a self,
        fqdn: &'a str,
        rtype: &'a str,
        value: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<()>>;

    /// 把 fqdn 的 rtype("A"/"AAAA") 记录同步为 ip。返回动作文案：
    /// "A 记录新建 1.2.3.4" / "A 记录更新 old → new" / "未变"
    fn sync_record<'a>(
        &'a self,
        fqdn: &'a str,
        rtype: &'a str,
        ip: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<String>> {
        Box::pin(async move {
            match self.get_value(fqdn, rtype).await? {
                Some(old) if old == ip => Ok("未变".to_string()),
                Some(old) => {
                    self.set_value(fqdn, rtype, ip).await?;
                    Ok(format!("{} 记录更新 {} → {}", rtype, old, ip))
                }
                None => {
                    self.set_value(fqdn, rtype, ip).await?;
                    Ok(format!("{} 记录新建 {}", rtype, ip))
                }
            }
        })
    }
}

/// 唯一的 blanket impl：任何 DnsProvider 自动是 DdnsProvider。
/// （不要额外为具体类型写 `impl DdnsProvider for X`，会与这条冲突。）
impl<T: crate::services::acme::dns::DnsProvider + ?Sized> DdnsProvider for T {}
```

同文件里的 `provider_for(s: &AppSettings)` 改为：

```rust
/// 按账号取实现；凭证缺失返回 None（调用方给出「请先配置凭证」错误）。
pub fn provider_for_account(a: &crate::models::dns_account::DnsAccount) -> Option<Box<dyn DdnsProvider>> {
    match a.provider.as_str() {
        "cloudflare" if !a.token.trim().is_empty() => Some(Box::new(
            cloudflare::Cloudflare::new(a.token.clone()),
        )),
        "aliyun"
            if !a.access_key_id.trim().is_empty() && !a.access_key_secret.trim().is_empty() =>
        {
            Some(Box::new(aliyun::Aliyun::new(
                a.access_key_id.clone(),
                a.access_key_secret.clone(),
            )))
        }
        "dnspod"
            if !a.access_key_id.trim().is_empty() && !a.access_key_secret.trim().is_empty() =>
        {
            Some(Box::new(dnspod::Dnspod::new(
                a.access_key_id.clone(),
                a.access_key_secret.clone(),
            )))
        }
        "huawei"
            if !a.access_key_id.trim().is_empty() && !a.access_key_secret.trim().is_empty() =>
        {
            Some(Box::new(huawei::Huawei::new(
                a.access_key_id.clone(),
                a.access_key_secret.clone(),
            )))
        }
        _ => None,
    }
}
```

> 注意：`DnsAccount` 只有一对 `access_key_id`/`access_key_secret`。DNSPod 的 `SecretId`/`SecretKey` 与华为的 `AccessKey`/`SecretKey`、阿里云的 `AccessKeyId`/`AccessKeySecret` 都映射到这一对——UI 按 provider 显示对应标签即可。

`sync_once` 的凭证来源改为**调用方传入**（原计划让它内联构造 `DnsAccount`，那等于让 DDNS 间接依赖 `AppSettings` 的全部字段，且折叠逻辑无法单测）：

```rust
pub async fn sync_once(
    s: &crate::models::settings::AppSettings,
    account: &crate::models::dns_account::DnsAccount,
) -> anyhow::Result<SyncResult> {
    ...
    let provider = provider_for_account(account)
        .ok_or_else(|| anyhow::anyhow!("DDNS 服务商凭证未配置（{}）", account.provider))?;
```

两个调用点同步改：`services/ddns/scheduler.rs` 与 `commands/config.rs` 的 `sync_ddns_now`，
都改为 `sync_once(&s, &ddns_account_of(&s))`。

- [ ] **步骤 4：加 `ddns_account_of` 适配函数**

把上面那段折叠逻辑抽成 `services/ddns/mod.rs` 的**独立公开函数**（不要再内联进 `sync_once`）：

```rust
/// 把设置页的 `ddns_*` 字段折成 `DnsAccount`，供 `sync_once` 用。
///
/// **这是刻意的适配层**：DDNS 的凭证字段与证书账号解耦（用户明确要求「证书可以
/// 是其他家的」），DDNS 不引入账号概念，只是复用同一个 provider 工厂。
pub fn ddns_account_of(s: &crate::models::settings::AppSettings) -> crate::models::dns_account::DnsAccount {
    crate::models::dns_account::DnsAccount {
        id: String::new(),
        name: "ddns".into(),
        provider: s.ddns_provider.clone(),
        token: s.ddns_cloudflare_token.clone(),
        access_key_id: match s.ddns_provider.as_str() {
            "aliyun" => s.ddns_aliyun_access_key_id.clone(),
            "dnspod" => s.ddns_dnspod_secret_id.clone(),
            "huawei" => s.ddns_huawei_access_key.clone(),
            _ => String::new(),
        },
        access_key_secret: match s.ddns_provider.as_str() {
            "aliyun" => s.ddns_aliyun_access_key_secret.clone(),
            "dnspod" => s.ddns_dnspod_secret_key.clone(),
            "huawei" => s.ddns_huawei_secret_key.clone(),
            _ => String::new(),
        },
        zones: Vec::new(),
        tested_at: None,
    }
}
```

**不要**把 DDNS 改成共用站点账号——那会破坏既有的解耦决策。

- [ ] **步骤 5：运行测试验证失败（T3 的出口是预期红，不是全绿）**

运行：`cd src-tauri && cargo test --lib services::ddns`
预期：`E0046` × 5（四家 `ddns/*.rs` 各一处，加 `ddns/mod.rs:59` blanket impl 一处）+ `E0609` × 2（`acme/mod.rs` 的 `AcmeSettings.account`，T5 的活）。**这 7 处正是后续 T4/T5 的入口**，不要在本步处理。详见下方实施记录。

若报 `Fake` 的 `list_zones` 未被使用警告，忽略（trait 要求实现）。

- [ ] **步骤 6：Commit**

```bash
git add src-tauri/src/services/ddns/mod.rs src-tauri/src/services/ddns/scheduler.rs src-tauri/src/commands/config.rs
git commit -m "refactor(ddns): DdnsProvider 收窄 + blanket impl，sync_record 改默认实现"
```

> **实施记录（2026-09-17 修正）**：T3 的实际出口是「7 处预期红」，不是「全绿」。步骤 5 的原预期「PASS」是错的——收窄 `DdnsProvider` 会让四家的旧 `impl DdnsProvider for X` 报 4 处 `E0046`（缺 `find_zone`/`get_value`/`set_value`），`ddns/mod.rs:59` 的 blanket impl 报 1 处 `E0046`，加上 T5 的 2 处 `E0609`。**这正是 T4 存在的理由**：T4 把四家改挂 `DnsProvider` 后前 5 处一起消失，故 T4 不能合并进 T3（合并会让 T4 失去可验证的出口）。
>
> 另两处计划缺陷已随实施修正：
> 1. `Fake.writes` 原写 `RefCell<Vec<String>>`，但 `DnsProvider: Send + Sync` → 必须用 `Mutex`。
> 2. `sync_once` 的凭证折叠原写成函数体内联的 `let ddns_account = ...`，使 DDNS 间接依赖 `AppSettings` 全部字段。改为 `pub fn ddns_account_of(&AppSettings) -> DnsAccount`，`sync_once` 收 `(&AppSettings, &DnsAccount)`；两个调用点（`scheduler.rs`、`config.rs`）同步改。

---

## 任务 4：四家 DDNS 实现改挂 DnsProvider

**这一步只搬 `impl`，不碰签名。四个文件的签名函数与常量一行不动。**

**本任务的出口（可验证）**：T3 留下的 4 处 `E0046` 从 `aliyun.rs` / `cloudflare.rs` / `dnspod.rs` / `huawei.rs` 消失，且 `ddns/mod.rs:59` blanket impl 那 1 处也消失（四家全挂上才成立），只剩 T5 的 2 处 `E0609`。

**文件：**
- 修改：`src-tauri/src/services/ddns/cloudflare.rs`
- 修改：`src-tauri/src/services/ddns/aliyun.rs`
- 修改：`src-tauri/src/services/ddns/dnspod.rs`
- 修改：`src-tauri/src/services/ddns/huawei.rs`

- [ ] **步骤 0：补 T2 留下的正例断言**

T2 的 `provider_for_account_rejects_missing_credentials`（`acme/dns/mod.rs`）只断负例——因为那一步三家还没 `impl DnsProvider`。本任务把三家改挂完之后，回到该测试，把「凭证齐全」的正例补上：

```rust
        // T4 起三家已 impl DnsProvider，正例可以断了
        assert!(
            crate::services::acme::dns::provider_for_account(&full).is_some(),
            "{} 双凭证齐全应通过",
            p
        );
```

（`acme::dns::provider_for_account` 目前仍只 match `"cloudflare"`，所以这条正例会**失败**——**不要**去改工厂。工厂收四家是 T7 的事（那时 `acme` 侧才有必要认识四家）。如果 T7 的实现在你这次执行时已经存在，则本条适用；否则**跳过步骤 0，只保留注释**，把「补正例」记进 T7 备注。）

> **给 T7 的备注**：T7 把 `acme::dns::provider_for_account` 扩成分派四家时，记得回来把 T2 那条测试的正例断言补上。

- [ ] **步骤 1：Cloudflare**

`ddns/cloudflare.rs`：把 `use super::{BoxFuture, DdnsProvider};` 改成
`use crate::services::acme::dns::{BoxFuture, DnsProvider};`

`impl DdnsProvider for Cloudflare { fn id ... fn sync_record ... }` 整块替换为下列五方法。**保留** `API`、`Cloudflare` 结构体、`new`、`get`、`json`、`zone_id` 不动。

```rust
impl DnsProvider for Cloudflare {
    fn id(&self) -> &str {
        "cloudflare"
    }

    fn list_zones<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            const MAX_PAGES: u32 = 20;
            let mut names: Vec<String> = Vec::new();
            let mut page = 1u32;
            loop {
                let body = self
                    .get(&format!("{}/zones?per_page=50&page={}", API, page))
                    .await?;
                let arr = body["result"].as_array().cloned().unwrap_or_default();
                if arr.is_empty() {
                    break;
                }
                for z in &arr {
                    if let Some(n) = z["name"].as_str() {
                        names.push(n.to_string());
                    }
                }
                if arr.len() < 50 || page >= MAX_PAGES {
                    break;
                }
                page += 1;
            }
            Ok(names)
        })
    }

    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move {
            let names = self.list_zones().await?;
            crate::services::acme::dns::longest_zone_match(domain, &names)
                .ok_or_else(|| anyhow!("该域名不在 Cloudflare 账户的 zone 中：{}", domain))
        })
    }

    fn get_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str)
        -> BoxFuture<'a, Result<Option<String>>>
    {
        Box::pin(async move {
            let zone = self.find_zone(fqdn).await?;
            let zone_id = self.zone_id(&zone).await?;
            let body = self
                .get(&format!(
                    "{}/zones/{}/dns_records?type={}&name={}",
                    API, zone_id, rtype, fqdn
                ))
                .await?;
            Ok(body["result"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|r| r["content"].as_str())
                .map(|s| s.to_string()))
        })
    }

    fn set_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str, value: &'a str)
        -> BoxFuture<'a, Result<()>>
    {
        Box::pin(async move {
            let zone = self.find_zone(fqdn).await?;
            let zone_id = self.zone_id(&zone).await?;
            let list = self
                .get(&format!(
                    "{}/zones/{}/dns_records?type={}&name={}",
                    API, zone_id, rtype, fqdn
                ))
                .await?;
            let existing = list["result"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|r| r["id"].as_str())
                .map(|s| s.to_string());
            let body = serde_json::json!({
                "type": rtype, "name": fqdn, "content": value, "ttl": 120
            });
            let resp = match existing {
                Some(rid) => {
                    self.client
                        .put(format!("{}/zones/{}/dns_records/{}", API, zone_id, rid))
                        .bearer_auth(&self.token)
                        .json(&body)
                        .send()
                        .await?
                }
                None => {
                    self.client
                        .post(format!("{}/zones/{}/dns_records", API, zone_id))
                        .bearer_auth(&self.token)
                        .json(&body)
                        .send()
                        .await?
                }
            };
            Self::json(resp).await.map(|_| ())
        })
    }

    fn delete_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str)
        -> BoxFuture<'a, Result<()>>
    {
        Box::pin(async move {
            let zone = self.find_zone(fqdn).await?;
            let zone_id = self.zone_id(&zone).await?;
            let body = self
                .get(&format!(
                    "{}/zones/{}/dns_records?type={}&name={}",
                    API, zone_id, rtype, fqdn
                ))
                .await?;
            if let Some(records) = body["result"].as_array() {
                for r in records {
                    let Some(id) = r["id"].as_str() else { continue };
                    match self
                        .client
                        .delete(format!("{}/zones/{}/dns_records/{}", API, zone_id, id))
                        .bearer_auth(&self.token)
                        .send()
                        .await
                    {
                        Ok(resp) if resp.status().is_success() => {}
                        Ok(resp) => tracing::warn!(
                            status = %resp.status(), name = %fqdn, "删除 DNS 记录失败（响应非成功）"
                        ),
                        Err(e) => tracing::warn!(
                            error = %e, name = %fqdn, "删除 DNS 记录请求失败"
                        ),
                    }
                }
            }
            Ok(())
        })
    }
}
```

- [ ] **步骤 2：阿里云**

`ddns/aliyun.rs`：`use super::{BoxFuture, DdnsProvider};` → `use crate::services::acme::dns::{BoxFuture, DnsProvider};`

`impl DdnsProvider for Aliyun` 整块替换。`percent_encode`、`rpc_signature`、`API`、`Aliyun` 结构体、`new`、`call`、`find_zone`（作为私有方法保留，供 `find_zone` trait 方法复用）都不动。**保留现有全部测试。**

```rust
impl DnsProvider for Aliyun {
    fn id(&self) -> &str {
        "aliyun"
    }

    fn list_zones<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            // ponytail: 首页 100；更多请用 DomainName 关键字过滤
            let body = self
                .call("DescribeDomains", &[("PageSize", "100".into())])
                .await?;
            Ok(body["Domains"]["Domain"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|d| d["DomainName"].as_str().map(String::from))
                .collect())
        })
    }

    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move { Aliyun::find_zone(self, domain).await })
    }

    /// 返回 (RecordId, Value)：无记录 → None
    fn get_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str)
        -> BoxFuture<'a, Result<Option<String>>>
    {
        Box::pin(async move {
            let list = self
                .call(
                    "DescribeSubDomainRecords",
                    &[("SubDomain", fqdn.to_string()), ("Type", rtype.to_string())],
                )
                .await?;
            Ok(list["DomainRecords"]["Record"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|r| r["Value"].as_str())
                .map(|s| s.to_string()))
        })
    }

    fn set_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str, value: &'a str)
        -> BoxFuture<'a, Result<()>>
    {
        Box::pin(async move {
            let zone = Aliyun::find_zone(self, fqdn).await?;
            let rr = fqdn
                .strip_suffix(&format!(".{}", zone))
                .unwrap_or("@")
                .to_string();
            let rr = if rr.is_empty() { "@".to_string() } else { rr };
            let list = self
                .call(
                    "DescribeSubDomainRecords",
                    &[("SubDomain", fqdn.to_string()), ("Type", rtype.to_string())],
                )
                .await?;
            let existing = list["DomainRecords"]["Record"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|r| r["RecordId"].as_str())
                .map(|s| s.to_string());
            match existing {
                Some(rid) => {
                    self.call(
                        "UpdateDomainRecord",
                        &[
                            ("RecordId", rid),
                            ("RR", rr),
                            ("Type", rtype.to_string()),
                            ("Value", value.to_string()),
                        ],
                    )
                    .await?;
                }
                None => {
                    self.call(
                        "AddDomainRecord",
                        &[
                            ("DomainName", zone),
                            ("RR", rr),
                            ("Type", rtype.to_string()),
                            ("Value", value.to_string()),
                        ],
                    )
                    .await?;
                }
            }
            Ok(())
        })
    }

    fn delete_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str)
        -> BoxFuture<'a, Result<()>>
    {
        Box::pin(async move {
            let list = self
                .call(
                    "DescribeSubDomainRecords",
                    &[("SubDomain", fqdn.to_string()), ("Type", rtype.to_string())],
                )
                .await?;
            let ids: Vec<String> = list["DomainRecords"]["Record"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|r| r["RecordId"].as_str().map(String::from))
                .collect();
            for rid in ids {
                self.call("DeleteDomainRecord", &[("RecordId", rid)]).await?;
            }
            Ok(())
        })
    }
}
```

把原先那个私有 `async fn find_zone(&self, fqdn: &str) -> Result<String>` 改名为 `fn find_zone_impl` 会与 trait 方法同名冲突——**不发生**：trait 方法接收 `&'a self, domain: &'a str`，与私有同名方法签名冲突。因此把私有方法**重命名为 `zone_of`**，`list_zones` / trait `find_zone` / `set_value` 三处都改调 `self.zone_of(...)`：

```rust
    /// 列账户域名（首页 100；ponytail: 更多请用 DomainName 过滤）
    async fn zone_of(&self, fqdn: &str) -> Result<String> {
        let names = self.list_zones().await?;
        crate::services::acme::dns::longest_zone_match(fqdn, &names)
            .ok_or_else(|| anyhow!("该域名不在阿里云账户的解析中：{}", fqdn))
    }

    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move { self.zone_of(domain).await })
    }
```

`set_value` 里改成 `let zone = self.zone_of(fqdn).await?;`。

- [ ] **步骤 3：DNSPod**

`ddns/dnspod.rs`：`use super::{BoxFuture, DdnsProvider};` → `use crate::services::acme::dns::{BoxFuture, DnsProvider};`

私有 `find_zone` 重命名为 `zone_of`（同阿里云）。`SERVICE`/`HOST`/`VERSION`/`CONTENT_TYPE`、`hex`、`hmac_sha256`、`tc3_canonical_request`、`tc3_authorization`、`Dnspod::new`、`call` **一行不动**，**保留全部 5 个测试**。

`impl DdnsProvider for Dnspod` 整块替换为：

```rust
impl DnsProvider for Dnspod {
    fn id(&self) -> &str {
        "dnspod"
    }

    fn list_zones<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            // ponytail: 首页 100；更多请用 DescribeDomain 精确查询
            let body = self
                .call(
                    "DescribeDomainList",
                    serde_json::json!({ "Offset": 0, "Limit": 100 }),
                )
                .await?;
            Ok(body["DomainList"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|d| d["Name"].as_str().map(String::from))
                .collect())
        })
    }

    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move { self.zone_of(domain).await })
    }

    fn get_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str)
        -> BoxFuture<'a, Result<Option<String>>>
    {
        Box::pin(async move {
            let zone = self.zone_of(fqdn).await?;
            let sub = sub_of(fqdn, &zone);
            let list = self
                .call(
                    "DescribeRecordList",
                    serde_json::json!({
                        "Domain": zone, "SubDomain": sub, "RecordType": rtype
                    }),
                )
                .await;
            match list {
                Ok(v) => Ok(v["RecordList"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|r| r["Value"].as_str())
                    .map(|s| s.to_string())),
                // DNSPod：该 name+type 无记录时报 ResourceNotFound.NoDataOfRecord（实测值）
                Err(e)
                    if e.to_string().contains("NoDataOfRecord")
                        || e.to_string().contains("NoFoundData") =>
                {
                    Ok(None)
                }
                Err(e) => Err(e),
            }
        })
    }

    fn set_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str, value: &'a str)
        -> BoxFuture<'a, Result<()>>
    {
        Box::pin(async move {
            let zone = self.zone_of(fqdn).await?;
            let sub = sub_of(fqdn, &zone);
            let list = self
                .call(
                    "DescribeRecordList",
                    serde_json::json!({
                        "Domain": zone, "SubDomain": sub, "RecordType": rtype
                    }),
                )
                .await;
            let existing = match list {
                Ok(v) => v["RecordList"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|r| r["RecordId"].as_u64()),
                Err(e)
                    if e.to_string().contains("NoDataOfRecord")
                        || e.to_string().contains("NoFoundData") =>
                {
                    None
                }
                Err(e) => return Err(e),
            };
            match existing {
                Some(rid) => {
                    self.call(
                        "ModifyRecord",
                        serde_json::json!({
                            "Domain": zone, "RecordId": rid, "SubDomain": sub,
                            "RecordType": rtype, "RecordLine": "默认", "Value": value
                        }),
                    )
                    .await?;
                }
                None => {
                    self.call(
                        "CreateRecord",
                        serde_json::json!({
                            "Domain": zone, "SubDomain": sub, "RecordType": rtype,
                            "RecordLine": "默认", "Value": value
                        }),
                    )
                    .await?;
                }
            }
            Ok(())
        })
    }

    fn delete_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str)
        -> BoxFuture<'a, Result<()>>
    {
        Box::pin(async move {
            let zone = self.zone_of(fqdn).await?;
            let sub = sub_of(fqdn, &zone);
            let list = self
                .call(
                    "DescribeRecordList",
                    serde_json::json!({
                        "Domain": zone, "SubDomain": sub, "RecordType": rtype
                    }),
                )
                .await;
            let ids: Vec<u64> = match list {
                Ok(v) => v["RecordList"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|r| r["RecordId"].as_u64())
                    .collect(),
                Err(e)
                    if e.to_string().contains("NoDataOfRecord")
                        || e.to_string().contains("NoFoundData") =>
                {
                    Vec::new()
                }
                Err(e) => return Err(e),
            };
            for rid in ids {
                self.call(
                    "DeleteRecord",
                    serde_json::json!({ "Domain": zone, "RecordId": rid }),
                )
                .await?;
            }
            Ok(())
        })
    }
}
```

`sub_of` 是新增的自由函数（替换原先散在两处的 `strip_suffix` 逻辑）：

```rust
/// DNSPod 的子域名：fqdn 相对 zone 的部分，根域用 "@"。
/// （qname 比对要转小写——zone 来自 API 列表，大小写不保证一致）
fn sub_of(fqdn: &str, zone: &str) -> String {
    let s = fqdn
        .strip_suffix(&format!(".{}", zone))
        .unwrap_or("@")
        .to_string();
    if s.is_empty() {
        "@".to_string()
    } else {
        s
    }
}
```

并在 `mod tests` 追加：

```rust
    /// 子域名拆解：根域 → "@"，子域 → 相对部分，大小写差异不应误判。
    /// 这条锁住的是 set/delete 共用的路径——拆错就会写到别的记录上。
    #[test]
    fn sub_of_handles_root_and_subdomain() {
        assert_eq!(sub_of("example.com", "example.com"), "@");
        assert_eq!(sub_of("a.example.com", "example.com"), "a");
        assert_eq!(sub_of("a.b.example.com", "example.com"), "a.b");
        // 不匹配的 zone：保守返回 "@"（调用方已由 zone_of 保证匹配，此处仅防呆）
        assert_eq!(sub_of("example.com", "other.com"), "@");
    }
```

- [ ] **步骤 4：华为**

`ddns/huawei.rs`：`use super::{BoxFuture, DdnsProvider};` → `use crate::services::acme::dns::{BoxFuture, DnsProvider};`

私有 `find_zone`（返回 `(zone, zone_id)`）**保留原名**（它的签名是 `(&self, &str) -> Result<(String, String)>`，与 trait 的 `-> Result<String>` 不冲突，不会重名报错）。`HOST`、`hex`、`hws_canonical_request`、`hws_authorization`、`pe`、`record_name`、`Huawei::new`、`call` **一行不动**，**保留全部测试**。

`impl DdnsProvider for Huawei` 整块替换为：

```rust
impl DnsProvider for Huawei {
    fn id(&self) -> &str {
        "huawei"
    }

    fn list_zones<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            // ponytail: zone 列表留 v2 —— 仅能确认同组 zone 接口（POST /v2/zones、
            // GET /v2/zones/{zone_id}）为 v2，ListPublicZones 文档页取不到。
            // 若首次查找即 404，就是这里：改 /v2.1/zones（同记录集一处前缀）。
            let body = self
                .call("GET", "/v2/zones", &[("limit", "100".into())], None)
                .await?;
            Ok(body["zones"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|z| z["name"].as_str().map(String::from))
                .collect())
        })
    }

    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move { Ok(Huawei::find_zone(self, domain).await?.0) })
    }

    fn get_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str)
        -> BoxFuture<'a, Result<Option<String>>>
    {
        Box::pin(async move {
            let (_zone, zid) = Huawei::find_zone(self, fqdn).await?;
            let name = record_name(fqdn); // 华为记录名带尾点
            let list = self
                .call(
                    "GET",
                    &format!("/v2.1/zones/{}/recordsets", zid),
                    &[("type", rtype.to_string()), ("name", name)],
                    None,
                )
                .await?;
            Ok(list["recordsets"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|r| r["records"].as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()))
        })
    }

    fn set_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str, value: &'a str)
        -> BoxFuture<'a, Result<()>>
    {
        Box::pin(async move {
            let (_zone, zid) = Huawei::find_zone(self, fqdn).await?;
            let name = record_name(fqdn);
            let list = self
                .call(
                    "GET",
                    &format!("/v2.1/zones/{}/recordsets", zid),
                    &[("type", rtype.to_string()), ("name", name.clone())],
                    None,
                )
                .await?;
            let existing = list["recordsets"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|r| r["id"].as_str())
                .map(|s| s.to_string());
            let body = serde_json::json!({
                "name": name, "type": rtype, "ttl": 300, "records": [value]
            });
            match existing {
                Some(rid) => {
                    self.call(
                        "PUT",
                        &format!("/v2.1/zones/{}/recordsets/{}", zid, rid),
                        &[],
                        Some(&body),
                    )
                    .await?;
                }
                None => {
                    self.call(
                        "POST",
                        &format!("/v2.1/zones/{}/recordsets", zid),
                        &[],
                        Some(&body),
                    )
                    .await?;
                }
            }
            Ok(())
        })
    }

    fn delete_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str)
        -> BoxFuture<'a, Result<()>>
    {
        Box::pin(async move {
            let (_zone, zid) = Huawei::find_zone(self, fqdn).await?;
            let name = record_name(fqdn);
            let list = self
                .call(
                    "GET",
                    &format!("/v2.1/zones/{}/recordsets", zid),
                    &[("type", rtype.to_string()), ("name", name)],
                    None,
                )
                .await?;
            let ids: Vec<String> = list["recordsets"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|r| r["id"].as_str().map(String::from))
                .collect();
            for rid in ids {
                self.call(
                    "DELETE",
                    &format!("/v2.1/zones/{}/recordsets/{}", zid, rid),
                    &[],
                    None,
                )
                .await?;
            }
            Ok(())
        })
    }
}
```

- [ ] **步骤 5：运行测试**

运行：`cd src-tauri && cargo test --lib services::ddns`
预期：PASS。四家原有的签名测试（DNSPod 的 5 条含独立重算链、华为/阿里云的测试）**全部原样通过**——如果它们需要改动才能过，说明签名层被误改了，**回退重做**。

- [ ] **步骤 6：Commit**

```bash
git add src-tauri/src/services/ddns/cloudflare.rs src-tauri/src/services/ddns/aliyun.rs src-tauri/src/services/ddns/dnspod.rs src-tauri/src/services/ddns/huawei.rs
git commit -m "refactor(ddns): 四家实现改挂 DnsProvider（签名层零改动）"
```

---

## 任务 5：acme 签发流程按账号 + 清理语义

**文件：**
- 修改：`src-tauri/src/services/acme/mod.rs`

- [ ] **步骤 1：写失败的测试**

在 `src-tauri/src/services/acme/mod.rs` 的 `mod tests` 追加：

```rust
    /// 清理计划：记录原本存在 → 还原旧值；原本不存在 → 删除。
    /// 这条锁死「OPX 自己造的 TXT 必须被清掉」——早先的设想是用 set_value 写回旧值，
    /// 那会让原本不存在的记录永久留在用户 DNS 里。
    #[test]
    fn cleanup_plan_restores_or_deletes() {
        #[derive(Debug, PartialEq)]
        enum Action {
            Restore(String),
            Delete,
        }
        let plan = |before: Option<&str>| match before {
            Some(v) => Action::Restore(v.to_string()),
            None => Action::Delete,
        };
        assert_eq!(plan(Some("old-value")), Action::Restore("old-value".into()));
        assert_eq!(plan(None), Action::Delete);
    }
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test --lib services::acme::mod`
预期：FAIL —— `AcmeSettings` 仍是旧字段，引用 `settings.dns_provider` 报错。

- [ ] **步骤 3：改 `AcmeSettings` 与签发主体**

`AcmeSettings` 结构体替换为：

```rust
pub struct AcmeSettings {
    /// 该站点的 DNS 账号（服务商 + 凭证）
    pub account: crate::models::dns_account::DnsAccount,
    pub use_staging: bool,
}
```

`issue_certificate` 里 provider 获取改为：

```rust
    let provider =
        provider_for_account(&settings.account).ok_or_else(|| {
            anyhow!(
                "DNS 账号凭证不完整或服务商不支持：{}",
                settings.account.provider
            )
        })?;
```

`use dns::provider_for;` 改为 `use dns::provider_for_account;`。

把 `let mut created: Vec<(String, String)> = Vec::new();` 改为记录清理计划：

```rust
    // 清理计划：记录原本就存在的旧值（None 表示原本无记录）。
    // 验证结束后按此还原/删除——否则每签发一次就在用户 DNS 里残留一条 TXT。
    let mut cleanup: Vec<(String, Option<String>)> = Vec::new();
```

把三处 `created.push((fqdn, value));` 形态与 `create_txt` 调用改为：

```rust
        let before = provider.get_value(&fqdn, "TXT").await?;
        on_progress("waiting-dns", &format!("写入 TXT 记录 {}", fqdn));
        if let Err(e) = provider.set_value(&fqdn, "TXT", &value).await {
            for (f, old) in &cleanup {
                restore_txt(&*provider, f, old.as_deref()).await;
            }
            return Err(e).context("创建 DNS 挑战记录失败");
        }
        cleanup.push((fqdn.clone(), before));
```

（`on_progress("waiting-dns", ...)` 那行原本在 `create_txt` 之前，移到这里保持原有的进度文案顺序。）

新增自由函数：

```rust
/// 按清理计划还原或删除一条 TXT（失败只记日志，不阻断证书流程）。
async fn restore_txt(provider: &dyn dns::DnsProvider, fqdn: &str, before: Option<&str>) {
    let r = match before {
        Some(old) => provider.set_value(fqdn, "TXT", old).await,
        None => provider.delete_value(fqdn, "TXT").await,
    };
    if let Err(e) = r {
        // {:#} 展开 error chain，便于定位真实原因（如权限不足）
        tracing::warn!(name = %fqdn, error = %format!("{:#}", e), "清理 DNS 挑战记录失败");
    }
}
```

三处清理循环（`create_txt` 失败后的回滚、`set_ready` 失败后的回滚、验证结束后的统一清理）全部改为：

```rust
    for (f, old) in &cleanup {
        restore_txt(&*provider, f, old.as_deref()).await;
    }
```

其中验证结束那处是**必须执行**（原来写成 `let _ = provider.delete_txt(f, v).await;` 忽略错误），现在由 `restore_txt` 内部记 warn。

- [ ] **步骤 4：运行测试验证通过**

运行：`cd src-tauri && cargo test --lib services::acme`
预期：PASS（含原有 `directory_url_switches_on_staging`、`cert_paths_use_domain` 与新增 `cleanup_plan_restores_or_deletes`）。

- [ ] **步骤 5：Commit**

```bash
git add src-tauri/src/services/acme/mod.rs
git commit -m "feat(acme): 签发按账号取凭证；清理改为还原/删除（不再用写回代替删除）"
```

---

## 任务 6：续期调度器按站点账号

**文件：**
- 修改：`src-tauri/src/services/acme/renew_scheduler.rs`
- 修改：`src-tauri/src/commands/website.rs`（`list_account_refs` 命令）

- [ ] **步骤 1：写失败的测试**

`renew_scheduler.rs` 的 `mod tests` 追加（现有 `needs_renewal_rules` 保留）：

```rust
    /// 站点该用哪个账号：绑了就用绑的；没绑就是配置缺失，必须跳过而不是
    /// 拿某个默认账号去签（会给用户搞出意外的证书）。
    #[test]
    fn account_resolution_requires_explicit_binding() {
        let accounts = vec![crate::models::dns_account::DnsAccount {
            id: "acc-1".into(),
            name: "n".into(),
            provider: "cloudflare".into(),
            token: "t".into(),
            access_key_id: String::new(),
            access_key_secret: String::new(),
            zones: vec![],
            tested_at: None,
        }];
        let pick = |bound: Option<&str>| -> Option<String> {
            let id = bound?;
            accounts.iter().find(|a| a.id == id).map(|a| a.id.clone())
        };
        assert_eq!(pick(Some("acc-1")), Some("acc-1".into()));
        // 未绑定 → None（跳过并 warn）
        assert_eq!(pick(None), None);
        // 绑了但账号已删 → None（跳过并 warn，而不是退回默认）
        assert_eq!(pick(Some("gone")), None);
    }
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test --lib services::acme::renew_scheduler`
预期：FAIL（`AcmeSettings` 字段变更导致该文件编译失败）。

- [ ] **步骤 3：改造调度器**

`run_scheduler` 的签名增加账号管理器：

```rust
pub async fn run_scheduler(
    _app: AppHandle,
    wm: Arc<WebsiteManager>,
    sm: Arc<SoftwareManager>,
    dns_accounts: Arc<crate::services::dns_account::DnsAccountManager>,
) {
```

循环体里删除一次性构造 `acme` 的代码（原 33-37 行的 `let acme = AcmeSettings { ... };`），改为逐站点解析：

```rust
        let accounts = dns_accounts.list();
        for site in wm.list().into_iter().filter(|s| s.ssl.acme) {
            if !needs_renewal(site.ssl.cert_expires_at.as_deref(), chrono::Local::now(), RENEW_BEFORE_DAYS) {
                continue;
            }
            // 必须显式绑定账号：未绑/账号已删都跳过并留痕，
            // 绝不退回某个「默认」账号——那会签出用户没预期的证书。
            let Some(account) = site
                .ssl
                .dns_account_id
                .as_deref()
                .and_then(|id| accounts.iter().find(|a| a.id == id))
            else {
                tracing::warn!(
                    site = %site.id,
                    bound = ?site.ssl.dns_account_id,
                    "站点未绑定有效的 DNS 账号，跳过续期"
                );
                continue;
            };
            let acme = crate::services::acme::AcmeSettings {
                account: account.clone(),
                use_staging: settings.acme_use_staging,
            };
```

（`settings` 那行仍在循环外读取——它现在只用来取 `acme_use_staging`。）

文件顶部的 `use` 增加 `use crate::services::dns_account::DnsAccountManager;`（若用全路径则不必）。

- [ ] **步骤 4：加 `list_account_refs` 命令**

在 `src-tauri/src/commands/website.rs` 追加（供删除账号时判断是否被引用）：

```rust
/// 返回所有站点的 (站点 id, 站点名, 绑定的账号 id)，供账号删除前的引用检查。
#[tauri::command]
pub fn list_account_refs(wm: State<'_, Arc<WebsiteManager>>) -> Vec<(String, String, String)> {
    wm.list()
        .into_iter()
        .filter_map(|s| {
            s.ssl
                .dns_account_id
                .map(|aid| (s.id.clone(), s.name.clone(), aid))
        })
        .collect()
}
```

同文件 `issue_site_certificate` 里那两行全局设置构造改为按站点取账号：

```rust
        let accounts = dns_accounts.list();
        let account = site
            .ssl
            .dns_account_id
            .as_deref()
            .and_then(|id| accounts.iter().find(|a| a.id == id))
            .cloned()
            .ok_or_else(|| "请先在站点 SSL 配置里选择 DNS 账号".to_string())?;
        let acme_settings = crate::services::acme::AcmeSettings {
            account,
            use_staging: crate::commands::config::read_settings()?.acme_use_staging,
        };
```

并把命令签名加上 `dns_accounts: State<'_, Arc<crate::services::dns_account::DnsAccountManager>>,`。原先那句 `let settings = crate::commands::config::read_settings()?;` 与其后两行（`dns_provider`/`cloudflare_api_token`）删除。

- [ ] **步骤 5：运行测试**

运行：`cd src-tauri && cargo test --lib services::acme`
预期：PASS。

- [ ] **步骤 6：Commit**

```bash
git add src-tauri/src/services/acme/renew_scheduler.rs src-tauri/src/commands/website.rs
git commit -m "feat(acme): 续期与签发按站点绑定的 DNS 账号取凭证"
```

---

## 任务 7：DnsAccountManager + 迁移 + 命令层接线

**本任务结束后全量可编译。**

**文件：**
- 创建：`src-tauri/src/services/dns_account/mod.rs`
- 创建：`src-tauri/src/services/dns_account/migrate.rs`
- 创建：`src-tauri/src/commands/dns_account.rs`
- 修改：`src-tauri/src/services/mod.rs`、`src-tauri/src/commands/mod.rs`
- 修改：`src-tauri/src/models/website.rs`（`dns_account_id`）
- 修改：`src-tauri/src/models/settings.rs`（删两个字段）
- 修改：`src-tauri/src/commands/config.rs`（删 `test_dns_token`）
- 修改：`src-tauri/src/lib.rs`

- [ ] **步骤 1：写失败的测试（迁移逻辑）**

创建 `src-tauri/src/services/dns_account/migrate.rs`，只写自由函数与测试——**不碰文件系统**，把「读」与「写」留给调用方，逻辑才可测：

```rust
//! 旧全局 DNS 配置 → DNS 账号的一次性迁移。
//!
//! 迁移信号是「dns-accounts.json 不存在」，不是「旧字段有值」：
//! 必须区分「从没配过」（不建账号）与「迁移后用户删光了账号」
//! （不能每次启动又冒出来一个）。

use crate::models::dns_account::DnsAccount;

/// 迁移决策。
#[derive(Debug, PartialEq)]
pub enum Plan {
    /// 账号文件已存在：什么都不做
    AlreadyDone,
    /// 建一条账号（携带可直接落盘的 DnsAccount）
    Create(DnsAccount),
    /// 旧配置为空：只标记「迁移已完成」，不建账号
    MarkOnly,
}

/// 给定「账号文件是否已存在」与「旧 settings 里的 cloudflare token」，决定怎么迁。
/// `id` 由调用方生成（便于测试注入固定值）。
pub fn plan(account_file_exists: bool, legacy_token: &str, id: String) -> Plan {
    if account_file_exists {
        return Plan::AlreadyDone;
    }
    let token = legacy_token.trim();
    if token.is_empty() {
        return Plan::MarkOnly;
    }
    Plan::Create(DnsAccount {
        id,
        name: "默认 Cloudflare".to_string(),
        provider: "cloudflare".to_string(),
        token: token.to_string(),
        access_key_id: String::new(),
        access_key_secret: String::new(),
        zones: Vec::new(),
        tested_at: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn already_migrated_file_is_left_alone() {
        // 哪怕旧 token 还有值，文件存在就绝不重复迁移
        assert_eq!(plan(true, "tok", "x".into()), Plan::AlreadyDone);
        assert_eq!(plan(true, "", "x".into()), Plan::AlreadyDone);
    }

    #[test]
    fn creates_account_when_legacy_token_present() {
        match plan(false, "tok-123", "acc-1".into()) {
            Plan::Create(a) => {
                assert_eq!(a.id, "acc-1");
                assert_eq!(a.provider, "cloudflare");
                assert_eq!(a.token, "tok-123");
                assert_eq!(a.name, "默认 Cloudflare");
                assert!(a.zones.is_empty());
                assert!(a.tested_at.is_none());
            }
            other => panic!("应建账号，实际 {:?}", other),
        }
    }

    #[test]
    fn marks_only_when_legacy_token_blank() {
        assert_eq!(plan(false, "", "x".into()), Plan::MarkOnly);
        // 纯空白也算空，别建出一个 token 全空格的账号
        assert_eq!(plan(false, "   ", "x".into()), Plan::MarkOnly);
    }

    #[test]
    fn token_is_trimmed() {
        match plan(false, "  tok  ", "acc-1".into()) {
            Plan::Create(a) => assert_eq!(a.token, "tok"),
            other => panic!("应建账号，实际 {:?}", other),
        }
    }
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test --lib services::dns_account`
预期：FAIL —— `services/mod.rs` 未声明 `pub mod dns_account;`。

- [ ] **步骤 3：声明模块，跑绿**

`src-tauri/src/services/mod.rs` 加 `pub mod dns_account;`（按字母序，在 `pub mod ddns;` 附近）。

运行：`cd src-tauri && cargo test --lib services::dns_account`
预期：PASS，4 个测试。

- [ ] **步骤 4：写 Manager（含测试）**

创建 `src-tauri/src/services/dns_account/mod.rs`：

```rust
//! DNS 服务商账号管理（新增/编辑/删除/被引用检查）。

pub mod migrate;

use std::path::PathBuf;
use std::sync::RwLock;

use anyhow::Result;

use crate::models::dns_account::DnsAccount;
use crate::utils::paths;

pub struct DnsAccountManager {
    accounts: RwLock<Vec<DnsAccount>>,
}

impl DnsAccountManager {
    pub fn new() -> Self {
        Self {
            accounts: RwLock::new(Self::load().unwrap_or_default()),
        }
    }

    pub fn store_path() -> PathBuf {
        paths::config_dir().join("dns-accounts.json")
    }

    fn load() -> Result<Vec<DnsAccount>> {
        let p = Self::store_path();
        if !p.exists() {
            return Ok(Vec::new());
        }
        Ok(serde_json::from_str(&std::fs::read_to_string(&p)?)?)
    }

    fn save(list: &[DnsAccount]) -> Result<()> {
        std::fs::write(Self::store_path(), serde_json::to_string_pretty(list)?)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<DnsAccount> {
        self.accounts.read().unwrap().clone()
    }

    pub fn get(&self, id: &str) -> Option<DnsAccount> {
        self.accounts
            .read()
            .unwrap()
            .iter()
            .find(|a| a.id == id)
            .cloned()
    }

    /// 新增或更新（按 id 匹配）并落盘。
    pub fn upsert(&self, a: DnsAccount) -> Result<()> {
        let mut l = self.accounts.write().unwrap();
        match l.iter_mut().find(|x| x.id == a.id) {
            Some(slot) => *slot = a,
            None => l.push(a),
        }
        Self::save(&l)
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let mut l = self.accounts.write().unwrap();
        l.retain(|a| a.id != id);
        Self::save(&l)
    }
}

impl Default for DnsAccountManager {
    fn default() -> Self {
        Self::new()
    }
}
```

> **不要加 `clear_binding` 之类的「清空绑定」方法。** 站点归属 `WebsiteManager`，删除账号时的引用检查与拦截全在命令层（步骤 6 的 `delete_dns_account`）——因为本次的策略是**被引用就拒绝删除**，不存在「删账号同时清站点绑定」这条路径。留一个空壳方法只会让人以为它有用。

在 `mod tests` 里加两条纯逻辑测试（不碰文件系统，直接构造 manager 的内存部分不可行——`load()` 读真实路径，故只测可测的部分）：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// store_path 必须落在 config 目录、文件名固定（改错目录会让数据丢失）
    #[test]
    fn store_path_is_in_config_dir() {
        let p = DnsAccountManager::store_path();
        assert_eq!(p.file_name().unwrap(), "dns-accounts.json");
        assert_eq!(p.parent().unwrap(), paths::config_dir());
    }

    /// 账号列表的序列化形状：Vec<DnsAccount> 的 JSON 必须是数组（不是对象）
    #[test]
    fn serializes_as_array() {
        let list = vec![DnsAccount {
            id: "a1".into(),
            name: "n".into(),
            provider: "cloudflare".into(),
            token: "t".into(),
            access_key_id: String::new(),
            access_key_secret: String::new(),
            zones: vec![],
            tested_at: None,
        }];
        let json = serde_json::to_string(&list).unwrap();
        assert!(json.starts_with('['), "必须是数组：{}", json);
        let back: Vec<DnsAccount> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].id, "a1");
    }
}
```

- [ ] **步骤 5：加 `SslConfig.dns_account_id` 与删 `AppSettings` 两字段**

`src-tauri/src/models/website.rs` 的 `SslConfig` 末尾追加：

```rust
    /// 签发/续期该站点证书所用的 DNS 账号 id（见 `models::dns_account`）。
    /// Option + skip：老 websites.json 反序列化后为 None，且不会因升级被改写出新字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_account_id: Option<String>,
```

同文件 `mod tests` 追加：

```rust
    /// 零迁移：老的 ssl 对象（无 dns_account_id）必须能读，且为 None
    #[test]
    fn ssl_config_without_account_id_defaults_to_none() {
        let c: SslConfig = serde_json::from_str(
            r#"{"enabled":true,"cert_path":"a.crt","key_path":"a.key","acme":true}"#,
        )
        .unwrap();
        assert!(c.acme);
        assert!(c.dns_account_id.is_none());
    }

    /// None 不写进 JSON：未被编辑的站点不会因为升级而被改写
    #[test]
    fn ssl_config_omits_none_account_id() {
        let c = SslConfig {
            enabled: true,
            cert_path: None,
            key_path: None,
            acme: false,
            cert_expires_at: None,
            dns_account_id: None,
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(!json.contains("dns_account_id"), "None 不应序列化：{}", json);
    }
```

`src-tauri/src/models/settings.rs`：删除 `dns_provider` 与 `cloudflare_api_token` 两个字段、`default_dns_provider()` 函数、`Default` impl 里对应的两行。同文件 `mod tests` 里的 `assert_eq!(s.dns_provider, "cloudflare");` 与 `assert!(s.cloudflare_api_token.is_empty());` 两行**删除**。

`src-tauri/src/commands/config.rs`：整个 `test_dns_token` 命令（含其文档注释）**删除**。

- [ ] **步骤 6：命令层**

创建 `src-tauri/src/commands/dns_account.rs`：

```rust
use std::sync::Arc;

use tauri::State;

use crate::models::dns_account::DnsAccount;
use crate::services::acme::dns::provider_for_account;
use crate::services::dns_account::DnsAccountManager;
use crate::services::website_manager::WebsiteManager;
use crate::{audited, audited_async};

#[tauri::command]
pub fn list_dns_accounts(m: State<'_, Arc<DnsAccountManager>>) -> Vec<DnsAccount> {
    m.list()
}

#[tauri::command]
pub fn save_dns_account(
    m: State<'_, Arc<DnsAccountManager>>,
    account: DnsAccount,
) -> Result<(), String> {
    let target = format!("{} ({})", account.name, account.provider);
    audited!("save_dns_account", target, "", {
        m.upsert(account).map_err(|e| format!("{:#}", e))
    })
}

/// 删除账号；仍被站点引用时拒绝（否则那些站点会突然签不出证书）。
#[tauri::command]
pub fn delete_dns_account(
    m: State<'_, Arc<DnsAccountManager>>,
    wm: State<'_, Arc<WebsiteManager>>,
    id: String,
) -> Result<(), String> {
    let target = id.clone();
    audited!("delete_dns_account", target, "", {
        let refs: Vec<String> = wm
            .list()
            .into_iter()
            .filter(|s| s.ssl.dns_account_id.as_deref() == Some(id.as_str()))
            .map(|s| s.name)
            .collect();
        if !refs.is_empty() {
            return Err(format!(
                "该账号仍被 {} 个站点引用：{}。请先在站点里改掉再删除。",
                refs.len(),
                refs.join("、")
            ));
        }
        m.remove(&id).map_err(|e| format!("{:#}", e))
    })
}

/// 测试账号连通：拉 zone 列表并缓存（只验读权限；写权限留到签发时暴露）。
#[tauri::command]
pub async fn test_dns_account(
    m: State<'_, Arc<DnsAccountManager>>,
    id: String,
) -> Result<Vec<String>, String> {
    let target = id.clone();
    audited_async!("test_dns_account", target, "", {
        let mut account = m.get(&id).ok_or_else(|| format!("未找到账号: {}", id))?;
        let provider = provider_for_account(&account)
            .ok_or_else(|| "凭证不完整或服务商不支持".to_string())?;
        let zones = provider.list_zones().await.map_err(|e| format!("{:#}", e))?;
        account.zones = zones.clone();
        account.tested_at = Some(chrono::Local::now().to_rfc3339());
        m.upsert(account).map_err(|e| format!("{:#}", e))?;
        Ok(zones)
    })
}
```

（`audited_async!` 的用法照 `commands/config.rs:39` 的原 `test_dns_token` 写法；若宏展开要求 block 返回 `Result<_, String>`，此处已满足。）

`src-tauri/src/commands/mod.rs` 加 `pub mod dns_account;`。

- [ ] **步骤 7：lib.rs 接线（迁移 + State + 命令）**

在 `src-tauri/src/lib.rs` 的 `setup` 里，**`WebsiteManager::new()` 那一行之前**插入迁移。找到 `app.manage(std::sync::Arc::new(crate::services::website_manager::WebsiteManager::new()));`，在其前插入：

```rust
            // 旧全局 DNS 配置 → DNS 账号（一次性）。必须在 WebsiteManager::new()
            // 之前：迁移会给 websites.json 写 dns_account_id，晚了就落不进内存。
            if let Err(e) = crate::services::dns_account::run_startup_migration() {
                tracing::warn!(error = %format!("{:#}", e), "DNS 账号迁移失败（已跳过）");
            }
```

在 `WebsiteManager` 注册之后加：

```rust
            let dns_account_mgr =
                std::sync::Arc::new(crate::services::dns_account::DnsAccountManager::new());
            app.manage(dns_account_mgr.clone());
```

命令注册：在 `invoke_handler` 的 `generate_handler!` 列表里加 `commands::dns_account::{list_dns_accounts, save_dns_account, delete_dns_account, test_dns_account}`、`commands::website::list_account_refs`，并**删除** `commands::config::test_dns_token`（如果列表里有）。

`renew_scheduler` 的 spawn 处补第四个参数（传 `dns_account_mgr.clone()`）。找不到该 spawn 时用 `grep -n "run_scheduler" src-tauri/src/lib.rs` 定位。

- [ ] **步骤 8：在 manager 模块里补迁移执行函数**

`src-tauri/src/services/dns_account/mod.rs` 末尾追加（这一步涉及真实文件读写，逻辑已由 `migrate::plan` 的单测覆盖）：

```rust
/// 执行一次性迁移：把旧 settings 里的 cloudflare token 变成一条账号，
/// 并绑定所有已启用 ACME 的站点。
pub fn run_startup_migration() -> anyhow::Result<()> {
    use anyhow::Context;

    let path = DnsAccountManager::store_path();
    let legacy_token = read_legacy_token();
    let id = format!("acc-{}", uuid::Uuid::new_v4());

    match migrate::plan(path.exists(), &legacy_token, id.clone()) {
        migrate::Plan::AlreadyDone => Ok(()),
        migrate::Plan::MarkOnly => {
            DnsAccountManager::save(&[]).context("写入空的 dns-accounts.json 失败")?;
            Ok(())
        }
        migrate::Plan::Create(account) => {
            DnsAccountManager::save(std::slice::from_ref(&account))
                .context("写入 dns-accounts.json 失败")?;
            bind_unbound_acme_sites(&id).context("绑定站点到新账号失败")?;
            tracing::info!(account = %account.name, "已从旧全局 DNS 配置迁移出一个账号");
            Ok(())
        }
    }
}

/// 从 settings.json 原始 JSON 取旧 token（结构体已删该字段，故按 Value 读）。
fn read_legacy_token() -> String {
    let p = paths::settings_path();
    let Ok(raw) = std::fs::read_to_string(&p) else {
        return String::new();
    };
    serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|v| v.get("cloudflare_api_token")?.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// 给所有「已启用 ACME 且未绑定账号」的站点绑上该账号，并落盘。
fn bind_unbound_acme_sites(account_id: &str) -> anyhow::Result<()> {
    let p = crate::services::website_manager::WebsiteManager::store_path();
    if !p.exists() {
        return Ok(());
    }
    let raw = std::fs::read_to_string(&p)?;
    let mut list: crate::models::website::WebsiteList = serde_json::from_str(&raw)?;
    let mut changed = false;
    for s in list.websites.iter_mut() {
        if s.ssl.acme && s.ssl.dns_account_id.is_none() {
            s.ssl.dns_account_id = Some(account_id.to_string());
            changed = true;
        }
    }
    if changed {
        std::fs::write(&p, serde_json::to_string_pretty(&list)?)?;
    }
    Ok(())
}
```

`WebsiteManager` 的 `store_path()` 在 `src/services/website_manager/mod.rs:22` 是私有的。把它改成 `pub`（仅此一改）：

```rust
    pub fn store_path() -> PathBuf {
        paths::config_dir().join("websites.json")
    }
```

- [ ] **步骤 9：全量编译与测试**

运行：
```bash
cd src-tauri && cargo test 2>&1 | tail -30
cd src-tauri && cargo clippy --all-targets 2>&1 | grep -E "^(warning|error)" | head -20
```
预期：全部 PASS；clippy 无 error。若报 `migrate::Plan` 未使用 `id` 之类的死代码警告，按提示处理（`Plan::MarkOnly` 不带 id 是刻意的）。

- [ ] **步骤 10：Commit**

```bash
git add src-tauri/src/services/dns_account src-tauri/src/commands/dns_account.rs src-tauri/src/services/mod.rs src-tauri/src/commands/mod.rs src-tauri/src/models/website.rs src-tauri/src/models/settings.rs src-tauri/src/commands/config.rs src-tauri/src/lib.rs src-tauri/src/services/website_manager/mod.rs
git commit -m "feat(dns): DnsAccountManager + 旧配置自动迁移 + 命令层接线"
```

---

## 任务 8：前端类型与设置页清理

**文件：**
- 创建：`src/models/dns-account.ts`
- 修改：`src/models/website.ts`、`src/models/settings.ts`
- 修改：`src/modules/settings/pages/SettingsPage.vue`
- 修改：`src/locales/{zh-CN,en-US}.ts`

- [ ] **步骤 1：前端类型**

创建 `src/models/dns-account.ts`：

```ts
/** 与 Rust src-tauri/src/models/dns_account.rs 对齐 */
export interface DnsAccount {
  id: string
  name: string
  /** 'cloudflare' | 'aliyun' | 'dnspod' | 'huawei' */
  provider: string
  token: string
  access_key_id: string
  access_key_secret: string
  zones: string[]
  tested_at?: string | null
}

export const PROVIDERS = ['cloudflare', 'aliyun', 'dnspod', 'huawei'] as const
export type ProviderId = (typeof PROVIDERS)[number]

/** 各服务商的凭证字段标签（UI 按 provider 切换） */
export const CRED_FIELDS: Record<ProviderId, { idLabel: string; secretLabel: string }> = {
  cloudflare: { idLabel: '', secretLabel: 'token' },
  aliyun: { idLabel: 'AccessKey ID', secretLabel: 'AccessKey Secret' },
  dnspod: { idLabel: 'SecretId', secretLabel: 'SecretKey' },
  huawei: { idLabel: 'AccessKey', secretLabel: 'SecretKey' },
}

export function emptyAccount(): DnsAccount {
  return {
    id: crypto.randomUUID(),
    name: '',
    provider: 'cloudflare',
    token: '',
    access_key_id: '',
    access_key_secret: '',
    zones: [],
    tested_at: null,
  }
}
```

`src/models/website.ts` 的 `SslConfig` 追加：

```ts
  /** 签发/续期该站点证书所用的 DNS 账号 id */
  dns_account_id?: string | null
```

`emptySite()` 的 ssl 字面量加 `dns_account_id: null`：

```ts
    ssl: { enabled: false, cert_path: null, key_path: null, acme: false, cert_expires_at: null, dns_account_id: null },
```

`src/models/settings.ts` 删除 `dns_provider: string` 与 `cloudflare_api_token: string` 两行。

- [ ] **步骤 2：设置页删除 DNS 服务商区块**

`SettingsPage.vue` 的 `dns` Tab（`<div v-show="activeTab === 'dns'">`）里，**删除**「DNS 服务商」整块（原文约 270-300 行：`<h3>{{ $t('dnsProvider') }}</h3>` 起的区块，含 provider select、Cloudflare Token 输入、测试按钮与 `dnsTestResult` 展示）。**保留**其后的 DDNS 区块。

`<script setup>` 里删除：`dnsProviderValue`、`cloudflareApiTokenValue`、`dnsTestZone`、`dnsTesting`、`dnsTestOk`、`dnsTestResult` 六个 ref，`testToken()` 函数，以及 `load()` 中的 `dnsProviderValue.value = ...` / `cloudflareApiTokenValue.value = ...` 两行与 `saveNow()`/自动保存里的 `settingsStore.settings.dns_provider = ...` / `cloudflare_api_token = ...` 两行。

Tab 文案：`settingsTabs` 里 `dns` 那项的 label 由 `t('settingsTabDns')` 改为 `t('settingsTabDdns')`。

- [ ] **步骤 3：i18n**

`zh-CN.ts` / `en-US.ts` 各自：

**删除**（两文件同位置）：`dnsProviderDesc`、`cloudflareToken`、`dnsTokenPlaintextWarning`、`testToken`、`testTokenNeedZone`、`testTokenOk`、`testTokenTesting`、`testTokenZone`。

`dnsProvider` 这个键保留（DDNS 区块在用，见 `SettingsPage.vue:334` 的 `$t('ddnsProvider')` 用的是另一个键——**确认后若 `dnsProvider` 已无引用则一并删除**，用 `grep -n "dnsProvider'" src/modules src/layouts` 检查）。

**新增**：

```ts
  // --- DNS 账号 ---
  settingsTabDdns: '动态域名',        // en: 'Dynamic DNS'
  dnsAccounts: 'DNS 账号',            // en: 'DNS Accounts'
  dnsAccountsDesc: '用于自动申请与续期 HTTPS 证书（DNS-01）的域名服务商凭证',  // en: 'DNS provider credentials used to issue and renew HTTPS certificates (DNS-01)'
  newDnsAccount: '新增账号',          // en: 'New Account'
  editDnsAccount: '编辑账号',         // en: 'Edit Account'
  noDnsAccounts: '暂无 DNS 账号',     // en: 'No DNS accounts'
  noDnsAccountsDesc: '添加一个域名服务商凭证后，网站即可自动申请证书',  // en: 'Add a DNS provider credential to issue website certificates automatically'
  accountName: '账号名称',            // en: 'Account Name'
  accountNamePlaceholder: '如：公司 Cloudflare',  // en: 'e.g. Company Cloudflare'
  dnsProviderLabel: '服务商',         // en: 'Provider'
  credId: '凭证 ID',                  // en: 'Credential ID'
  credSecret: '凭证密钥',             // en: 'Credential Secret'
  credToken: 'API Token',             // en: 'API Token'
  testConnect: '测试连通',            // en: 'Test Connection'
  testing: '测试中…',                 // en: 'Testing…'
  testOkZones: '连通，{n} 个域名',    // en: 'Connected, {n} domains'
  testFailed: '测试失败',             // en: 'Test failed'
  notTested: '未测试',                // en: 'Not tested'
  testedAt: '测试于 {time}',          // en: 'Tested {time}'
  zonesCached: '已缓存域名',          // en: 'Cached zones'
  accountInUse: '该账号仍被 {n} 个站点引用，请先改掉站点配置',  // en: 'This account is used by {n} site(s); reassign them first'
  dnsAccount: 'DNS 账号',             // en: 'DNS Account'
  selectDnsAccount: '请选择账号',     // en: 'Select an account'
  acmeNeedAccount: '请选择 DNS 账号后再申请证书',  // en: 'Select a DNS account before issuing a certificate'
  credPlaintextWarning: '凭证以明文保存在配置文件中，请确保本机环境可信',  // en: 'Credentials are stored in plaintext; ensure this machine is trusted'
```

- [ ] **步骤 4：验证**

运行：
```bash
npx vue-tsc --noEmit
npm run build
```
预期：零错误，构建成功。**`npm run build` 必须跑**——`vue-tsc` 查不出删除模板块时残留的多余 `</div>`。

- [ ] **步骤 5：Commit**

```bash
git add src/models src/modules/settings/pages/SettingsPage.vue src/locales
git commit -m "feat(dns): 前端类型 + 设置页移除全局 DNS 凭证区块（含 testToken 死代码）"
```

---

## 任务 9：DNS 账号页面

**文件：**
- 创建：`src/modules/dns-accounts/pages/DnsAccountsPage.vue`
- 创建：`src/modules/dns-accounts/components/DnsAccountDialog.vue`
- 修改：`src/router/index.ts`、`src/layouts/Sidebar.vue`

- [ ] **步骤 1：对话框组件**

创建 `src/modules/dns-accounts/components/DnsAccountDialog.vue`。骨架照 `src/modules/website-manager/components/SiteEditDialog.vue`（同样的 `overlay`/`confirm-box` 类名、`Teleport to="body"`、`emit('close')`/`emit('saved')`）。

字段：账号名称（必填）、服务商 `<select>`（四项）、按 `CRED_FIELDS` 切换的凭证输入（`cloudflare` 只显示一个 Token 输入，其余显示 ID + Secret 两个；全部 `type="password"`）、底部一行「凭证以明文保存」的提示（`credPlaintextWarning`）。

底部按钮：取消、**测试连通**、保存。

测试按钮逻辑（**仅在已保存过的账号上可用**——测试命令按 id 查账号）：

```ts
async function doTest() {
  testing.value = true
  testResult.value = ''
  testOk.value = null
  try {
    zones.value = await invoke<string[]>('test_dns_account', { id: form.value.id })
    testOk.value = true
  } catch (e) {
    testOk.value = false
    testResult.value = String(e)
  } finally {
    testing.value = false
  }
}
```

新建（尚未保存）时测试按钮 `:disabled="isNew"`，`:title="$t('selectDnsAccount')"` 之类的提示——用 `$t('saveFirst')` 之类现有键，若无则用 `accountName` 之外的既有键；**不要**新造键，若确需则加入任务 8 的 i18n 清单。

保存：

```ts
async function save() {
  saving.value = true
  saveError.value = ''
  try {
    await invoke('save_dns_account', { account: form.value })
    emit('saved')
  } catch (e) {
    saveError.value = String(e)
  } finally {
    saving.value = false
  }
}
```

- [ ] **步骤 2：列表页**

创建 `src/modules/dns-accounts/pages/DnsAccountsPage.vue`，骨架照 `WebsiteListPage.vue`：

- `PageHeader icon="mdi:dns-outline" :title="$t('dnsAccounts')" :subtitle="$t('dnsAccountsDesc')"` + `#actions` 里「新增账号」按钮
- `EmptyState v-if="!loading && accounts.length === 0"`（`noDnsAccounts` / `noDnsAccountsDesc`）
- 卡片网格：`grid grid-cols-1 md:grid-cols-2 gap-4`，每卡显示账号名、服务商徽标、凭证是否已填、测试状态（`tested_at` 存在 → `testedAt` 格式化 + `testOkZones` 带 zone 数；否则 `notTested`）、`zones` 标签列表、「编辑」「删除」按钮
- 删除确认框照 `WebsiteListPage` 的 `delTarget` + `Teleport` 写法，`confirmDeleteDnsAccount` 文案用 `accountInUse`（后端也会拦截，前端提示是预检）
- 错误展示用 `pageError` + `Teleport` overlay（照抄 `WebsiteListPage` 的 11-21 行）

`load()` 调 `invoke<DnsAccount[]>('list_dns_accounts')`。

- [ ] **步骤 3：路由与菜单**

`src/router/index.ts` 在 `/websites` 之后插入：

```ts
  {
    path: '/dns-accounts',
    name: 'dnsAccounts',
    component: () => import('@/modules/dns-accounts/pages/DnsAccountsPage.vue'),
    meta: { title: 'dnsAccounts' },
  },
```

`src/layouts/Sidebar.vue` 的 `management` 组内，`/websites` 那行之后插入：

```ts
      { path: '/dns-accounts', titleKey: 'dnsAccounts', icon: 'mdi:dns-outline' },
```

- [ ] **步骤 4：验证**

运行：
```bash
npx vue-tsc --noEmit
npm run build
```
预期：零错误，构建成功。

- [ ] **步骤 5：Commit**

```bash
git add src/modules/dns-accounts src/router/index.ts src/layouts/Sidebar.vue
git commit -m "feat(dns): DNS 账号管理页面（列表 + 对话框 + 测试连通）"
```

---

## 任务 10：站点绑定账号

**文件：**
- 修改：`src/modules/website-manager/components/SiteEditDialog.vue`
- 修改：`src/modules/website-manager/pages/WebsiteListPage.vue`（把账号列表传给对话框）

- [ ] **步骤 1：删掉全局凭证判断**

`SiteEditDialog.vue` 里删除：

```ts
const hasDnsToken = computed(() => {
  const s = settingsStore.settings
  if (!s) return true // 设置未加载完成时不误报
  return s.dns_provider === 'cloudflare' ? !!s.cloudflare_api_token : false
})
```

以及所有 `hasDnsToken` 的使用点（模板里的禁用/提示）。若删除后 `settingsStore` 在该文件已无其他用途，一并删掉 `const settingsStore = useSettingsStore()` 与其 import。

- [ ] **步骤 2：加载账号列表 + 加下拉**

在 `<script setup>` 里：

```ts
const dnsAccounts = ref<DnsAccount[]>([])
onMounted(async () => {
  try {
    dnsAccounts.value = await invoke<DnsAccount[]>('list_dns_accounts')
  } catch (e) {
    console.error('load dns accounts failed:', e)
  }
})
```

（若该文件已有 `onMounted`，合并进去，别写第二个。）

模板里在 SSL 配置区（`certSource` 切到 `acme` 时可见）插入：

```vue
<div class="flex items-center justify-between gap-4 py-3">
  <div>
    <span class="text-sm">{{ $t('dnsAccount') }}</span>
    <p class="text-xs text-muted-foreground mt-0.5">{{ $t('acmeNeedAccount') }}</p>
  </div>
  <select
    v-model="form.ssl.dns_account_id"
    class="h-8 px-2 w-56 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary cursor-pointer"
  >
    <option :value="null">{{ $t('selectDnsAccount') }}</option>
    <option v-for="a in dnsAccounts" :key="a.id" :value="a.id">
      {{ a.name }}（{{ a.provider }}）
    </option>
  </select>
</div>
```

- [ ] **步骤 3：签发前校验账号已选**

`doIssue()` 开头加：

```ts
  if (!form.value.ssl.dns_account_id) {
    acmeStatus.value = t('acmeNeedAccount')
    return false
  }
```

（放在现有前置校验之后、`invoke('issue_site_certificate', ...)` 之前。）

- [ ] **步骤 4：验证**

运行：
```bash
npx vue-tsc --noEmit
npm run build
```
预期：零错误。

- [ ] **步骤 5：Commit**

```bash
git add src/modules/website-manager
git commit -m "feat(dns): 站点 SSL 配置改为选择 DNS 账号"
```

---

## 任务 11：全量验证与实机

- [ ] **步骤 1：全量测试与构建**

运行：
```bash
cd src-tauri && cargo test 2>&1 | tail -40
cd src-tauri && cargo clippy --all-targets 2>&1 | grep -E "^(warning|error)" | head -20
npx vue-tsc --noEmit
npm run build
```
预期：全绿。

- [ ] **步骤 2：确认签名层零改动**

运行：
```bash
git diff dev...HEAD --stat -- src-tauri/src/services/ddns/
git log --oneline dev..HEAD
```
预期：`ddns/*.rs` 的 diff **不含**任何 `*_canonical_request` / `*_authorization` / `hmac_sha256` / `hex` / `percent_encode` / `pe` / `HOST` / `SERVICE` / `VERSION` / `CONTENT_TYPE` 行的改动；测试函数名与数量不变（DNSPod 那 5 条必须原样在）。若不符，**回退重做任务 4**。

- [ ] **步骤 3：实机验证（必须）**

`npm run tauri dev`，按规格的清单逐项：

1. 有旧全局 Cloudflare Token 的配置启动 → 自动生成「默认 Cloudflare」账号，已勾 ACME 的站点自动绑定
2. 无旧 token 的配置启动 → 账号页为空，`dns-accounts.json` 被创建（重启不再重复迁移）
3. 账号页新增四家各一条，分别点「测试连通」→ 各自返回 zone 列表
4. 站点里选不同账号，分别申请证书
5. **DNSPod 账号实机申请一次证书**（TXT 路径首次实测；DDNS 只验证过 A/AAAA）
6. 申请完成后去 DNS 控制台确认 `_acme-challenge` TXT 已被清理
7. 删除被引用的账号 → 被拦截并提示站点名
8. 设置页 `dns` Tab 只剩 DDNS，控制台无报错
9. 等一轮自动续期或手工触发，确认续期走账号凭证

- [ ] **步骤 4：合并回 dev**

按项目规则（合并即删分支）：

```bash
git checkout dev
git merge --no-ff feat/dns-accounts
git push origin dev
git branch -d feat/dns-accounts
git push origin --delete feat/dns-accounts 2>/dev/null || true
```
