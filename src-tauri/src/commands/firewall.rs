//! 防火墙端口规则管理命令。
//!
//! 设计边界完整说明见 `opx_core::services::firewall` 模块文档，要点：
//! **只维护端口规则，绝不启停防火墙服务**；不碰 iptables/nftables 裸规则。
//!
//! 本层职责是「把系统里所有会监听端口的来源汇总成一张可操作的清单」：
//! opx 自身 Web 入口 / 内置与自定义软件 / JAR 应用 / 站点 / 用户手动添加。

use std::collections::HashMap;
use std::sync::Arc;

use tauri::State;

use opx_core::models::settings::ManualPortRule;
use opx_core::services::firewall::{self, BackendInfo, PortRule, Proto};
use opx_core::services::software_manager::config_editor;
use opx_core::services::software_manager::netutils;
use opx_core::services::software_manager::providers::{self, ConfigContext};
use opx_core::services::software_manager::start_stop::collect_configured_ports;
use opx_core::services::software_manager::SoftwareManager;
use opx_core::services::springboot_manager::SpringBootManager;
use opx_core::services::website_manager::WebsiteManager;
use opx_core::utils::settings::{read_settings, write_settings_file};
use opx_core::audited;

/// 各 provider 表示「监听地址」的字段名，按优先级排列。
///
/// `bind-address` 是 MySQL 在 my.cnf 里的写法（连字符，与 `bind_address` 不同键）。
const BIND_KEYS: [&str; 6] = ["bind", "bind_address", "bind-address", "bind_ip", "listen_host", "host"];

/// 端口清单里的一条记录（一个来源 × 一个端口）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct FirewallPortEntry {
    pub port: u16,
    /// "tcp" | "udp"
    pub proto: String,
    /// "opx" | "software" | "jar" | "site" | "manual"
    pub source: String,
    /// 来源显示名（opx 自身留空，由前端用 i18n 文案渲染）
    pub label: String,
    /// 附加说明（版本 / 域名等）
    pub detail: String,
    /// 内置/自定义软件实例 id，供前端跳转到配置改 bind；其它来源为 None
    pub instance_id: Option<String>,
    /// 防火墙当前是否已放行
    pub allowed: bool,
    /// 来源限定（仅手动添加的端口会有）：非空 = 仅对这些 IP/CIDR 放行
    pub sources: Vec<String>,
    /// 服务监听地址
    pub bind: Option<String>,
    /// true = 仅监听回环地址，即便放行防火墙也无法从外部访问
    pub bind_local_only: bool,
    /// `listen` = 取自真实监听套接字（最可信）；`config` = 取自配置/默认值；
    /// `unknown` = 未运行且无配置可推断
    pub bind_source: String,
    /// 能否由 opx 一键改成 0.0.0.0（配置或 schema 里有可写的地址字段）
    pub bind_fixable: bool,
}

/// 端口页的完整数据。
#[derive(Debug, Clone, serde::Serialize)]
pub struct FirewallOverview {
    pub backend: BackendInfo,
    pub ports: Vec<FirewallPortEntry>,
    /// 读取防火墙规则失败的原因（如非 root 运行）。此时 `allowed` 全为 false，
    /// 前端必须显式展示该错误，否则用户会误以为「全都没放行」而重复操作。
    pub error: Option<String>,
    pub web_enabled: bool,
    pub web_port: u16,
}

