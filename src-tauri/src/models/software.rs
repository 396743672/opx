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

#[cfg(test)]
mod tests {
    use super::{
        BuiltinInfo, ConfigField, ConfigFieldType, ConfigSchema, CustomHealthSpec,
        CustomStartCommand, HealthCheckSpec, InstallSource, InstalledSoftware, JreDependent,
        JreUsageReport, MirrorSource, SoftwareStatus, UninstallBlocker, UninstallSafetyReport,
    };

    #[test]
    fn mirror_source_with_builtin_serializes_correctly() {
        let mirror = MirrorSource {
            name: "内置默认版本（离线）".to_string(),
            url: "builtin://software/jre/17.0.15.zip".to_string(),
            builtin: Some(BuiltinInfo {
                version: "17.0.15".to_string(),
                sha256: "abc123".to_string(),
                size: 43478873,
            }),
        };
        let json = serde_json::to_string(&mirror).unwrap();
        assert!(json.contains("\"builtin\""));
        assert!(json.contains("\"sha256\":\"abc123\""));

        let deserialized: MirrorSource = serde_json::from_str(&json).unwrap();
        assert!(deserialized.builtin.is_some());
        assert_eq!(deserialized.builtin.unwrap().sha256, "abc123");
    }

    #[test]
    fn mirror_source_without_builtin_omits_field() {
        let mirror = MirrorSource {
            name: "Adoptium(清华)".to_string(),
            url: "https://github.com/...".to_string(),
            builtin: None,
        };
        let json = serde_json::to_string(&mirror).unwrap();
        // None 字段应被 skip_serializing_if 跳过，不出现在 JSON 中
        assert!(!json.contains("builtin"));
    }

    #[test]
    fn install_source_builtin_serializes_correctly() {
        let source = InstallSource::Builtin {
            version: "17.0.15".to_string(),
        };
        let json = serde_json::to_string(&source).unwrap();
        assert!(json.contains("\"Builtin\""));
        assert!(json.contains("\"version\":\"17.0.15\""));

        let deserialized: InstallSource = serde_json::from_str(&json).unwrap();
        match deserialized {
            InstallSource::Builtin { version } => assert_eq!(version, "17.0.15"),
            _ => panic!("应反序列化为 Builtin 变体"),
        }
    }

