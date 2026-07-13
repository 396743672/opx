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
