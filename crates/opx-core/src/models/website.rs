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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamTarget {
    pub addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyHeader {
    pub name: String,
    pub value: String,
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
    /// 多后端负载均衡目标（与 target 二选一，非空时生成 upstream 块）
    #[serde(default)]
    pub upstreams: Vec<UpstreamTarget>,
    /// 自定义 proxy_set_header（追加在默认头之后）
    #[serde(default)]
    pub proxy_headers: Vec<ProxyHeader>,
    /// 子路径改写：非空时 proxy_pass 目标拼接该子路径
    #[serde(default)]
    pub proxy_subpath: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SslConfig {
    pub enabled: bool,
    #[serde(default)]
    pub cert_path: Option<String>,
    #[serde(default)]
    pub key_path: Option<String>,
    /// 是否由 ACME（Let's Encrypt）签发
    #[serde(default)]
    pub acme: bool,
    /// ACME 证书到期时间（RFC3339 本地时间）；自签为空
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cert_expires_at: Option<String>,
    /// 签发/续期该站点证书所用的 DNS 账号 id（见 `models::dns_account`）。
    /// Option + skip：老 websites.json 反序列化后为 None，且不会因升级被改写出新字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_account_id: Option<String>,
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
    /// 是否为「手写模式」：true 时该站点的 conf 由用户在源码视图直接维护，
    /// regenerate 会跳过它、保留其手写 .conf，不再由表单自动重建覆盖。
    #[serde(default)]
    pub custom_conf: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebsiteList {
    pub websites: Vec<Site>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssl_config_without_acme_fields_defaults() {
        let c: SslConfig = serde_json::from_str(r#"{"enabled":true,"cert_path":"a.crt","key_path":"a.key"}"#).unwrap();
        assert!(c.enabled);
        assert!(!c.acme);
        assert!(c.cert_expires_at.is_none());
    }

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
}
