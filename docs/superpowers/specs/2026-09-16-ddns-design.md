# DDNS 动态域名设计

> 状态：已确认设计
> 日期：2026-09-16
> 范围：v1 四家服务商（Cloudflare / 阿里云 / DNSPod / 华为云），公网 IP 检测 + 多域名 A/AAAA 同步

## 背景

类 ddns-go：宽带/内网环境的公网 IP 会变，需要定时检测并更新 DNS 解析，使域名始终指向当前出口 IP。项目已有可复用地基：后台调度器模式（recorder 30s / acme renew 1h / watchdog 5s）、reqwest HTTP、审计宏、设置页区块模式。**与网站证书的 DNS 配置完全独立**（两套字段、两套 provider 选择）。

## 已确认决策

1. **四家服务商**：Cloudflare / 阿里云 / DNSPod / 华为云，trait 预留扩展。
2. **多域名列表**：同一 IP 同步到所有条目。
3. **IPv4 + IPv6**：A + AAAA 双记录，IPv6 默认关（`ddns_enable_ipv6`）。
4. **公网 API 检测**：多源轮询任一成功即用。
5. **5 分钟固定间隔**，不做间隔配置。
6. **与证书配置互不相干**：DDNS 自带 provider 选择与凭证字段（`ddns_*` 前缀），Cloudflare 不复用证书的 token。
7. 失败：warn + 审计，不重试。
8. 「立即同步」即测试：跑真实同步，结果/错误直接报出。

## 设计

### 1. 服务商抽象 `services/ddns/`（新模块）

```rust
/// DDNS 服务商：把 fqdn 的 A/AAAA 记录同步为当前 IP。
pub trait DdnsProvider: Send + Sync {
    fn id(&self) -> &str;
    /// 查现有记录 → 无则创建 / 不同则更新 / 相同跳过。返回动作描述（"新建"/"更新 1.2.3.4 → 5.6.7.8"/"未变"）。
    fn sync_record<'a>(&'a self, fqdn: &'a str, rtype: &'a str, ip: &'a str)
        -> BoxFuture<'a, anyhow::Result<String>>;
}

/// 按 id + 凭证取实现
pub fn provider_for(s: &DdnsSettings) -> Option<Box<dyn DdnsProvider>>;
```

实现文件（各一个）：
- `cloudflare.rs`：`GET/POST/PATCH /zones/{zone}/dns_records`，zone 用 `?name=` 查最长匹配；Bearer token。
- `aliyun.rs`：`alidns.aliyuncs.com` RPC——`DescribeSubDomainRecords` / `AddDomainRecord` / `UpdateDomainRecord`；签名 HMAC-SHA1（`RPCSignature` 纯函数 + 单测，固定 key+参数 → 已知签名值）。
- `dnspod.rs`：`dnsapi.tencentcloudapi.com` TC3-HMAC-SHA256 签名（纯函数 + 单测）——`DescribeRecordList` / `CreateRecord` / `ModifyRecord`。
- `huawei.rs`：华为云 DNS `dns.myhuaweicloud.com`——`ListRecordSetsByZone` / `CreateRecordSet` / `UpdateRecordSet`；AK/SK 签名（HWS HMAC-SHA256，纯函数 + 单测）。zone 通过 `/v2/zones?name=` 查询。

四家的 zone/记录 ID 缓存在一次 `sync_record` 调用内，不跨轮持久化（查询一次的开销可接受）。

### 2. IP 检测 `services/ddns/ip.rs`

```rust
/// 依次请求源列表，任一成功提取到合法 IP 即返回；全失败返回 Err。
pub async fn detect_public_ip(v6: bool) -> anyhow::Result<String>
```

- v4 源：`https://myip.ipip.net`（响应文本提取 IPv4）、`https://api.ipify.org`
- v6 源：`https://api6.ipify.org`、`https://ipv6.icanhazip.com`
- 提取：正则 `\bIPv?4?地址[：:]\s*([0-9.]+)` 或直接整个 body trim 后用 `std::net::Ipv4Addr/Ipv6Addr` parse 校验（后者为主，解析失败换下一源）。
- 纯函数 `extract_ip(body) -> Option<String>` + 单测（ipip 中文格式 / 纯 IP / 垃圾文本）。

### 3. 调度器 `services/ddns/scheduler.rs`

```
run_ddns_scheduler(app)：5 分钟 tokio interval（Skip 错过补偿，与 recorder 一致）
  读设置；未启用 → 跳过本轮（不退出任务，改设置即时生效）
  v4 = detect_public_ip(false)；v6（若开启）= detect_public_ip(true)
  与内存中 last_ip 比对（v4/v6 各自记录）
  变化 → 遍历 ddns_domains 逐条 sync_record("A", v4)（+ IPv6 开启时 "AAAA"）
  成功动作写审计 oplog!("ddns_update", 域名, "A 旧→新 或 新建")；失败 oplog_fail! + warn
  全部成功后更新 last_ip
```

