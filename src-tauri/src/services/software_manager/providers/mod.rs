use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::models::software::{
    CatalogEntry, CatalogVersion, ConfigSchema, CustomStartCommand, HealthCheckSpec, LogSource,
    LogSourceKind,
};

pub mod mysql;
pub mod jre;
pub mod jdk;
pub mod redis;
pub mod nginx;
pub mod minio;
pub mod rustfs;
pub mod postgresql;
pub mod mongodb;
pub mod nacos;
pub mod kafka;
pub mod elasticsearch;
pub mod influxdb;
pub mod influxdb3;
pub mod node;
pub mod custom_templates;

/// 自持资源分发基址：体积较大的内置软件包固化在自有 GitHub Release（tag `res-v1`）上，
/// 避免依赖上游 URL（MySQL 官方 CDN 慢/易断、MinIO 已停发社区版二进制随时可能下架）。
///
/// URL 含 `github.com`，因此会被 `utils::download::resolve_url` 自动加上
/// `github_proxy_url`（默认 ghfast.top）前缀，国内无需额外配置即走加速。
pub const RESOURCE_BASE: &str = "https://github.com/396743672/opx/releases/download/res-v1";

pub trait SoftwareProvider: Send + Sync {
    fn key(&self) -> &str;
    fn catalog_entry(&self) -> CatalogEntry;
    fn post_install(&self, ctx: &InstallContext) -> Result<()>;

    /// 拉取远程版本列表（可选，默认返回 None 表示不动态拉取）
    /// 返回 Some(Vec) 时，版本会与内置 catalog_entry() 的版本合并
    /// 拉取失败应返回 None（不阻塞其他软件）
    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        None
    }

    /// 启动命令（含可执行文件路径、参数、env、工作目录、首次初始化钩子）
    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand>;

    /// 健康检查 spec（默认 ProcessOnly）
    fn health_check(&self, _ctx: &HealthContext) -> HealthCheckSpec {
        HealthCheckSpec::ProcessOnly
    }

    /// 表单 schema（None 表示无表单，如自定义软件）
    fn config_schema(&self) -> Option<ConfigSchema> {
        None
    }

    /// 配置文件路径（相对 install_path；None 表示无配置文件，如 MinIO/RustFS）
    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> {
        None
    }

    /// 启动时的工作目录（默认 install_path；MySQL 重写为子目录）
    fn working_dir(&self, ctx: &WorkingDirContext) -> PathBuf {
        PathBuf::from(&ctx.install_path)
    }

    // ===== C 扩展：日志来源 / 数据目录 / 级别正则（默认实现，新 provider 零改动即获得能力）=====

    /// 日志来源列表。默认仅 StdoutRedirect（spawn_process 落盘文件存在时返回）。
    /// provider 可覆盖以追加自带日志文件（如 MongoDB 的 data/mongod.log）。
    fn log_sources(&self, ctx: &LogContext) -> Vec<LogSource> {
        default_log_sources(ctx)
    }

    /// 需要备份/重置的数据目录。默认 [<install_path>/data]。
    /// 数据目录来自配置（非默认 <install_path>/data）的 provider 应覆盖此方法。
    fn data_dirs(&self, ctx: &DataDirContext) -> Vec<PathBuf> {
        default_data_dirs(ctx)
    }

    /// 结构化日志的级别提取正则；默认 None → LogService 用内置默认正则。
    /// 返回 Some(pattern) 时优先使用该正则做级别匹配。
    fn log_level_pattern(&self) -> Option<String> {
        None
    }

    /// 该软件所需的最低 JDK 主版本（仅 Java 中间件需要，如 Kafka=11、ES=17）。
    /// 默认 None 表示不需要 JDK（如 InfluxDB 等原生二进制）。
    /// 命令层 `fill_jdk_options` 据此过滤不兼容的已装 JDK/JRE。
    fn min_jdk_version(&self) -> Option<u32> {
        None
    }

    /// 服务健康后的一次性 HTTP 初始化（如 InfluxDB 2 onboarding：创建 admin/org/bucket/token）。
    /// 返回 Some(request) 时，命令层在健康检查通过后执行一次，成功后写 config.initialized=true。
    /// 默认 None 表示无需 post-start 初始化。
    fn post_start_http_init(&self, _ctx: &HealthContext) -> Option<PostStartHttpInit> {
        None
    }
}

