use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ArchiveFormat {
    Zip,
    TarGz,
    Executable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveInfo {
    pub format: ArchiveFormat,
    pub size: Option<u64>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinInfo {
    pub version: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorSource {
    pub name: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builtin: Option<BuiltinInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogVersion {
    pub version: String,
    pub mirrors: Vec<MirrorSource>,
    pub archive: ArchiveInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SoftwareCategory {
    Database,
    Runtime,
    Cache,
    WebServer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub key: String,
    pub name: String,
    pub description: String,
    pub category: SoftwareCategory,
    pub icon: String,
    pub versions: Vec<CatalogVersion>,
    pub default_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub entries: Vec<CatalogEntry>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SoftwareStatus {
    Running,
    Stopped,
    Error,
    Unknown,
    Starting,
    Stopping,
    Initializing,
}

impl Default for SoftwareStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InstallSource {
    Mirror {
        mirror_name: String,
        url: String,
    },
    Builtin {
        version: String,
    },
    Custom {
        archive_name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSoftware {
    pub id: String,
    pub key: String,
    pub version: String,
    pub name: String,
    pub install_path: String,
    pub install_time: NaiveDateTime,
    pub status: SoftwareStatus,
    pub port: u16,
    pub config: serde_json::Value,
    pub is_custom: bool,
    pub auto_start_on_app_start: bool,
    pub startup_order: u32,
    pub source: InstallSource,

    #[serde(default)]
    pub pid: Option<u32>,

    #[serde(default)]
    pub last_started_at: Option<NaiveDateTime>,

    #[serde(default)]
    pub last_stopped_at: Option<NaiveDateTime>,

    #[serde(default)]
    pub last_error: Option<String>,

    #[serde(default)]
    pub custom_start_command: Option<CustomStartCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSoftwareList {
    pub software: Vec<InstalledSoftware>,
}

impl Default for InstalledSoftwareList {
    fn default() -> Self {
        Self {
            software: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustomStartCommand {
    pub executable: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub env_vars: std::collections::BTreeMap<String, String>,
    pub health_check: CustomHealthSpec,
    #[serde(default)]
    pub config_file_relative: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "spec")]
pub enum CustomHealthSpec {
    None,
    Tcp { port: u16 },
    Http { url: String, expected_status: u16 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSchema {
    pub fields: Vec<ConfigField>,
    /// 标记为 ephemeral 的字段 key 列表。这些字段（如 MySQL 初始化密码）只用于首次初始化，
    /// 是敏感的一次性值，绝不写入配置文件 / installed.json（不落盘）。
    /// 前端将其渲染为红色敏感字段；后端 write_config_form / write_form_to_config 会跳过它们。
    #[serde(default)]
    pub ephemeral_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigField {
    pub key: String,
    pub label_i18n: String,
    pub field_type: ConfigFieldType,
    pub default_value: serde_json::Value,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub description_i18n: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ConfigFieldType {
    Text,
    Number,
    Port,
    Password,
    Select { options: Vec<String> },
    /// 数值 + 单位下拉：值形如 "256mb"/"512M"，数字可填、单位只能从 units 里选（防手写单位出错）
    Size { units: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "spec")]
pub enum HealthCheckSpec {
    ProcessOnly,
    Tcp { port: u16, timeout_ms: u64 },
    Http {
        url: String,
        expected_status: u16,
        timeout_ms: u64,
    },
}

/// Provider 内部使用的自定义健康检查函数指针（不参与序列化）
pub type CustomHealthChecker = std::sync::Arc<dyn Fn() -> bool + Send + Sync>;

/// 调度器实际执行的 spec（包装 Custom 函数指针，避免序列化边界问题）
#[derive(Clone)]
pub enum ResolvedHealthSpec {
    ProcessOnly,
    Tcp { port: u16, timeout_ms: u64 },
    Http {
        url: String,
        expected_status: u16,
        timeout_ms: u64,
    },
    Custom { checker: CustomHealthChecker },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallSafetyReport {
    pub safe: bool,
    pub blockers: Vec<UninstallBlocker>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallBlocker {
    pub kind: String,
    pub message_i18n: String,
    #[serde(default)]
    pub dependents: Vec<JreDependent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JreUsageReport {
    pub in_use: bool,
    pub is_default: bool,
    pub dependents: Vec<JreDependent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JreDependent {
    pub kind: String,
    pub id: String,
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallParams {
    pub key: String,
    pub version: String,
    pub mirror_index: usize,
    pub set_as_default_jre: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomInstallParams {
    pub name: String,
    pub archive_path: String,
}

// 保留原设计中的类型（兼容性）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareMeta {
    pub key: String,
    pub name: String,
    pub description: String,
    pub available_versions: Vec<String>,
    pub default_version: String,
}
