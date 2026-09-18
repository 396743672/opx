# DNS 账号独立化设计

> 状态：已确认设计
> 日期：2026-09-16
> 范围：证书 DNS 凭证从全局设置搬到独立的「DNS 账号」实体；`DnsProvider` trait 下沉为 acme 与 DDNS 共用

## 背景

「每个网站配置的证书可能是不同家的」——当前证书 DNS 凭证是**全局**的：

| 位置 | 现状 |
|---|---|
| `AppSettings` | `dns_provider`（默认 `"cloudflare"`）、`cloudflare_api_token` 两个字段 |
| `Site.ssl` | 只有 `enabled / cert_path / key_path / acme / cert_expires_at`，无服务商与凭证 |
| `acme::dns::provider_for(id, token)` | 只认 `"cloudflare"`，token 是单值 `String`（阿里云/DNSPod/华为需双凭证） |
| 签发入口 | 两处都读全局设置：`commands/website.rs:523`（手动申请）、`acme/renew_scheduler.rs:33`（自动续期） |
| 旧 UI | `SiteEditDialog` 的 `hasDnsToken` 读全局设置判断「是否配好」；设置页 `dns` Tab 内有凭证区块 |

结果：所有站点只能共用同一家 DNS 服务商。本次把凭证抽成独立实体，站点按 id 引用。

## 已确认决策

1. **独立的「DNS 账号」页面**（与网站管理同级菜单），维护服务商账号列表；站点只选「用哪条」。
2. **站点存账号 id 引用**（`Site.ssl.dns_account_id`），凭证只存一份。
3. **支持四家服务商**：Cloudflare / 阿里云 / DNSPod / 华为云。
4. **自动迁移**旧配置，用户无需手动重建。
5. **测试按钮 + 缓存 zone 列表**：测试连通并缓存 zone 名供站点下拉。
6. **测试只验 zone 列表**（读权限），不做写探针——不往用户 DNS 写任何记录。
7. **trait 下沉**（方案 B）：`DnsProvider` 定义在 `acme::dns`，由 acme 与 DDNS 共用；DDNS 的 `sync_record` 变成默认实现。
8. **修复既有缺陷**：设置页 `testToken()` 函数与 `dnsTestOk/dnsTestResult/dnsTesting` 三个 ref 是死代码（无按钮绑定），删除。

## 设计

### 1. 数据模型

新增 `src-tauri/src/models/dns_account.rs`：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsAccount {
    pub id: String,          // uuid
    pub name: String,        // 用户可读名，如「公司 Cloudflare」
    /// "cloudflare" | "aliyun" | "dnspod" | "huawei"
    pub provider: String,
    /// 单凭证服务商（cloudflare）用；其余为空
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub access_key_id: String,
    #[serde(default)]
    pub access_key_secret: String,
    /// 测试成功后缓存的 zone 名列表；失败/未测试为空
    #[serde(default)]
    pub zones: Vec<String>,
    /// 上次测试成功时间（RFC3339 本地），未测过为空
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tested_at: Option<String>,
}
```

持久化在 `config/dns-accounts.json`，管理器 `DnsAccountManager` 照 `WebsiteManager` 的写法（`RwLock<Vec<DnsAccount>>` + 原子落盘）。

`Site.ssl` 新增：

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub dns_account_id: Option<String>,
```

`Option` + `skip_serializing_if` 保证**零迁移**：老 `websites.json` 反序列化后为 `None`，未被编辑的站点不会因升级而被改写出新字段。

### 2. trait 下沉（核心）

`acme/dns/mod.rs` 定义完整 trait：

```rust
pub trait DnsProvider: Send + Sync {
    fn id(&self) -> &str;
    /// 该账户下所有 zone 名（测试按钮与域名归属确认用）
    fn list_zones<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>>>;
    /// 返回 domain 所属 zone；无匹配 → Err
    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, Result<String>>;
    /// 读 name+type 的当前值（无记录 → None）
    fn get_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str) -> BoxFuture<'a, Result<Option<String>>>;
    /// 无记录则新建，有则更新
    fn set_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str, value: &'a str) -> BoxFuture<'a, Result<()>>;
    /// 删除 name+type 的记录；不存在时视为成功（幂等）
    fn delete_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str) -> BoxFuture<'a, Result<()>>;
}
```

