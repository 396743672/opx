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
    /// 对象存储（MinIO / RustFS 等，治理自 Database 迁移而来）
    Storage,
    /// 消息队列（Kafka 等）
    MessageQueue,
    /// 搜索（Elasticsearch 等）
    Search,
    /// 时序数据库（InfluxDB 等）
    TimeSeries,
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

    /// 软件图标（来自 catalog，软件专属；自定义软件为空占位）
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,

    /// 软件分类（来自 catalog，用于栈候选等按类型过滤）；自定义软件为 None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<SoftwareCategory>,

    /// 依赖的其他已装软件 id（启动时按拓扑序自动拉起，须处于运行态）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
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

/// 历史归档日志描述（同目录/日期目录下滚动压缩的旧日志，时间倒序）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArchiveLog {
    /// 归档文件绝对路径
    pub path: String,
    /// 展示标签（如 "2026-08-24" / "info.2026-08-26.0.log.gz"）
    pub label: String,
}

/// 全局日志搜索命中（search_all_logs 返回）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogHit {
    pub installed_id: String,
    pub software_name: String,
    pub source_label: String,
    pub file: String,
    pub line: String,
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
    /// 展示名（如 nginx 的「访问日志」「错误日志」）；None 时前端回退通用标签
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 历史归档（时间倒序，最新在前）。空 = 无归档。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub archives: Vec<ArchiveLog>,
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
    /// 当前实际读取的历史归档索引（0=主文件）。前端下次请求携带它实现无缝续接。
    #[serde(default)]
    pub archive_index: usize,
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

/// 软件升级检测结果：target_version 为比当前版本高的最高可用版本
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeInfo {
    pub key: String,
    pub name: String,
    pub current_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_version: Option<String>,
    /// 同 key 存在 <ver>.bak 备份时填该旧版本（回滚目标）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_to: Option<String>,
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
    /// 字段显示/必填规则（如「auth_enabled=true 时才显示 admin_token 且必填」）。
    /// 由 ConfigFormTab 渲染时应用。
    #[serde(default)]
    pub field_rules: Vec<FieldRule>,
}

/// 字段规则：当 visible_when 满足时显示，且 required 时必填。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldRule {
    pub field_key: String,
    /// 条件显示：None 表示始终显示。
    #[serde(default)]
    pub visible_when: Option<FieldCondition>,
    /// 满足显示条件时是否必填（+ 非空校验）。
    #[serde(default)]
    pub required: bool,
}

/// 条件：当 config 的 key 字段值等于 equals 时满足。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldCondition {
    pub key: String,
    pub equals: serde_json::Value,
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
    /// 多行文本（如集群节点列表，一行一项）
    Textarea,
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

