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
            custom_conf: false,
        };
        let json = serde_json::to_string(&site).unwrap();
        let back: Site = serde_json::from_str(&json).unwrap();
        assert_eq!(back.locations.len(), 2);
        assert_eq!(back.locations[0].kind, LocationKind::Static);
        assert_eq!(back.locations[1].target.as_deref(), Some("http://127.0.0.1:8080"));
        assert!(back.enabled);
        assert!(!back.custom_conf);
    }

    #[test]
    fn site_custom_conf_defaults_false_when_absent() {
        // 旧 websites.json 无 custom_conf 字段时应回落为 false
        let json = r#"{"id":"s1","name":"官网","listen":80,"enabled":true}"#;
        let site: Site = serde_json::from_str(json).unwrap();
        assert!(!site.custom_conf);
    }

    #[test]
    fn location_defaults_spa_fallback_true_when_absent() {
        let json = r#"{"path":"/","kind":"Static"}"#;
        let loc: Location = serde_json::from_str(json).unwrap();
        assert!(loc.spa_fallback);
        assert!(loc.root.is_none());
    }
}