/// post-start 初始化：HTTP 请求描述（服务已启动、健康检查通过后由命令层执行）。
#[derive(Debug, Clone)]
pub struct PostStartHttpInit {
    pub url: String,
    /// 幂等探测：GET 该 URL 判定是否已完成（如 InfluxDB /api/v2/setup 返回 {allowed:...}）。
    /// None 表示无需探测，直接执行。
    pub probe_url: Option<String>,
    /// 探测响应体包含此子串则认为已完成（跳过初始化）。
    pub probe_done_marker: String,
    /// 初始化请求体（POST）。
    pub body: serde_json::Value,
    /// 初始化成功后回写到 config 的字段（如 admin_token），供前端后续使用。
    pub config_fields: Vec<(String, serde_json::Value)>,
}

pub struct InstallContext {
    pub key: String,
    pub version: String,
    pub install_path: String,
}

impl InstallContext {
    pub fn new(key: String, version: String, install_path: String) -> Self {
        Self {
            key,
            version,
            install_path,
        }
    }

    pub fn install_dir(&self) -> &Path {
        Path::new(&self.install_path)
    }
}

/// 启动上下文（传给 provider.start_command）
pub struct StartContext {
    pub installed_id: String,
    pub install_path: String,
    pub version: String,
    pub config: serde_json::Value,
    pub custom_start_command: Option<CustomStartCommand>,
    /// 首次初始化密码（如 MySQL 初始化 root 密码）。来自 start_software 命令的可选参数，
    /// 仅在未初始化时由 provider 消费一次，绝不持久化到 installed.json / 配置文件。
    pub init_password: Option<String>,
    /// 已安装 JDK 的 install_path（如 Nacos 等 Java 软件启动用）。None 表示无 JDK 或软件不需要。
    pub jdk_install_path: Option<String>,
    /// 已安装 MySQL 的 install_path（Nacos 选 MySQL 数据库模式时，用其 mysql.exe 建库建表）。
    /// None 表示无 MySQL 或软件不需要。
    pub mysql_install_path: Option<String>,
}

/// 健康检查上下文
pub struct HealthContext {
    pub installed_id: String,
    pub install_path: String,
    pub port: u16,
    pub config: serde_json::Value,
}

/// 配置文件路径上下文
pub struct ConfigContext {
    pub install_path: String,
    pub version: String,
    pub config: serde_json::Value,
}

/// 工作目录上下文
pub struct WorkingDirContext {
    pub install_path: String,
    pub version: String,
}

// ===== C 扩展：日志来源 / 数据目录 上下文与默认实现 =====

/// 传给 `log_sources` 的上下文（由命令层从 InstalledSoftware 构造）
pub struct LogContext {
    pub installed_id: String,
    pub install_path: String, // 已 resolve 的绝对路径
    pub version: String,
    pub config: serde_json::Value,
    pub pid: Option<u32>,
}

/// 传给 `data_dirs` 的上下文（由命令层从 InstalledSoftware 构造）
pub struct DataDirContext {
    pub install_path: String,
    pub version: String,
    pub config: serde_json::Value,
}

/// 默认日志来源：仅 StdoutRedirect（基于 spawn_process 落盘的日志）。
/// 要求 <install_path>/logs/opx-<installed_id>.log 存在；不存在时返回空 vec（决策：仅展示运行实例）。
pub fn default_log_sources(ctx: &LogContext) -> Vec<LogSource> {
    let p = Path::new(&ctx.install_path)
        .join("logs")
        .join(format!("opx-{}.log", ctx.installed_id));
    if p.exists() {
        let src = LogSource {
            path: p.to_string_lossy().to_string(),
            kind: LogSourceKind::StdoutRedirect,
            has_levels: false, // 通用 stdout 默认无级别
            level_pattern: None,
            label: None,
            archives: vec![],
        };
        vec![crate::services::software_manager::log_viewer::attach_archives(src)]
    } else {
        vec![]
    }
}