### 4. 设置字段（`AppSettings`，全部 `#[serde(default)]`）

```rust
ddns_enabled: bool,            // 默认 false
ddns_provider: String,         // "cloudflare"（默认）| "aliyun" | "dnspod" | "huawei"
ddns_cloudflare_token: String,
ddns_aliyun_access_key_id: String,  ddns_aliyun_access_key_secret: String,
ddns_dnspod_secret_id: String,      ddns_dnspod_secret_key: String,
ddns_huawei_access_key: String,     ddns_huawei_secret_key: String,
ddns_domains: Vec<String>,     // 每行一个
ddns_enable_ipv6: bool,        // 默认 false
```

TypeScript 侧同步。

### 5. 命令 `commands/config.rs`

```rust
/// 立即执行一轮完整同步（测试按钮）：检测 IP + 逐域名同步。
/// 返回人可读结果串（"公网 IP: x.x.x.x；home.example.com A 记录更新 1→2；… "）。
/// 未启用/未配域名/未配凭证 → 明确报错。
#[tauri::command]
pub async fn sync_ddns_now() -> Result<String, String>
```

复用调度器同一条同步路径（抽 `sync_once(&settings) -> Result<Report>`，调度器与命令共用）。

### 6. 设置页 UI

「DDNS 动态域名」区块（告警通知之后）：
- 启用开关
- 服务商下拉（四选一）
- 凭证输入区 `v-if` 按所选服务商切换（Cloudflare 单 token；其余 AK/SK 或 SecretId/Key 两栏）
- 域名 Textarea（placeholder：每行一个完整子域名，如 home.example.com）
- IPv6 开关 + 副文案（开启后同步 AAAA 记录）
- 「立即同步」按钮：`await save()` → `invoke('sync_ddns_now')` → toast 展示返回串/错误（沿用测试按钮哲学：先存盘再测）

i18n：`ddnsSection / ddnsEnabled / ddnsProvider / ddnsCloudflareToken / ddnsAliyunKey / ddnsAliyunSecret / ddnsDnspodId / ddnsDnspodKey / ddnsHuaweiKey / ddnsHuaweiSecret / ddnsDomains / ddnsDomainsPlaceholder / ddnsIpv6 / ddnsIpv6Hint / syncDdnsNow / ddnsSyncOk / ddnsNeedConfig` 等中英各 ~18 键。

### 7. 测试

- 签名纯函数四家：固定 key+参数 → 已知签名值（各家官方文档算例或手工 HMAC 基准）
- `extract_ip`：中文 ipip 格式 / 纯 IP / 垃圾文本
- `longest_zone_match` 已有测试复用（Cloudflare 场景）
- 调度/真实 API 不做单测，「立即同步」+ 实机验证覆盖

### 8. 实机验证清单

1. 未启用时点「立即同步」→ 报「请先启用」
2. Cloudflare + 真实 token + 一个子域名 → 立即同步 → 记录创建成功，返回串含动作
3. 重复点 → 「未变」
4. 手动改 Cloudflare 上的 A 记录为别的 IP → 再点 → 更新回正确 IP
5. 错误 token → 明确的 API 错误（401/10000 等）
6. （有 IPv6 环境）开 IPv6 → AAAA 同步
7. 启用后等 5 分钟（或改本机 hosts 模拟不可行，改为观察审计）→ 审计出现 ddns_update
8. 旧 settings.json 加载 → 全部默认值，无迁移

## 边界（不做）

- 不做网卡 IP 模式、自定义检测源、检测间隔配置
- 不做多套公网 IP（多出口负载均衡）
- 不做独立状态页（变更看操作记录）
- TTL 用各服务商默认（Cloudflare Auto、阿里 600、DNSPod 600、华为 300）
- 不做证书签发的 DNS-01 扩展（证书仍仅 Cloudflare，且与本功能字段无关——「证书按站点配置」另立项）

## 改动文件清单

- `src-tauri/src/services/ddns/{mod,cloudflare,aliyun,dnspod,huawei,ip,scheduler}.rs`（新模块）
- `src-tauri/src/models/settings.rs`、`src/models/settings.ts`（11 字段）
- `src-tauri/src/commands/config.rs`（sync_ddns_now）
- `src-tauri/src/lib.rs`（spawn 调度器 + 注册命令）
- `src/modules/settings/pages/SettingsPage.vue`
- `src/locales/zh-CN.ts`、`src/locales/en-US.ts`