`ddns/mod.rs` 的 trait 收窄，只多一个默认实现方法：

```rust
pub trait DdnsProvider: Send + Sync {
    fn id(&self) -> &str;
    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, Result<String>>;
    fn get_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str) -> BoxFuture<'a, Result<Option<String>>>;
    fn set_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str, value: &'a str) -> BoxFuture<'a, Result<()>>;

    /// DDNS 语义：读-比较-写，返回「A 记录新建 1.2.3.4 / 更新 old → new / 未变」
    fn sync_record<'a>(&'a self, fqdn: &'a str, rtype: &'a str, ip: &'a str)
        -> BoxFuture<'a, Result<String>>
    { /* get_value → 相等则 "未变" → 否则 set_value → 拼文案 */ }
}

/// dyn DnsProvider 天然是 DdnsProvider —— 这是唯一的 blanket impl，
/// 四个服务商文件只需 impl DnsProvider，不再各写一遍 sync_record。
impl<T: DnsProvider + ?Sized> DdnsProvider for T {}

pub fn provider_for_account(a: &DnsAccount) -> Option<Box<dyn DnsProvider>>;
```

四个方法在**两个 trait 上同名同签名**，所以 `ddns/{cloudflare,aliyun,dnspod,huawei}.rs` 里每个文件只需：
- 加 `use crate::services::acme::dns::{BoxFuture, DnsProvider};`
- 改 `impl DdnsProvider for X` → `impl DnsProvider for X`
- **删掉 `sync_record` 块**（改由默认实现提供），其内部逻辑搬到 `get_value` / `set_value` 拆分

签名函数、`call()`、`find_zone()`、`record_name()`、常量**一行不动**——已有测试因此全部保留。

**行为变化（必须显式注明）**：原 `create_txt` 只新建、`delete_txt` 按值匹配删除；改成 `set_value` 后新建与更新同路径。对 DNS-01 这是正确的（同名 TXT 可能残留旧值，更新比追加干净），但这是真实的行为变更，代码里需写注释说明。

`acme/mod.rs` 的改动：
- `AcmeSettings { dns_provider, cloudflare_api_token, use_staging }` → `AcmeSettings { account: DnsAccount, use_staging }`
- `provider_for(&settings.dns_provider, &settings.cloudflare_api_token)` → `provider_for_account(&settings.account)`
- `create_txt(&fqdn, &value)` → `set_value(&fqdn, "TXT", &value)`

**清理阶段的语义（重要，原设计想漏了）**：原 `delete_txt(fqdn, value)` 是**删除**记录。`set_value` 没有删除能力，所以清理不能靠它。正确做法是：签发开始时对每个 `_acme-challenge.<domain>` 先 `get_value` 记下旧值，验证结束后：

```
记录原本就存在（get_value == Some(old)）→ set_value(fqdn, "TXT", old)   还原
记录原本不存在（get_value == None）      → delete_value(fqdn, "TXT")     删除
```

因此 **trait 需要第 5 个方法 `delete_value`**（而不是我先前说的「写回旧值即可」——那会把 OPX 自己造的记录永久留在用户 DNS 里）：

```rust
/// 删除 name+type 的记录；不存在时视为成功（幂等）
fn delete_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str) -> BoxFuture<'a, Result<()>>;
```

`sync_record` 的 DDNS 路径不需要它（DDNS 从不删记录），但 acme 的清理路径必须要有——否则每次签发都在用户 DNS 里残留一条 TXT。`delete_value` 也一并成为 ACME 与 DDNS 共用 trait 的一部分。

### 3. 测试与 zone 缓存

```rust
#[tauri::command]
pub async fn test_dns_account(id: String) -> Result<Vec<String>, String>
```

流程：读账号 → `provider_for_account` → `list_zones()` → 成功后把 zones 与 `tested_at` 写回账号并落盘 → 返回 zone 名列表。失败只返回错误，**不动缓存**（保留旧值便于区分「之前好的」与「现在坏了」）。

