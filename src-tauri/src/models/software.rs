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
    Registry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub key: String,
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_i18n: Option<String>,
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

// ===== C 扩展（日志查看器 + 备份/恢复）新增类型 =====

/// 日志来源种类
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LogSourceKind {
    /// 进程 stdout/stderr 重定向落盘（spawn_process 写入 <install_path>/logs/opx-<installed_id>.log）
    StdoutRedirect,
    /// provider 自带日志文件（如 MongoDB 的 data/mongod.log）
    ProviderFile,
}

/// 单条日志来源（序列化给前端展示与选择）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogSource {
    /// 日志文件绝对路径
    pub path: String,
    pub kind: LogSourceKind,
    /// 是否结构化、可显示级别筛选（决策 6：仅结构化级别日志显示筛选）
    pub has_levels: bool,
    /// provider 提供的级别提取正则；None 时用 LogService 内置默认正则
    pub level_pattern: Option<String>,
}

/// 读取日志返回的分块（前端轮询/分页消费）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogChunk {
    /// 命中的日志行（已应用关键字/正则/级别过滤）
    pub lines: Vec<String>,
    /// 本块首行的字节偏移
    pub start_offset: u64,
    /// 本块末行之后的字节偏移（前端下次轮询/分页携带）
    pub end_offset: u64,
    /// 文件总字节数
    pub total_bytes: u64,
    /// 向前是否还有更早的历史（用于「加载更多历史」）
    pub has_more: bool,
    /// 因超过单次上限被截断（命中行多于 limit）
    pub truncated: bool,
}

/// 备份快照元信息（持久化于 <app_data>/backups/<id>/manifest.json）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SnapshotMeta {
    /// = 快照文件名去后缀（如 20260817_143000）
    pub id: String,
    /// RFC3339 创建时间
    pub created_at: String,
    /// 来源软件 key（如 "mysql"）
    pub source_key: String,
    /// 来源软件版本（如 "8.4.11"）
    pub source_version: String,
    /// 首数字段大版本；MinIO 等以非数字开头的版本无法解析时为 None
    pub major_version: Option<u32>,
    /// zip 文件字节大小
    pub size_bytes: u64,
    /// 快照格式（"zip"）
    pub format: String,
    /// 用户自定义名称（P1-B1）
    pub name: Option<String>,
    /// 用户自定义备注（P1-B1）
    pub note: Option<String>,
}

/// 备份模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BackupMode {
    /// 默认：先停服再备份（决策 2）
    StopAndBackup,
    /// 热备（提供警告）
    Hot,
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
    /// options 是选项值（存 config），labels 是对应显示文本（长度与 options 一致）。
    /// 用于「值存稳定标识（如 installed_id）、显示友好名称」的场景（如 JDK 选择）。
    /// labels 为空时前端直接显示 options 值，向后兼容。
    Select {
        options: Vec<String>,
        #[serde(default)]
        labels: Vec<String>,
    },
    /// 数值 + 单位下拉：值形如 "256mb"/"512M"，数字可填、单位只能从 units 里选（防手写单位出错）
    Size { units: Vec<String> },
    Boolean,
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

