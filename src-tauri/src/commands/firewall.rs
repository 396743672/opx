//! 防火墙端口规则管理命令。
//!
//! 设计边界完整说明见 `opx_core::services::firewall` 模块文档，要点：
//! **只维护端口规则，绝不启停防火墙服务**；不碰 iptables/nftables 裸规则。
//!
//! 本层职责是「把系统里所有会监听端口的来源汇总成一张可操作的清单」：
//! opx 自身 Web 入口 / 内置与自定义软件 / JAR 应用 / 站点 / 用户手动添加。

use std::sync::Arc;

use tauri::State;

use opx_core::models::settings::ManualPortRule;
use opx_core::services::firewall::{self, BackendInfo, PortRule, Proto};
use opx_core::services::software_manager::providers;
use opx_core::services::software_manager::start_stop::collect_configured_ports;
use opx_core::services::software_manager::SoftwareManager;
use opx_core::services::springboot_manager::SpringBootManager;
use opx_core::services::website_manager::WebsiteManager;
use opx_core::utils::settings::{read_settings, write_settings_file};
use opx_core::audited;

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
    /// 服务实际监听地址（能从配置判定时给出）
    pub bind: Option<String>,
    /// true = 仅监听回环地址，即便放行防火墙也无法从外部访问
    pub bind_local_only: bool,
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
    const KEYS: [&str; 5] = ["bind", "bind_address", "bind_ip", "listen_host", "host"];
    for key in KEYS {
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

/// 汇总所有端口来源 + 防火墙放行状态。
#[tauri::command]
pub fn get_firewall_overview(
    sm: State<'_, Arc<SoftwareManager>>,
    sb: State<'_, Arc<SpringBootManager>>,
    wm: State<'_, Arc<WebsiteManager>>,
) -> FirewallOverview {
    let kind = firewall::backend();
    let backend = firewall::info(kind);
    let settings = read_settings().unwrap_or_default();

    // 读不到规则（非 root / 后端异常）时不阻断清单展示：用户需要看到
    // 「哪些端口该放行」，只是放行状态不可信，用 error 明确告知。
    let (allowed_rules, error) = match firewall::list_allowed(kind) {
        Ok(rules) => (rules, None),
        Err(e) => (Vec::new(), Some(format!("{e:#}"))),
    };
    let is_allowed = |rule: PortRule| allowed_rules.contains(&rule);

    let mut ports: Vec<FirewallPortEntry> = Vec::new();

    // ---- 1. opx 自身 Web 入口 ----
    // bind 由 web_lan_access 决定（见 services/web_server）：false = 127.0.0.1。
    let web_port = settings.web_port;
    ports.push(FirewallPortEntry {
        port: web_port,
        proto: "tcp".to_string(),
        source: "opx".to_string(),
        label: String::new(),
        detail: String::new(),
        instance_id: None,
        allowed: is_allowed(PortRule::tcp(web_port)),
        bind: Some(if settings.web_lan_access {
            "0.0.0.0".to_string()
        } else {
            "127.0.0.1".to_string()
        }),
        bind_local_only: !settings.web_lan_access,
    });

    // ---- 2. 内置软件 + 自定义软件 ----
    let providers_list = providers::all_providers();
    for sw in sm.get_installed() {
        let provider = providers_list.iter().find(|p| p.key() == sw.key);
        let schema = provider.and_then(|p| p.config_schema());
        let bind = bind_of(&sw.config, schema.as_ref());
        let local_only = bind.as_deref().map(is_loopback).unwrap_or(false);
        for port in collect_configured_ports(&sw, provider.map(|p| &**p)) {
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
                allowed: is_allowed(PortRule::tcp(port)),
                bind: bind.clone(),
                bind_local_only: local_only,
            });
        }
    }

    // ---- 3. JAR 应用（Spring Boot）----
    // Spring Boot 默认监听 0.0.0.0，端口未知时不列出（表单里没填、JAR 里也没读到）
    for app in sb.export_apps() {
        let Some(port) = app.port else { continue };
        ports.push(FirewallPortEntry {
            port,
            proto: "tcp".to_string(),
            source: "jar".to_string(),
            label: app.name.clone(),
            detail: String::new(),
            instance_id: None,
            allowed: is_allowed(PortRule::tcp(port)),
            bind: Some("0.0.0.0".to_string()),
            bind_local_only: false,
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
        ports.push(FirewallPortEntry {
            port: site.listen,
            proto: "tcp".to_string(),
            source: "site".to_string(),
            label: site.name.clone(),
            detail,
            instance_id: None,
            allowed: is_allowed(PortRule::tcp(site.listen)),
            bind: Some("0.0.0.0".to_string()),
            bind_local_only: false,
        });
        if site.ssl.enabled && site.listen != 443 {
            ports.push(FirewallPortEntry {
                port: 443,
                proto: "tcp".to_string(),
                source: "site".to_string(),
                label: site.name.clone(),
                detail: "SSL".to_string(),
                instance_id: None,
                allowed: is_allowed(PortRule::tcp(443)),
                bind: Some("0.0.0.0".to_string()),
                bind_local_only: false,
            });
        }
    }

    // ---- 5. 用户手动添加的端口 ----
    for extra in &settings.firewall_extra_ports {
        let Some(proto) = Proto::parse(&extra.proto) else { continue };
        let rule = PortRule { port: extra.port, proto };
        ports.push(FirewallPortEntry {
            port: extra.port,
            proto: proto.as_str().to_string(),
            source: "manual".to_string(),
            label: String::new(),
            detail: String::new(),
            instance_id: None,
            allowed: is_allowed(rule),
            bind: None,
            bind_local_only: false,
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

/// 放行 / 撤销某个端口的规则。
///
/// 返回 `true` 表示规则确实发生了变化，`false` 表示本来就是目标状态（幂等）。
#[tauri::command]
pub fn set_firewall_port(port: u16, proto: String, allow: bool) -> Result<bool, String> {
    let proto = parse_proto(&proto)?;
    let rule = PortRule { port, proto };
    let target = rule.spec();
    let detail = if allow { "放行" } else { "撤销" };
    let kind = firewall::backend();
    audited!("set_firewall_port", target, detail, {
        let changed = if allow {
            firewall::allow(kind, rule)
        } else {
            firewall::revoke(kind, rule)
        };
        changed.map_err(|e| format!("{e:#}"))
    })
}

/// 手动添加一个放行端口（写入设置并立即放行）。
#[tauri::command]
pub fn add_manual_port(port: u16, proto: String) -> Result<(), String> {
    let proto = parse_proto(&proto)?;
    if port == 0 {
        return Err("端口不能为 0".to_string());
    }
    let mut settings = read_settings().map_err(|e| format!("读取设置失败: {e}"))?;
    if settings
        .firewall_extra_ports
        .iter()
        .any(|p| p.port == port && p.proto == proto.as_str())
    {
        return Err(format!("{} 已在列表中", PortRule { port, proto }.spec()));
    }

    let kind = firewall::backend();
    let rule = PortRule { port, proto };
    audited!("add_manual_port", rule.spec(), "", {
        firewall::allow(kind, rule).map_err(|e| format!("{e:#}"))?;
        settings.firewall_extra_ports.push(ManualPortRule {
            port,
            proto: proto.as_str().to_string(),
        });
        write_settings_file(&settings).map_err(|e| format!("保存设置失败: {e}"))
    })
}

/// 移除手动添加的端口（从设置里删掉并撤销放行）。
///
/// 只允许删手动添加的端口：软件/站点/JAR 的端口由各自实例决定，
/// 从这里删会造成「配置还在、规则没了」的不一致，应去对应页面改端口。
#[tauri::command]
pub fn remove_manual_port(port: u16, proto: String) -> Result<(), String> {
    let proto = parse_proto(&proto)?;
    let mut settings = read_settings().map_err(|e| format!("读取设置失败: {e}"))?;
    let before = settings.firewall_extra_ports.len();
    settings
        .firewall_extra_ports
        .retain(|p| !(p.port == port && p.proto == proto.as_str()));
    if settings.firewall_extra_ports.len() == before {
        return Err("该端口不在手动添加列表里".to_string());
    }

    let kind = firewall::backend();
    let rule = PortRule { port, proto };
    audited!("remove_manual_port", rule.spec(), "", {
        // 撤销失败不回滚设置：防火墙规则可以手动清理，设置里留着会让用户
        // 永远删不掉这一项。宁可留下一条孤立规则，也不制造删不掉的条目。
        let revoke_err = firewall::revoke(kind, rule).err();
        write_settings_file(&settings).map_err(|e| format!("保存设置失败: {e}"))?;
        if let Some(e) = revoke_err {
            return Err(format!("已从列表移除，但撤销防火墙规则失败：{e:#}"));
        }
        Ok(())
    })
}

/// 把某实例的监听地址改为 `0.0.0.0`（仅改配置，需重启该实例才生效）。
///
/// 两种情形都要覆盖：
/// - 实例配置里已有 `bind`（用户改过）→ 直接覆盖；
/// - 实例配置里没有该键（用户从没动过，默认值写在 provider schema 里，
///   例如 consul/redis 默认 `127.0.0.1`）→ 用 schema 声明的键名写入 `0.0.0.0`，
///   相当于显式覆盖默认值。
///
/// 找不到这类字段时明确报错而不是静默成功 —— 否则用户会以为已改好，
/// 实际仍只监听本机。
#[tauri::command]
pub fn set_instance_bind_all(
    sm: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
) -> Result<(), String> {
    const KEYS: [&str; 3] = ["bind", "bind_address", "bind_ip"];

    let software = sm
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {installed_id}"))?;

    let providers_list = providers::all_providers();
    let provider = providers_list.iter().find(|p| p.key() == software.key);
    let schema = provider.and_then(|p| p.config_schema());

    let key = KEYS
        .iter()
        .find(|k| software.config.get(**k).is_some())
        .map(|k| k.to_string())
        .or_else(|| {
            schema.as_ref().and_then(|s| {
                s.fields
                    .iter()
                    .find(|f| KEYS.contains(&f.key.as_str()))
                    .map(|f| f.key.clone())
            })
        })
        .ok_or_else(|| {
            "该软件的配置里没有可修改的监听地址字段（bind / bind_address / bind_ip），请到配置编辑中手动调整"
                .to_string()
        })?;

    let mut config = software.config.clone();
    if !config.is_object() {
        // 配置为空/非对象（从未保存过）时先归一成空对象，再写入键
        config = serde_json::json!({});
    }
    if let Some(obj) = config.as_object_mut() {
        obj.insert(key.clone(), serde_json::json!("0.0.0.0"));
    }

    let target = software.name.clone();
    audited!("set_instance_bind_all", target, key, {
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
}