/// 从实例 config 推断监听地址，实例未配置时回退到 provider schema 的默认值。
///
/// 为什么必须看 schema 默认值：provider 的「默认只绑本机」是写在
/// `ConfigSchema` 字段默认值里的（如 consul/redis 的 `bind` = `127.0.0.1`），
/// 实例 config 在用户没改过时**根本没有这个键**。只看 config 会得到 None，
/// 于是 UI 不提示「仅监听本机」—— 而这正是「防火墙放行了却连不上」的主因。
///
/// 各 provider 字段名不统一（`bind` / `bind_address` / `host`），按常见优先级取
/// 第一个非空值。都找不到返回 None：表示无法判定，前端不显示告警（宁可不说，也不误报）。
fn bind_of(
    config: &serde_json::Value,
    schema: Option<&opx_core::models::software::ConfigSchema>,
) -> Option<String> {
    for key in BIND_KEYS {
        // 1. 实例显式配置优先
        if let Some(v) = config.get(key).and_then(|v| v.as_str()) {
            let v = v.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
        // 2. 回退 provider schema 的字段默认值
        if let Some(field) = schema.and_then(|s| s.fields.iter().find(|f| f.key == key)) {
            if let Some(v) = field.default_value.as_str() {
                let v = v.trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// 找到「opx 可以改」的监听地址字段。
///
/// 优先实例 config 里已有的键（改它不会引入新键），其次 provider schema 声明的键
/// （如 MySQL 的 `bind-address`）。都找不到返回 None：说明该软件的监听地址写死在
/// 程序或私有配置里，一键改无法生效，前端应隐藏按钮并提示手动改。
fn bind_key_of(
    config: &serde_json::Value,
    schema: Option<&opx_core::models::software::ConfigSchema>,
) -> Option<&'static str> {
    for key in BIND_KEYS {
        if config
            .get(key)
            .and_then(|v| v.as_str())
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
        {
            return Some(key);
        }
    }
    let schema = schema?;
    BIND_KEYS
        .into_iter()
        .find(|k| schema.fields.iter().any(|f| f.key == *k))
}

/// 是否为「仅本机」地址。
///
/// 用标准库判定而不是字符串前缀：`127.0.0.1.example.com` 这种主机名以 `127.`
/// 开头却不是回环，前缀匹配会误报「仅本机」—— 那会让用户去改一个根本不存在的
/// 问题（我们只在确定是回环时才提示改 bind）。
fn is_loopback(bind: &str) -> bool {
    let b = bind.trim();
    if b.eq_ignore_ascii_case("localhost") {
        return true;
    }
    b.parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// 一条端口的监听信息。
struct BindView {
    addr: Option<String>,
    local_only: bool,
    /// "listen" | "config" | "unknown"
    source: &'static str,
}

/// 端口 → (地址, 是否仅本机)。同端口可能有多条监听记录（`0.0.0.0` 与 `::`、
/// 多网卡分别绑），取「最宽松」的那条——只要有一条不是回环，外部就能连上。
fn live_binds() -> HashMap<u16, (String, bool)> {
    let mut map: HashMap<u16, (String, bool)> = HashMap::new();
    for b in netutils::listening_binds() {
        map.entry(b.port)
            .and_modify(|cur| {
                if cur.1 && !b.local_only {
                    *cur = (b.addr.clone(), false);
                }
            })
            .or_insert((b.addr, b.local_only));
    }
    map
}

/// 判定某端口的监听地址：**真实套接字优先**，服务没跑时才退回配置推断。
///
/// 为什么不只看配置：监听地址可能写在程序自己生成的配置文件里（MySQL 的
/// `my.cnf` 的 `bind-address`）、命令行参数里、甚至代码里。配置推断会漏，
/// 而漏掉的后果就是「放行了防火墙却连不上」查不出来。真实套接字不会有假。
fn bind_view(
    live: &HashMap<u16, (String, bool)>,
    port: u16,
    fallback: Option<String>,
) -> BindView {
    if let Some((addr, local_only)) = live.get(&port) {
        return BindView {
            addr: Some(addr.clone()),
            local_only: *local_only,
            source: "listen",
        };
    }
    match fallback {
        Some(addr) => {
            let local_only = is_loopback(&addr);
            BindView {
                addr: Some(addr),
                local_only,
                source: "config",
            }
        }
        None => BindView {
            addr: None,
            local_only: false,
            source: "unknown",
        },
    }
}

/// 汇总所有端口来源 + 防火墙放行状态。
#[tauri::command]
pub fn get_firewall_overview(
    sm: State<'_, Arc<SoftwareManager>>,
    sb: State<'_, Arc<SpringBootManager>>,
    wm: State<'_, Arc<WebsiteManager>>,
) -> FirewallOverview {
    let kind = firewall::backend();
    let backend = firewall::info(firewall::detection());
    let settings = read_settings().unwrap_or_default();

    // 读不到规则（非 root / 后端异常）时不阻断清单展示：用户需要看到
    // 「哪些端口该放行」，只是放行状态不可信，用 error 明确告知。
    let (allowed_rules, error) = match firewall::list_allowed(kind) {
        Ok(rules) => (rules, None),
        Err(e) => (Vec::new(), Some(format!("{e:#}"))),
    };
    // 该端口在系统防火墙里是否存在任意放行规则（含来源限定规则）：
    // 只查全网规则会把「仅限某 IP」的端口误显示成未放行。
    let has_any = |port: u16, proto: &str| -> bool {
        allowed_rules
            .iter()
            .any(|r| r.port == port && r.proto.as_str() == proto)
    };
    // 该端口当前的「来源限定」规则（从系统防火墙读回展示——防火墙本身是
    // 事实源，被管软件 / JAR / 站点 / opx 的端口都按此展示，不落设置）。
    let sources_of = |port: u16, proto: &str| -> Vec<String> {
        let mut out: Vec<String> = allowed_rules
            .iter()
            .filter(|r| r.port == port && r.proto.as_str() == proto && r.source.is_some())
            .map(|r| r.source.clone().unwrap_or_default())
            .collect();
        out.sort();
        out.dedup();
        out
    };

    let mut ports: Vec<FirewallPortEntry> = Vec::new();
    // 真实监听地址只采一次，五个来源共用（采集要读 /proc 或跑 netstat）
    let live = live_binds();

    // ---- 1. opx 自身 Web 入口 ----
    // bind 由 web_lan_access 决定（见 services/web_server）：false = 127.0.0.1。
    let web_port = settings.web_port;
    let b = bind_view(
        &live,
        web_port,
        Some(if settings.web_lan_access {
            "0.0.0.0".to_string()
        } else {
            "127.0.0.1".to_string()
        }),
    );
    ports.push(FirewallPortEntry {
        port: web_port,
        proto: "tcp".to_string(),
        source: "opx".to_string(),
        label: String::new(),
        detail: String::new(),
        instance_id: None,
        allowed: has_any(web_port, "tcp"),
        sources: sources_of(web_port, "tcp"),
        bind: b.addr,
        bind_local_only: b.local_only,
        bind_source: b.source.to_string(),
        bind_fixable: false,
    });

    // ---- 2. 内置软件 + 自定义软件 ----
    let providers_list = providers::all_providers();
    for sw in sm.get_installed() {
        let provider = providers_list.iter().find(|p| p.key() == sw.key);
        let schema = provider.and_then(|p| p.config_schema());
        let fixable = bind_key_of(&sw.config, schema.as_ref()).is_some();
        for port in collect_configured_ports(&sw, provider.map(|p| &**p)) {
            let b = bind_view(&live, port, bind_of(&sw.config, schema.as_ref()));
            ports.push(FirewallPortEntry {
                port,
                proto: "tcp".to_string(),
                source: "software".to_string(),
                label: sw.name.clone(),
                // 安装时 name 已经是「<软件名> <版本>」，再从 version 拼一遍会在
                // 行内出现两行相同文本（MinIO 的 RELEASE.xxx 尤其明显）——已含则留空
                detail: if sw.name.ends_with(&sw.version) {
                    String::new()
                } else {
                    sw.version.clone()
                },
                instance_id: Some(sw.id.clone()),
                allowed: has_any(port, "tcp"),
                sources: sources_of(port, "tcp"),
                bind: b.addr,
                bind_local_only: b.local_only,
                bind_source: b.source.to_string(),
                bind_fixable: fixable,
            });
        }
    }

    // ---- 3. JAR 应用（Spring Boot）----
    // Spring Boot 默认监听 0.0.0.0，端口未知时不列出（表单里没填、JAR 里也没读到）
    for app in sb.export_apps() {
        let Some(port) = app.port else { continue };
        let b = bind_view(&live, port, Some("0.0.0.0".to_string()));
        ports.push(FirewallPortEntry {
            port,
            proto: "tcp".to_string(),
            source: "jar".to_string(),
            label: app.name.clone(),
            detail: String::new(),
            instance_id: None,
            allowed: has_any(port, "tcp"),
            sources: sources_of(port, "tcp"),
            bind: b.addr,
            bind_local_only: b.local_only,
            bind_source: b.source.to_string(),
            bind_fixable: false,
        });
    }

    // ---- 4. 站点（nginx）----
    // 只列已启用的站点：未启用的站点不生成 conf，nginx 不会监听该端口。
    // 开了 SSL 的站点额外占用 443。
    for site in wm.list() {
        if !site.enabled {
            continue;
        }
        let detail = site
            .server_name
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| site.name.clone());
        let b = bind_view(&live, site.listen, Some("0.0.0.0".to_string()));
        ports.push(FirewallPortEntry {
            port: site.listen,
            proto: "tcp".to_string(),
            source: "site".to_string(),
            label: site.name.clone(),
            detail,
            instance_id: None,
            allowed: has_any(site.listen, "tcp"),
            sources: sources_of(site.listen, "tcp"),
            bind: b.addr,
            bind_local_only: b.local_only,
            bind_source: b.source.to_string(),
            bind_fixable: false,
        });
        if site.ssl.enabled && site.listen != 443 {
            let b = bind_view(&live, 443, Some("0.0.0.0".to_string()));
            ports.push(FirewallPortEntry {
                port: 443,
                proto: "tcp".to_string(),
                source: "site".to_string(),
                label: site.name.clone(),
                detail: "SSL".to_string(),
                instance_id: None,
                allowed: has_any(443, "tcp"),
                sources: sources_of(443, "tcp"),
                bind: b.addr,
                bind_local_only: b.local_only,
                bind_source: b.source.to_string(),
                bind_fixable: false,
            });
        }
    }

    // ---- 5. 用户手动添加的端口 ----
    for extra in &settings.firewall_extra_ports {
        let Some(proto) = Proto::parse(&extra.proto) else { continue };
        // 一条手动记录 = 一组规则：无来源 = 1 条全网规则；有来源 = 每来源 1 条。
        // 全部规则都已放行才显示「已放行」（部分放行说明中途出错，须提示重开）。
        let rules = manual_rules(extra.port, proto, &extra.sources);
        let allowed = !rules.is_empty() && rules.iter().all(|r| allowed_rules.contains(r));
        let b = bind_view(&live, extra.port, None);
        ports.push(FirewallPortEntry {
            port: extra.port,
            proto: proto.as_str().to_string(),
            source: "manual".to_string(),
            label: String::new(),
            detail: String::new(),
            instance_id: None,
            allowed,
            sources: extra.sources.clone(),
            bind: b.addr,
            bind_local_only: b.local_only,
            bind_source: b.source.to_string(),
            bind_fixable: false,
        });
    }

    FirewallOverview {
        backend,
        ports,
        error,
        web_enabled: settings.web_enabled,
        web_port,
    }
}

fn parse_proto(s: &str) -> Result<Proto, String> {
    Proto::parse(s).ok_or_else(|| format!("协议只能是 tcp 或 udp，收到「{s}」"))
}

/// 手动端口记录展开成防火墙规则：无来源 = 1 条全网规则；有来源 = 每来源 1 条。
/// 来源不合法的条目被丢弃（写入时已校验，这里只是防御历史脏数据）。
fn manual_rules(port: u16, proto: Proto, sources: &[String]) -> Vec<PortRule> {
    if sources.is_empty() {
        return vec![PortRule { port, proto, source: None }];
    }
    sources
        .iter()
        .filter_map(|s| {
            let s = s.trim();
            firewall::validate_source(s).ok()?;
            Some(PortRule { port, proto, source: Some(s.to_string()) })
        })
        .collect()
}

/// 放行 / 撤销某个端口的规则。`source` 为 Some 时只操作「仅该来源」那条规则。
///
/// 返回 `true` 表示规则确实发生了变化，`false` 表示本来就是目标状态（幂等）。
#[tauri::command]
pub fn set_firewall_port(
    port: u16,
    proto: String,
    allow: bool,
    source: Option<String>,
) -> Result<bool, String> {
    let proto = parse_proto(&proto)?;
    let source = source.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    if let Some(s) = &source {
        firewall::validate_source(s).map_err(|e| format!("{e:#}"))?;
    }
    let rule = PortRule { port, proto, source };
    let target = rule.spec_full();
    let detail = if allow { "放行" } else { "撤销" };
    let kind = firewall::backend();
    audited!("set_firewall_port", target, detail, {
        let changed = if allow {
            firewall::allow(kind, &rule)
        } else {
            firewall::revoke(kind, &rule)
        };
        changed.map_err(|e| format!("{e:#}"))
    })
}

/// 比较某端口规则现状与目标：返回需要撤销、需要写入的规则（纯函数，便于单测）。
fn rules_diff(current: &[PortRule], target: &[PortRule]) -> (Vec<PortRule>, Vec<PortRule>) {
    let to_revoke = current.iter().filter(|r| !target.contains(r)).cloned().collect();
    let to_add = target.iter().filter(|r| !current.contains(r)).cloned().collect();
    (to_revoke, to_add)
}

/// 整组设置某端口的放行来源（覆盖式，适用于所有端口来源：被管软件 / JAR /
/// 站点 / opx 自身 / 手动端口）。
///
/// `sources` 空 = 对任意来源放行：撤销该端口现有全部规则（含来源限定），
/// 写一条全网规则；非空 = 撤销该端口现有全部规则，再按来源逐条写入。
/// 与 `set_firewall_port` 的单条操作不同，本命令保证端口规则集精确等于目标。
#[tauri::command]
pub fn set_firewall_port_sources(
    port: u16,
    proto: String,
    sources: Option<Vec<String>>,
) -> Result<(), String> {
    let proto = parse_proto(&proto)?;
    if port == 0 {
        return Err("端口不能为 0".to_string());
    }
    let mut sources = sources.unwrap_or_default();
    sources.retain(|s| !s.trim().is_empty());
    for s in &sources {
        firewall::validate_source(s).map_err(|e| format!("{e:#}"))?;
    }

    let kind = firewall::backend();
    let target: Vec<PortRule> = if sources.is_empty() {
        vec![PortRule { port, proto, source: None }]
    } else {
        sources
            .iter()
            .map(|s| PortRule { port, proto, source: Some(s.clone()) })
            .collect()
    };
    audited!("set_firewall_port_sources", format!("{} ×{}", PortRule { port, proto, source: None }.spec(), target.len()), "", {
        let current: Vec<PortRule> = firewall::list_allowed(kind)
            .map_err(|e| format!("{e:#}"))?
            .into_iter()
            .filter(|r| r.port == port && r.proto == proto)
            .collect();
        // 先撤掉不在目标里的现有规则（全网 + 旧来源），再补上缺的目标规则；
        // 两步都幂等，重复执行不会产生重复规则。
        let (to_revoke, to_add) = rules_diff(&current, &target);
        for r in &to_revoke {
            firewall::revoke(kind, r).map_err(|e| format!("{e:#}"))?;
        }
        for r in &to_add {
            firewall::allow(kind, r).map_err(|e| format!("{e:#}"))?;
        }
        Ok(())
    })
}

/// 手动添加一个放行端口（写入设置并立即放行）。
///
/// `sources` 为 None/空 = 对任意来源放行；非空 = 每个元素（IP/CIDR）一条
/// 「仅该来源」的规则，全部作为一条手动记录管理。
#[tauri::command]
pub fn add_manual_port(
    port: u16,
    proto: String,
    sources: Option<Vec<String>>,
) -> Result<(), String> {
    let proto = parse_proto(&proto)?;
    if port == 0 {
        return Err("端口不能为 0".to_string());
    }
    let mut sources = sources.unwrap_or_default();
    sources.retain(|s| !s.trim().is_empty());
    for s in &sources {
        firewall::validate_source(s).map_err(|e| format!("{e:#}"))?;
    }
    let mut settings = read_settings().map_err(|e| format!("读取设置失败: {e}"))?;
    if settings
        .firewall_extra_ports
        .iter()
        .any(|p| p.port == port && p.proto == proto.as_str())
    {
        return Err(format!("{} 已在列表中", PortRule::tcp(port).spec()));
    }

    let kind = firewall::backend();
    let rules = manual_rules(port, proto, &sources);
    audited!("add_manual_port", format!("{} ×{}", PortRule::tcp(port).spec(), rules.len()), "", {
        for rule in &rules {
            firewall::allow(kind, rule).map_err(|e| format!("{e:#}"))?;
        }
        settings.firewall_extra_ports.push(ManualPortRule {
            port,
            proto: proto.as_str().to_string(),
            sources,
        });
        write_settings_file(&settings).map_err(|e| format!("保存设置失败: {e}"))
    })
}

/// 移除手动添加的端口（从设置里删掉并撤销该端口的全部规则，含来源限定）。
///
/// 只允许删手动添加的端口：软件/站点/JAR 的端口由各自实例决定，
/// 从这里删会造成「配置还在、规则没了」的不一致，应去对应页面改端口。
#[tauri::command]
pub fn remove_manual_port(port: u16, proto: String) -> Result<(), String> {
    let proto = parse_proto(&proto)?;
    let mut settings = read_settings().map_err(|e| format!("读取设置失败: {e}"))?;
    let entry = settings
        .firewall_extra_ports
        .iter()
        .find(|p| p.port == port && p.proto == proto.as_str())
        .cloned();
    let before = settings.firewall_extra_ports.len();
    settings
        .firewall_extra_ports
        .retain(|p| !(p.port == port && p.proto == proto.as_str()));
    if settings.firewall_extra_ports.len() == before {
        return Err("该端口不在手动添加列表里".to_string());
    }

    let kind = firewall::backend();
    let rules = entry
        .as_ref()
        .map(|e| manual_rules(port, proto, &e.sources))
        .unwrap_or_else(|| vec![PortRule { port, proto, source: None }]);
    audited!("remove_manual_port", PortRule::tcp(port).spec(), "", {
        // 撤销失败不回滚设置：防火墙规则可以手动清理，设置里留着会让用户
        // 永远删不掉这一项。宁可留下一条孤立规则，也不制造删不掉的条目。
        let mut revoke_err = None;
        for rule in &rules {
            if let Err(e) = firewall::revoke(kind, rule) {
                revoke_err.get_or_insert(e);
            }
        }
        write_settings_file(&settings).map_err(|e| format!("保存设置失败: {e}"))?;
        if let Some(e) = revoke_err {
            return Err(format!("已从列表移除，但撤销防火墙规则失败：{e:#}"));
        }
        Ok(())
    })
}

/// 把某实例的监听地址改成「对外可访问地址」（仅改配置，需重启该实例才生效）。
///
/// 目标地址优先取系统配置的「对外访问地址」（`service_bind_address`），未配置
/// 时回退 `0.0.0.0`：前者能精确绑到某块网卡（多网卡机器上更可控），后者总是可达。
///
/// 两种情形都要覆盖：
/// - 实例配置里已有 `bind`（用户改过）→ 直接覆盖；
/// - 实例配置里没有该键（用户从没动过，默认值写在 provider schema 里，
///   例如 consul/redis 默认 `127.0.0.1`）→ 用 schema 声明的键名写入目标地址，
///   相当于显式覆盖默认值。
///
/// 找不到这类字段时明确报错而不是静默成功 —— 否则用户会以为已改好，
/// 实际仍只监听本机。
#[tauri::command]
pub fn set_instance_bind_all(
    sm: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
) -> Result<(), String> {
    let target_addr = {
        let configured = read_settings()
            .map(|s| s.service_bind_address.trim().to_string())
            .unwrap_or_default();
        if configured.is_empty() || configured.eq_ignore_ascii_case("auto") {
            netutils::default_service_bind()
        } else {
            configured
        }
    };
    set_instance_bind(sm, installed_id, target_addr)
}

/// 同 [`set_instance_bind_all`]，但目标地址由调用方指定（配置页选中哪个地址就绑哪个）。
#[tauri::command]
pub fn set_instance_bind(
    sm: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
    address: String,
) -> Result<(), String> {
    let address = address.trim().to_string();
    if address.is_empty() {
        return Err("监听地址不能为空".to_string());
    }
    let software = sm
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {installed_id}"))?;

    let providers_list = providers::all_providers();
    let provider = providers_list.iter().find(|p| p.key() == software.key);
    let schema = provider.and_then(|p| p.config_schema());

    let key = bind_key_of(&software.config, schema.as_ref())
        .map(|k| k.to_string())
        .ok_or_else(|| {
            "该软件的监听地址不在可配置项里（bind / bind_address / bind-address / bind_ip），请在软件的配置文件中手动调整"
                .to_string()
        })?;

    let mut config = software.config.clone();
    if !config.is_object() {
        // 配置为空/非对象（从未保存过）时先归一成空对象，再写入键
        config = serde_json::json!({});
    }
    if let Some(obj) = config.as_object_mut() {
        obj.insert(key.clone(), serde_json::json!(address.clone()));
    }

    // 同步写进 provider 的配置文件：MySQL 的监听地址只认 my.cnf 里的
    // `bind-address`，只改 installed.json 的话，重启后依旧只监听 127.0.0.1 ——
    // 用户点完「改为对外地址」还是连不上，比不给按钮更糟。
    if let (Some(provider), Some(schema)) = (provider, schema.as_ref()) {
        let cctx = ConfigContext {
            install_path: software.install_path.clone(),
            version: software.version.clone(),
            config: software.config.clone(),
        };
        if let Some(rel) = provider.config_file_path(&cctx) {
            let full = std::path::Path::new(&software.install_path).join(rel);
            if full.exists() {
                let mut form = config_editor::FormData::new();
                form.insert(key.clone(), serde_json::json!(address.clone()));
                config_editor::write_form_to_config(&full, schema, &form)
                    .map_err(|e| format!("写入配置文件失败：{e:#}"))?;
            }
        }
    }

    let target = software.name.clone();
    let detail = format!("{key}={address}");
    audited!("set_instance_bind_all", target, detail, {
        sm.update_config(&installed_id, config)
            .map_err(|e| format!("{e:#}"))
    })
}

#[cfg(test)]
mod tests {
    use super::{bind_of, is_loopback};

    /// bind 字段名在各 provider 之间不统一，顺序必须稳定（先 `bind` 再 `bind_address`）。
    #[test]
    fn bind_of_picks_known_keys_in_order() {
        assert_eq!(
            bind_of(&serde_json::json!({ "bind": "0.0.0.0" }), None).as_deref(),
            Some("0.0.0.0")
        );
        assert_eq!(
            bind_of(&serde_json::json!({ "bind_address": "127.0.0.1" }), None).as_deref(),
            Some("127.0.0.1")
        );
        // 空白值等于没配，应继续往后找（否则会把「空 bind」显示成已配置）
        assert_eq!(
            bind_of(&serde_json::json!({ "bind": "   ", "host": "localhost" }), None).as_deref(),
            Some("localhost")
        );
        assert!(bind_of(&serde_json::json!({}), None).is_none());
        assert!(bind_of(&serde_json::json!({ "port": 3306 }), None).is_none());
    }

    /// provider 的「默认只绑本机」写在 schema 字段默认值里，实例配置里根本没有这个键。
    /// 不回退到 schema 就会漏报「仅监听本机」——而用户恰恰卡在这一步（防火墙开了也连不上）。
    #[test]
    fn bind_of_falls_back_to_schema_default() {
        use opx_core::models::software::{ConfigField, ConfigFieldType, ConfigSchema};

        let schema = ConfigSchema {
            fields: vec![ConfigField {
                key: "bind".to_string(),
                label_i18n: "configField.bind".to_string(),
                field_type: ConfigFieldType::Text,
                default_value: serde_json::json!("127.0.0.1"),
                section: None,
                description_i18n: None,
            }],
            ephemeral_keys: Vec::new(),
            field_rules: Vec::new(),
        };
        // 实例从未改过配置 → 取 schema 默认值（consul/redis 的真实形状）
        assert_eq!(
            bind_of(&serde_json::json!({}), Some(&schema)).as_deref(),
            Some("127.0.0.1")
        );
        // 实例显式配过 → 实例优先
        assert_eq!(
            bind_of(&serde_json::json!({ "bind": "0.0.0.0" }), Some(&schema)).as_deref(),
            Some("0.0.0.0")
        );
    }

    #[test]
    fn is_loopback_accepts_only_real_loopback() {
        for v in ["127.0.0.1", "127.0.0.53", "::1", "localhost", "LOCALHOST", " 127.0.0.1 "] {
            assert!(is_loopback(v), "{v} 应判定为回环");
        }
        for v in ["0.0.0.0", "::", "192.168.1.5", "", "example.com", "127.0.0.1.example.com"] {
            assert!(!is_loopback(v), "{v} 不应判定为回环");
        }
    }

    /// set_firewall_port_sources 的 diff：目标与现状一致时不产生任何操作（幂等）。
    #[test]
    fn rules_diff_noop_when_target_matches_current() {
        use super::rules_diff;
        use opx_core::services::firewall::PortRule;

        let cur = vec![
            PortRule::tcp(3306),
            PortRule::sourced(3306, opx_core::services::firewall::Proto::Tcp, "192.168.200.1"),
        ];
        let (revoke, add) = rules_diff(&cur, &cur);
        assert!(revoke.is_empty() && add.is_empty());
    }

    /// 从「全网放行」切到「仅限某 IP」：必须撤掉全网规则（否则来源限定形同虚设）。
    #[test]
    fn rules_diff_switching_to_sourced_revokes_unrestricted() {
        use super::rules_diff;
        use opx_core::services::firewall::{PortRule, Proto};

        let cur = vec![PortRule::tcp(6379)];
        let target = vec![PortRule::sourced(6379, Proto::Tcp, "10.0.0.0/8")];
        let (revoke, add) = rules_diff(&cur, &target);
        assert_eq!(revoke, vec![PortRule::tcp(6379)]);
        assert_eq!(add, vec![PortRule::sourced(6379, Proto::Tcp, "10.0.0.0/8")]);
    }

    /// 来源集合增删：只动差异部分，同端口多来源互不影响。
    #[test]
    fn rules_diff_updates_only_changed_sources() {
        use super::rules_diff;
        use opx_core::services::firewall::{PortRule, Proto};

        let a = "192.168.200.1";
        let b = "10.0.0.0/8";
        let c = "172.16.0.5";
        let cur = vec![
            PortRule::sourced(8500, Proto::Tcp, a),
            PortRule::sourced(8500, Proto::Tcp, b),
        ];
        let target = vec![
            PortRule::sourced(8500, Proto::Tcp, b),
            PortRule::sourced(8500, Proto::Tcp, c),
        ];
        let (revoke, add) = rules_diff(&cur, &target);
        assert_eq!(revoke, vec![PortRule::sourced(8500, Proto::Tcp, a)]);
        assert_eq!(add, vec![PortRule::sourced(8500, Proto::Tcp, c)]);
    }
}