**只验读权限**：`list_zones()` 不验写权限，只有读权限的 Token 会通过测试、在申请证书时才失败。这是刻意的取舍——写探针要往用户 DNS 真建一条 `_opx-token-test` TXT，残留比「晚一步报错」更糟。ACME 签发失败的报错已经足够定位（Cloudflare 10000 的权限提示见 `acme/dns/cloudflare.rs:39`）。

### 4. 迁移

`AppSettings` 删除 `dns_provider` / `cloudflare_api_token`；`test_dns_token` 命令删除。

启动迁移在 `lib.rs` 的 `setup` 里，`WebsiteManager::new()` 注册 State 之前（顺序要求：迁移会改 `websites.json`，必须在 `WebsiteManager::new()` 读盘之前完成，否则内存里的站点拿不到 `dns_account_id`）：

```
读 config/dns-accounts.json
  已存在（哪怕空数组）→ 不迁移，结束
  不存在 → 用 serde_json::Value 读 settings.json 取 cloudflare_api_token
    非空 → 建账号 { name: "默认 Cloudflare", provider: "cloudflare",
                    token: <旧值> }，写入 dns-accounts.json
    为空 → 只写空文件（标记「迁移已完成」）
  若建了账号 → 把所有 ssl.acme == true 且 dns_account_id == None 的站点绑到该 id
              （直接读改 websites.json）
```

**不放进 `startup_bootstrap`**：那里是「开机统一启动序列」（软件/Node/Stack 编排），职责是拉起进程、与本次迁移无关；且它在 `setup` 里注册完所有 State 之后才 spawn，时机会晚于 `WebsiteManager::new()`。

**关键点**：迁移信号是「`dns-accounts.json` 不存在」，不是「字段有值」。因为要区分「从没配过」（不建账号）与「迁移后用户删光了账号」（不能每次启动又冒出来一个）。

旧字段读取方式：结构体删字段后，用 `serde_json::Value` 直接读 settings.json 取 `cloudflare_api_token`（结构体反序列化默认允许未知字段，旧文件能正常解析，不需要 `deny_unknown_fields` 处理）。用户下次保存设置时旧键自然消失。

`acme_use_staging` 保留在 `AppSettings`（与账号无关）。

### 5. UI

**新页面 `/dns-accounts`**（路由名 `dnsAccounts`，`meta.title: 'dnsAccounts'`，侧边栏放在「网站管理」附近）

结构与 `WebsiteListPage` 一致：`PageHeader` + 列表卡片 + 新增/编辑对话框。

列表每行：账号名、服务商、凭证是否已填、测试状态（`tested_at` + zone 数 / 未测试）、编辑、删除。
**删除拦截**：被 N 个站点引用时拒绝删除，提示先改站点。

对话框字段按服务商切换（与 DDNS 区块同款交互）：

| provider | 字段 |
|---|---|
| cloudflare | Token |
| aliyun | AccessKey ID + AccessKey Secret |
| dnspod | SecretId + SecretKey |
| huawei | AccessKey + SecretKey |

底部「测试连通」按钮 → `testDnsAccount(id)`，成功后显示「连通，N 个域名」并把 zone 名渲染成标签。

**设置页**：`dns` Tab 只剩 DDNS。删除「DNS 服务商 + Cloudflare Token」区块，及 `dnsProviderDesc` / `cloudflareToken` / `dnsTokenPlaintextWarning` / `testToken*` 等 i18n 键；删除死代码 `testToken()` 与 `dnsTestOk` / `dnsTestResult` / `dnsTesting` / `dnsTestZone` ref。Tab 标签「域名与 DNS」→「动态域名」。

**`SiteEditDialog`**：删除 `hasDnsToken` computed，SSL 配置里加「DNS 账号」下拉（选项来自账号列表，显示 `名称（服务商）`）。已勾 ACME 但未绑账号时给出提示。

i18n 中英各约 20 键。

## 验证

