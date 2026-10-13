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
pub mod consul;
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

/// 按当前 OS 拼装可执行文件名（P1-1 跨平台）。
/// Windows 上追加 `.exe` 后缀；Unix/macOS 上返回原名（扩展名由包管理约定，无 `.exe`）。
/// 用此助手替代硬编码的 `xxx.exe`，避免 Unix 上因 `.exe` 启动失败。
pub(crate) fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{}.exe", base)
    } else {
        base.to_string()
    }
}

/// 在 `dir` 下按顺序执行构建命令（P2-1）。
///
/// 供**上游无预编译二进制、只能源码构建**的软件（redis / nginx）在 *nix 上安装后编译用。
/// 每条命令为 `(program, args)`；任一步失败即返回带命令名 + stderr 尾部的明确错误，
/// 缺工具链（make/gcc 未安装）时提示安装构建工具。
/// ⚠️ 该路径依赖目标机工具链，且无法在 Windows CI 验证 —— 待真实 Linux/macOS 实机验证。
pub(crate) fn run_build_steps(
    dir: &std::path::Path,
    what: &str,
    steps: &[(&str, &[&str])],
) -> anyhow::Result<()> {
    for (program, args) in steps {
        let out = std::process::Command::new(program)
            .args(*args)
            .current_dir(dir)
            .output()
            .map_err(|e| {
                anyhow::anyhow!(
                    "{what} 源码构建失败：无法运行 `{program}`（{e}）。请先安装构建工具链（gcc / make）。"
                )
            })?;
        if !out.status.success() {
            let tail = String::from_utf8_lossy(&out.stderr);
            let tail = tail.lines().rev().take(8).collect::<Vec<_>>();
            let tail = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
            return Err(anyhow::anyhow!(
                "{what} 源码构建失败：`{program}` 退出码 {:?}\n{}",
                out.status.code(),
                tail
            ));
        }
    }
    Ok(())
}

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

    /// 语义化优雅停止命令（可选；默认 None 表示无，回退强杀）。
    ///
    /// P1-3：避免对数据库/中间件直接 `taskkill /F` 强杀导致数据损坏（MySQL/PostgreSQL/
    /// MongoDB/Redis 被强杀有未落盘/日志截断风险）。实现者返回各自的关闭命令
    /// （如 redis-cli shutdown / nginx -s quit / pg_ctl stop / mongod --shutdown）。
    /// 命令层在强杀前先执行它，等待进程自行退出；失败/超时再回退强杀。
    /// 需要认证才能关闭的软件（如 MySQL root 密码为一次性 ephemeral、未持久化）应返回
    /// None，交由命令层强杀——否则会陷入「认证失败 → 强杀」的假优雅。
    fn graceful_stop_command(&self, _ctx: &StopContext) -> Option<GracefulStopCommand> {
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

/// 优雅停止命令（provider 返回，由 lifecycle 执行；失败/超时回退强杀）。
/// 程序路径用 install_path 下的绝对路径，避免依赖 PATH。
#[derive(Debug, Clone)]
pub struct GracefulStopCommand {
    pub program: String,
    pub args: Vec<String>,
    pub working_dir: PathBuf,
    /// 附加环境变量。用于需要认证的关闭命令——如 MySQL 经 `MYSQL_PWD` 传密码，
    /// 避免密码出现在命令行参数中被其他用户从进程列表读到。
    pub env_vars: std::collections::BTreeMap<String, String>,
    /// 等待进程自行退出的最长秒数；超时则回退强杀。
    pub timeout_secs: u64,
}

/// 优雅停止上下文（传给 provider.graceful_stop_command）
pub struct StopContext {
    pub installed_id: String,
    pub install_path: String, // 已 resolve 绝对路径
    pub version: String,
    pub config: serde_json::Value,
    pub port: u16,
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
    /// spawn 前从继承环境中移除的变量名。
    ///
    /// 用途：宿主进程注入的环境变量可能改变子软件行为。典型：`SERVER_PORT` /
    /// `SERVER__PORT` 会被 Spring Boot 宽松绑定解析为 `server.port`，覆盖被管软件
    /// （Nacos 等）自带的端口配置，导致绑到宿主端口启动失败。按需声明，勿滥用。
    pub remove_envs: Vec<String>,
    pub working_dir: PathBuf,
    pub creation_flags: u32,
    pub first_run_init: Option<Box<FirstRunInit>>,
}

/// 首次启动前执行的初始化命令（如 mysqld --initialize-insecure）
#[derive(Debug)]
pub struct FirstRunInit {
    pub init_command: StartCommand,
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
        Box::new(consul::ConsulProvider::new()),
        Box::new(nacos::NacosProvider::new()),
        Box::new(kafka::KafkaProvider::new()),
        Box::new(elasticsearch::ElasticsearchProvider::new()),
        Box::new(influxdb::InfluxdbProvider::new()),
        Box::new(influxdb3::Influxdb3Provider::new()),
        Box::new(node::NodeProvider::new()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P1-3 回归：已实现的语义化优雅停止命令形状正确（二进制路径 + 关键参数）。
    /// 其余 provider 返回 None（走强杀回退），不在本次断言范围。
    #[test]
    fn graceful_stop_commands_are_well_formed() {
        let ctx = StopContext {
            installed_id: "t1".to_string(),
            install_path: "C:/opx/apps/redis/1.0".to_string(),
            version: "1.0".to_string(),
            config: serde_json::json!({}),
            port: 0,
        };
        for p in all_providers() {
            let key = p.key().to_string();
            let Some(cmd) = p.graceful_stop_command(&ctx) else {
                continue;
            };
            match key.as_str() {
                "redis" => {
                    assert!(cmd.program.ends_with(&exe_name("redis-cli")), "redis 停止程序应为 redis-cli");
                    assert!(cmd.args.contains(&"shutdown".to_string()), "redis 停止应带 shutdown");
                }
                "nginx" => {
                    assert!(cmd.program.ends_with(&exe_name("nginx")), "nginx 停止程序应为 nginx");
                    assert_eq!(cmd.args, vec!["-s".to_string(), "quit".to_string()]);
                }
                "postgresql" => {
                    assert!(cmd.program.ends_with(&exe_name("pg_ctl")), "pg 停止程序应为 pg_ctl");
                    assert!(cmd.args.contains(&"-D".to_string()), "pg 停止应带 -D <data>");
                    assert!(cmd.args.contains(&"fast".to_string()), "pg 停止模式应为 fast");
                }
                "mongodb" => {
                    assert!(cmd.program.ends_with(&exe_name("mongod")), "mongo 停止程序应为 mongod");
                    assert!(cmd.args.contains(&"--shutdown".to_string()), "mongo 停止应带 --shutdown");
                    assert!(cmd.args.contains(&"--dbpath".to_string()), "mongo 停止应带 --dbpath");
                }
                other => panic!("未预期的 provider 实现了 graceful_stop_command: {other}"),
            }
        }
    }

    /// requirepass 非空时 redis 停止命令应带 `-a` 认证参数
    #[test]
    fn redis_graceful_stop_includes_auth_when_password_set() {
        let ctx = StopContext {
            installed_id: "t1".to_string(),
            install_path: "C:/opx/apps/redis/1.0".to_string(),
            version: "1.0".to_string(),
            config: serde_json::json!({ "requirepass": "s3cret" }),
            port: 0,
        };
        let p = redis::RedisProvider::new();
        let cmd = p.graceful_stop_command(&ctx).expect("redis 应有优雅停止命令");
        let idx = cmd.args.iter().position(|a| a == "-a").expect("应带 -a");
        assert_eq!(cmd.args[idx + 1], "s3cret");
        assert_eq!(cmd.args[0], "-p");
    }

    /// P1-3 补完：MySQL 密码落盘后应产出 `mysqladmin shutdown` 优雅停止，
    /// 且密码经 `MYSQL_PWD` 环境变量传递（**绝不**出现在命令行参数中）。
    #[test]
    fn mysql_graceful_stop_uses_mysqladmin_with_mysql_pwd() {
        let ctx = StopContext {
            installed_id: "t1".to_string(),
            install_path: "C:/opx/apps/mysql/8.4.11".to_string(),
            version: "8.4.11".to_string(),
            config: serde_json::json!({ "init_password": "s3cret", "port": 3307 }),
            port: 0,
        };
        let p = mysql::MySqlProvider::new();
        let cmd = p
            .graceful_stop_command(&ctx)
            .expect("有密码时应产出优雅停止命令");
        assert!(cmd.program.ends_with(&exe_name("mysqladmin")), "{}", cmd.program);
        assert!(cmd.args.contains(&"shutdown".to_string()), "应带 shutdown");
        assert!(cmd.args.contains(&"3307".to_string()), "应使用配置端口，而非默认 3306");
        assert_eq!(
            cmd.env_vars.get("MYSQL_PWD").map(|s| s.as_str()),
            Some("s3cret"),
            "密码应经 MYSQL_PWD 传递"
        );
        assert!(
            !cmd.args.iter().any(|a| a.contains("s3cret")),
            "密码绝不能出现在命令行"
        );
    }

    /// 无密码（老实例：密码未落盘）→ `None`，走既有强杀回退，**无回归**。
    #[test]
    fn mysql_graceful_stop_is_none_without_password() {
        let ctx = StopContext {
            installed_id: "t1".to_string(),
            install_path: "C:/opx/apps/mysql/8.4.11".to_string(),
            version: "8.4.11".to_string(),
            config: serde_json::json!({}),
            port: 0,
        };
        let p = mysql::MySqlProvider::new();
        assert!(
            p.graceful_stop_command(&ctx).is_none(),
            "无密码时应返回 None（回退强杀）"
        );
    }
}
