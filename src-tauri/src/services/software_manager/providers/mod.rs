use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::models::software::{CatalogEntry, CatalogVersion, ConfigSchema, CustomStartCommand, HealthCheckSpec};

pub mod mysql;
pub mod jre;
pub mod jdk;
pub mod redis;
pub mod nginx;
pub mod minio;
pub mod rustfs;
pub mod custom_templates;

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

    /// 停止命令（None 表示用通用 kill 流程）
    fn stop_command(&self, _ctx: &StopContext) -> Result<Option<StopCommand>> {
        Ok(None)
    }

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
}

/// 停止上下文
pub struct StopContext {
    pub installed_id: String,
    pub install_path: String,
    pub pid: u32,
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

/// 启动命令（provider 返回，由 lifecycle 执行 spawn）
pub struct StartCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env_vars: std::collections::BTreeMap<String, String>,
    pub working_dir: PathBuf,
    pub creation_flags: u32,
    pub first_run_init: Option<Box<FirstRunInit>>,
}

/// 首次启动前执行的初始化命令（如 mysqld --initialize-insecure）
pub struct FirstRunInit {
    pub init_command: StartCommand,
    pub temp_secret_output: Option<TempSecretSpec>,
}

/// 临时密码提取方式
pub enum TempSecretSpec {
    FromStdoutRegex(String),
    FromLogFile { path: PathBuf, regex: String },
}

/// 停止命令（provider 可选返回；None 表示走通用 kill 流程）
pub struct StopCommand {
    pub program: String,
    pub args: Vec<String>,
    pub working_dir: PathBuf,
    pub env_vars: std::collections::BTreeMap<String, String>,
    pub creation_flags: u32,
    pub wait_timeout_secs: u64,
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
    ]
}