- `cargo test`：现有测试全绿（签名层未改动）
- `cargo clippy` / `npx vue-tsc --noEmit` 零错误；`npm run build` 成功
- 新增测试：
  - `DnsAccount` 序列化往返；缺字段反序列化为空
  - `Site.ssl.dns_account_id` 缺失时为 `None`（零迁移）
  - `sync_record` 默认实现三分支：无记录 → 新建文案；值不同 → 更新文案；值相同 → `"未变"`
  - `provider_for_account` 按 provider 分派、凭证缺失返回 `None`
  - 迁移函数：有 token → 建账号 + 绑定 acme 站点；无 token → 只写空文件；文件已存在 → 不改动
  - 删除拦截：被引用时拒绝
- 实机清单：
  1. 旧配置启动 → 自动生成一个「默认 Cloudflare」账号，已勾 ACME 的站点自动绑定
  2. 新页面新增四家各一条账号，测试按钮分别返回 zone 列表
  3. 站点里选不同账号，各自申请证书成功
  4. DNSPod 账号实机申请一次证书（DDNS 已验证过 DNSPod 的 A 记录，TXT 是首次）
  5. 删除被引用的账号 → 被拦截
  6. 设置页 `dns` Tab 只剩 DDNS，无残留报错
  7. 自动续期仍工作（`renew_scheduler` 走账号凭证）

## 边界（不做）

- 不做跨账号 zone 自动匹配（站点必须显式选账号）
- 不做写权限探针（见第 3 节取舍）
- 不做凭证加密存储（沿用现有明文 + UI 提示）
- 不改自签证书流程（不依赖 DNS）
- 不新增第五家服务商

## 风险

> **状态（2026-09-18）**：用户当前**仅持有腾讯云（DNSPod）域名账号**，其余三家无可用账号 →
> 下面第一条的「硬性验收项」**降低为已知风险**（非已验证），待日后取得任一另三家账号时补跑。
> 验证入口已备好：`src-tauri/tests/live_dns.rs`（DNSPod 写-读-删闭环，含代理劫持/鉴权链路复现）。

- **阿里云 / DNSPod / 华为的 TXT 写入未实测**。DDNS 只验证过这四家的 **A/AAAA** 路径（DNSPod 全链路通了），云显/ACME 的 TXT 路径是首次。历史上 DDNS 的 5 个缺陷全是「单测绿、线上错」（错误码、端点、IP 家族来源），故**实机验证 TXT 是本次的硬性验收项**，优先 DNSPod（唯一有实测账号的）。
- 华为 zone 列表版本（`/v2/zones` vs `/v2.1/zones`）仍未经实机确认，见 `ddns/huawei.rs:154` 的 `ponytail:` 注释。本次 `list_zones` 也走这条路径，风险不变。

## 改动文件

**新增**
- `src-tauri/src/models/dns_account.rs`
- `src-tauri/src/services/dns_account/mod.rs`（`DnsAccountManager`）
- `src-tauri/src/commands/dns_account.rs`
- `src/modules/dns-accounts/pages/DnsAccountsPage.vue`（+ 对话框组件）
- `src/models/dns-account.ts`

**修改**
- `src-tauri/src/services/acme/dns/mod.rs`（trait 下沉 + `provider_for_account`）
- `src-tauri/src/services/acme/mod.rs`（`AcmeSettings` 换 account）
- `src-tauri/src/services/acme/renew_scheduler.rs`（读账号）
- `src-tauri/src/services/ddns/{mod,cloudflare,aliyun,dnspod,huawei}.rs`（impl 改名、删 `sync_record`）
- `src-tauri/src/models/{settings,website}.rs`
- `src-tauri/src/commands/{config,website}.rs`
- `src-tauri/src/services/startup_bootstrap.rs`（**不改**——迁移不放这里，见第 4 节）
- `src/modules/settings/pages/SettingsPage.vue`（删区块 + 死代码）
- `src/modules/website-manager/components/SiteEditDialog.vue`（账号下拉）
- `src/router/index.ts`、侧边栏导航
- `src/models/settings.ts`、`src/models/website.ts`
- `src/locales/{zh-CN,en-US}.ts`