/// 默认数据目录：<install_path>/data
pub fn default_data_dirs(ctx: &DataDirContext) -> Vec<PathBuf> {
    vec![Path::new(&ctx.install_path).join("data")]
}

/// 从 config 解析绝对 data 目录（与 provider.start_command 的解析逻辑保持一致）。
/// 绝对路径原样返回；相对路径按 install_path 拼接。
pub(crate) fn resolve_data_dir(config: &serde_json::Value, key: &str, default: &str, install_path: &str) -> PathBuf {
    let raw = config
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(default);
    if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        let clean = raw
            .strip_prefix("./")
            .or_else(|| raw.strip_prefix(".\\"))
            .unwrap_or(raw);
        Path::new(install_path).join(clean)
    }
}

/// 启动命令（provider 返回，由 lifecycle 执行 spawn）
#[derive(Debug)]
pub struct StartCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env_vars: std::collections::BTreeMap<String, String>,
    pub working_dir: PathBuf,
    pub creation_flags: u32,
    pub first_run_init: Option<Box<FirstRunInit>>,
}

/// 首次启动前执行的初始化命令（如 mysqld --initialize-insecure）
#[derive(Debug)]
pub struct FirstRunInit {
    pub init_command: StartCommand,
    pub temp_secret_output: Option<TempSecretSpec>,
}

/// 临时密码提取方式
#[derive(Debug)]
pub enum TempSecretSpec {
    FromStdoutRegex(String),
    FromLogFile { path: PathBuf, regex: String },
}

use std::collections::HashMap;
use std::sync::OnceLock;

/// 内置 zip 清单条目
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ManifestEntry {
    pub sha256: String,
    pub size: u64,
}

/// 内置 zip 清单：{key: {version: ManifestEntry}}
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct BuiltinManifest {
    #[serde(flatten)]
    entries: HashMap<String, HashMap<String, ManifestEntry>>,
}

impl BuiltinManifest {
    /// 从指定路径加载 manifest.json，文件不存在或解析失败返回空 manifest
    pub fn load_from_path(path: &std::path::Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Self::default(),
        };
        serde_json::from_str(&content).unwrap_or_default()
    }

    /// 查询 {key}/{version} 对应的 manifest 条目
    pub fn get_builtin(&self, key: &str, version: &str) -> Option<&ManifestEntry> {
        self.entries
            .get(key)
            .and_then(|versions| versions.get(version))
    }
}

static MANIFEST: OnceLock<BuiltinManifest> = OnceLock::new();

/// 获取全局 manifest 单例。首次调用时返回默认空 manifest（真正的初始化在 init_builtin_manifest）。
pub fn builtin_manifest() -> &'static BuiltinManifest {
    MANIFEST.get_or_init(|| BuiltinManifest::default())
}

/// 用指定 manifest 路径初始化全局单例（应用启动时调用）
pub fn init_builtin_manifest(path: &std::path::Path) {
    let manifest = BuiltinManifest::load_from_path(path);
    let _ = MANIFEST.set(manifest);
}

pub fn all_providers() -> Vec<Box<dyn SoftwareProvider>> {
    vec![
        Box::new(mysql::MySqlProvider::new()),
        Box::new(jre::JreProvider::new()),
        Box::new(jdk::JdkProvider::new()),
        Box::new(redis::RedisProvider::new()),
        Box::new(nginx::NginxProvider::new()),
        Box::new(minio::MinioProvider::new()),
        Box::new(rustfs::RustfsProvider::new()),
        Box::new(postgresql::PostgreSqlProvider::new()),
        Box::new(mongodb::MongoDbProvider::new()),
        Box::new(nacos::NacosProvider::new()),
        Box::new(kafka::KafkaProvider::new()),
        Box::new(elasticsearch::ElasticsearchProvider::new()),
        Box::new(influxdb::InfluxdbProvider::new()),
        Box::new(influxdb3::Influxdb3Provider::new()),
        Box::new(node::NodeProvider::new()),
    ]
}