    #[test]
    fn software_status_has_starting_variant() {
        let s = SoftwareStatus::Starting;
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"Starting\"");
    }

    #[test]
    fn software_status_has_stopping_variant() {
        let s = SoftwareStatus::Stopping;
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"Stopping\"");
    }

    #[test]
    fn software_status_has_initializing_variant() {
        let s = SoftwareStatus::Initializing;
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"Initializing\"");
    }

    fn make_installed() -> InstalledSoftware {
        InstalledSoftware {
            id: "uuid".to_string(),
            key: "mysql".to_string(),
            version: "8.4.10".to_string(),
            name: "MySQL".to_string(),
            install_path: "apps/mysql/8.4.10".to_string(),
            install_time: chrono::DateTime::from_timestamp(1700000000, 0)
                .unwrap()
                .naive_utc(),
            status: SoftwareStatus::Stopped,
            port: 3306,
            config: serde_json::json!({}),
            is_custom: false,
            auto_start_on_app_start: false,
            startup_order: 0,
            source: InstallSource::Mirror {
                mirror_name: "test".to_string(),
                url: "http://test".to_string(),
            },
            pid: None,
            last_started_at: None,
            last_stopped_at: None,
            last_error: None,
            custom_start_command: None,
        }
    }

    #[test]
    fn installed_software_has_runtime_fields() {
        let s = make_installed();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"pid\":null"));
        assert!(json.contains("\"last_started_at\":null"));
        assert!(json.contains("\"custom_start_command\":null"));
    }

    #[test]
    fn custom_start_command_serializes() {
        let cmd = CustomStartCommand {
            executable: "bin/app.exe".to_string(),
            args: vec!["--port=8080".to_string()],
            working_dir: None,
            env_vars: {
                let mut m = std::collections::BTreeMap::new();
                m.insert("NODE_ENV".to_string(), "production".to_string());
                m
            },
            health_check: CustomHealthSpec::Tcp { port: 8080 },
            config_file_relative: None,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"executable\":\"bin/app.exe\""));
        assert!(json.contains("\"Tcp\""));
        let de: CustomStartCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(de.executable, "bin/app.exe");
    }

    #[test]
    fn custom_health_spec_http_serializes() {
        let spec = CustomHealthSpec::Http {
            url: "http://127.0.0.1:8080/health".to_string(),
            expected_status: 200,
        };
        let json = serde_json::to_string(&spec).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"Http","spec":{"url":"http://127.0.0.1:8080/health","expected_status":200}}"#
        );
    }

    #[test]
    fn health_check_spec_tcp_serializes() {
        let spec = HealthCheckSpec::Tcp {
            port: 3306,
            timeout_ms: 1000,
        };
        let json = serde_json::to_string(&spec).unwrap();
        assert_eq!(json, r#"{"kind":"Tcp","spec":{"port":3306,"timeout_ms":1000}}"#);
    }

    #[test]
    fn config_schema_round_trip() {
        let schema = ConfigSchema {
            fields: vec![ConfigField {
                key: "port".to_string(),
                label_i18n: "configField.port".to_string(),
                field_type: ConfigFieldType::Port,
                default_value: serde_json::json!(3306),
                section: Some("[mysqld]".to_string()),
                description_i18n: None,
            }],
            ephemeral_keys: vec![],
        };
        let json = serde_json::to_string(&schema).unwrap();
        let de: ConfigSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(de.fields.len(), 1);
        assert_eq!(de.fields[0].key, "port");
    }

    #[test]
    fn config_field_type_select_serializes_flat() {
        let ft = ConfigFieldType::Select {
            options: vec!["utf8mb4".to_string(), "utf8".to_string()],
        };
        let json = serde_json::to_string(&ft).unwrap();
        // 改用 tag="type"（无 content）后，options 字段直接平铺，避免嵌套 options.options
        assert_eq!(
            json,
            r#"{"type":"Select","options":["utf8mb4","utf8"]}"#
        );
    }

    #[test]
    fn uninstall_safety_report_serializes() {
        let report = UninstallSafetyReport {
            safe: false,
            blockers: vec![UninstallBlocker {
                kind: "running".to_string(),
                message_i18n: "uninstallBlockedRunning".to_string(),
                dependents: vec![],
            }],
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"safe\":false"));
        assert!(json.contains("\"running\""));
    }

    #[test]
    fn jre_usage_report_serializes() {
        let report = JreUsageReport {
            in_use: true,
            is_default: true,
            dependents: vec![JreDependent {
                kind: "springboot-app".to_string(),
                id: "app-1".to_string(),
                name: "my-api".to_string(),
                status: "running".to_string(),
            }],
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"in_use\":true"));
        assert!(json.contains("\"is_default\":true"));
        assert!(json.contains("\"springboot-app\""));
    }

    #[test]
    fn installed_software_deserializes_old_format_without_runtime_fields() {
        // 旧 installed.json 没有新字段，应能反序列化（向后兼容）
        let old = r#"{
            "id":"uuid","key":"mysql","version":"8.4.10","name":"MySQL",
            "install_path":"apps/mysql/8.4.10","install_time":"2024-01-01T00:00:00",
            "status":"Stopped","port":3306,"config":{},"is_custom":false,
            "auto_start_on_app_start":false,"startup_order":0,
            "source":{"Mirror":{"mirror_name":"t","url":"http://t"}}
        }"#;
        let de: InstalledSoftware = serde_json::from_str(old).unwrap();
        assert_eq!(de.pid, None);
        assert_eq!(de.custom_start_command, None);
        assert_eq!(de.last_error, None);
        assert_eq!(de.last_started_at, None);
        assert_eq!(de.last_stopped_at, None);
    }
}
