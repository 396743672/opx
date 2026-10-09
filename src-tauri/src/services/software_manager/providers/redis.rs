use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, LogSource, MirrorSource, SoftwareCategory,
};

use super::{
    ConfigContext, GracefulStopCommand, HealthContext, InstallContext, LogContext, SoftwareProvider,
    StartCommand, StartContext, StopContext, default_log_sources, exe_name,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

/// Redis 发行包 URL 与格式（P2-1，per-OS）：
/// - Windows：`redis-windows` 项目的 cygwin 预编译 zip；
/// - Linux/macOS：官方**源码** tar.gz（上游无预编译二进制，安装后由 post_install 编译出 `src/redis-server`）。
fn redis_archive(version: &str, os: &str) -> (String, ArchiveFormat) {
    if os == "windows" {
        (
            format!("https://github.com/redis-windows/redis-windows/releases/download/{version}/Redis-{version}-Windows-x64-cygwin.zip"),
            ArchiveFormat::Zip,
        )
    } else {
        (
            format!("https://download.redis.io/releases/redis-{version}.tar.gz"),
            ArchiveFormat::TarGz,
        )
    }
}

pub struct RedisProvider;

impl RedisProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RedisProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for RedisProvider {
    fn key(&self) -> &str {
        "redis"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        // P2-1：运行时按 OS 选包（Windows 预编译 zip / *nix 官方源码 tar.gz，装后编译）。
        let os = crate::utils::platform::current_os();
        let mirror_name = if os == "windows" { "i18n:redisWindowsGithub" } else { "i18n:official" };
        let versions = ["8.8.0", "8.2.7", "7.4.9"]
            .iter()
            .map(|v| {
                let (url, format) = redis_archive(v, os);
                CatalogVersion {
                    version: v.to_string(),
                    mirrors: vec![MirrorSource {
                        name: mirror_name.to_string(),
                        url,
                        builtin: None,
                    }],
                    archive: ArchiveInfo { format, size: None, sha256: None },
                }
            })
            .collect();

        CatalogEntry {
            key: "redis".to_string(),
            name: "Redis".to_string(),
            description: "内存键值存储".to_string(),
            description_i18n: Some("catalogDesc.redis".to_string()),
            category: SoftwareCategory::Cache,
            icon: "mdi:memory".to_string(),
            versions,
            default_version: "7.4.9".to_string(),
        }
    }

    fn post_install(&self, ctx: &InstallContext) -> Result<()> {
        // 修复 zip 自带的 redis.conf 中 bare `requirepass`（无值）语法错误。
        // 保留原始配置文件的所有注释和设置，仅修复问题行。
        let conf_path = ctx.install_dir().join("redis.conf");
        if conf_path.exists() {
            let raw = std::fs::read_to_string(&conf_path)?;
            // 将 bare `requirepass`（无值或空值）替换为注释
            let fixed = raw
                .lines()
                .map(|line| {
                    let trimmed = line.trim();
                    if trimmed.starts_with("requirepass")
                        && trimmed["requirepass".len()..].trim().is_empty()
                    {
                        // 将 bare requirepass 注释掉，避免语法错误
                        if line.trim_start().starts_with('#') {
                            line.to_string()
                        } else {
                            format!("#{}", line)
                        }
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            std::fs::write(&conf_path, fixed.as_bytes())?;
        }

        // P2-1：非 Windows 走官方源码包 → 在此编译出 `src/redis-server`（上游无预编译二进制）。
        // 依赖目标机 gcc/make，缺失时 run_build_steps 给出明确错误。
        if crate::utils::platform::current_os() != "windows" {
            let steps: &[(&str, &[&str])] = &[("make", &[])];
            super::run_build_steps(ctx.install_dir(), "Redis", steps)?;
        }
        Ok(())
    }

    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        // redis-windows GitHub Releases API（社区维护的 Windows Redis 移植）
        let url = "https://api.github.com/repos/redis-windows/redis-windows/releases?per_page=20";
        let response = reqwest::blocking::Client::builder()
            // 关掉环境变量代理探测（ALL_PROXY 等）：reqwest 默认会读，
            // 宿主若设了不支持 CONNECT 的 HTTP 代理，公网直连被劫持后必失败
            .no_proxy()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .ok()?
            .get(url)
            .header("User-Agent", "OPX")
            .header("Accept", "application/vnd.github+json")
            .send()
            .ok()?;

        if !response.status().is_success() {
            eprintln!("[redis] GitHub API 返回 {}", response.status());
            return None;
        }

        let releases: Vec<serde_json::Value> = response.json().ok()?;
        let mut versions = vec![];

        // 只取第一个（Latest，API 默认按创建时间倒序）
        if let Some(release) = releases.first() {
            let tag = match release.get("tag_name").and_then(|t| t.as_str()) {
                Some(t) => t.to_string(),
                None => return None,
            };
            // tag 直接是版本号如 "8.8.0" / "7.4.9"
            // 找 cygwin.zip asset（与现有硬编码一致）
            if let Some(asset_url) = find_cygwin_asset(release) {
                versions.push(CatalogVersion {
                    version: tag,
                    mirrors: vec![MirrorSource {
                        name: "i18n:redisWindowsGithub".to_string(),
                        url: asset_url,
                        builtin: None,
                    }],
                    archive: ArchiveInfo {
                        format: ArchiveFormat::Zip,
                        size: None,
                        sha256: None,
                    },
                });
            }
        }

        if versions.is_empty() {
            None
        } else {
            Some(versions)
        }
    }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        // 使用配置文件启动 redis-server（配置文件包含完整设置，CLI 参数覆盖面有限）。
        // Cygwin 编译的 redis-server.exe 不认 Windows 绝对路径（D:/path 会被 POSIX
        // 映射打乱为 /7.4.9/D:/path），但认相对路径 —— working_dir 已设为 install_path，
        // 直接传 "redis.conf" 即可。
        Ok(StartCommand {
            // P2-1：*nix 上源码编译产物在 <install>/src/redis-server；Windows 为包内 redis-server.exe。
            program: if crate::utils::platform::current_os() == "windows" {
                exe_name("redis-server")
            } else {
                PathBuf::from(&ctx.install_path)
                    .join("src")
                    .join("redis-server")
                    .to_string_lossy()
                    .to_string()
            },
            args: vec!["redis.conf".to_string()],
            env_vars: std::collections::BTreeMap::new(),
            working_dir: PathBuf::from(&ctx.install_path),
            remove_envs: Vec::new(),
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: None,
        })
    }

    fn health_check(&self, ctx: &HealthContext) -> HealthCheckSpec {
        let port = ctx
            .config
            .get("port")
            .and_then(|v| v.as_u64())
            .map(|p| p as u16)
            .unwrap_or(if ctx.port > 0 { ctx.port } else { 6379 });
        HealthCheckSpec::Tcp {
            port,
            timeout_ms: 1000,
        }
    }

    fn config_schema(&self) -> Option<ConfigSchema> {
        Some(ConfigSchema {
            fields: vec![
                ConfigField {
                    key: "port".to_string(),
                    label_i18n: "configField.port".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(6379),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "bind".to_string(),
                    label_i18n: "configField.bind".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("127.0.0.1"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "maxmemory".to_string(),
                    label_i18n: "configField.maxmemory".to_string(),
                    field_type: ConfigFieldType::Size {
                        units: vec!["mb".to_string(), "gb".to_string()],
                    },
                    default_value: serde_json::json!("256mb"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "maxmemory-policy".to_string(),
                    label_i18n: "configField.maxmemoryPolicy".to_string(),
                    field_type: ConfigFieldType::Select {
                        options: vec![
                            "allkeys-lru".to_string(),
                            "volatile-lru".to_string(),
                            "noeviction".to_string(),
                        ],
                        labels: vec![],
                    },
                    default_value: serde_json::json!("noeviction"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "requirepass".to_string(),
                    label_i18n: "configField.requirepass".to_string(),
                    field_type: ConfigFieldType::Password,
                    default_value: serde_json::json!(""),
                    section: None,
                    description_i18n: None,
                },
            ],
            ephemeral_keys: vec![],
            field_rules: vec![],
        })
    }

    fn log_sources(&self, ctx: &LogContext) -> Vec<LogSource> {
        // C 扩展：stdout 为结构化级别日志，启用级别筛选下拉
        let mut sources = default_log_sources(ctx);
        for s in &mut sources {
            s.has_levels = true;
        }
        sources
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> {
        // 仅返回相对文件名，调用方（write_config_form / read_config_source）
        // 会自行拼接 install_path，避免双路径拼接 bug。
        Some(PathBuf::from("redis.conf"))
    }

    /// P1-3：语义化优雅停止。`redis-cli shutdown` 干净关闭（关闭连接、按配置刷 AOF/RDB），
    /// 避免强杀导致未落盘数据丢失。requirepass 非空时带 `-a` 认证。
    fn graceful_stop_command(&self, ctx: &StopContext) -> Option<GracefulStopCommand> {
        let port = ctx
            .config
            .get("port")
            .and_then(|v| v.as_u64())
            .map(|p| p as u16)
            .unwrap_or(if ctx.port > 0 { ctx.port } else { 6379 });
        let mut args = vec!["-p".to_string(), port.to_string(), "shutdown".to_string()];
        if let Some(pw) = ctx.config.get("requirepass").and_then(|v| v.as_str()) {
            if !pw.is_empty() {
                args.insert(1, "-a".to_string());
                args.insert(2, pw.to_string());
            }
        }
        Some(GracefulStopCommand {
            program: PathBuf::from(&ctx.install_path)
                .join(exe_name("redis-cli").as_str())
                .to_string_lossy()
                .to_string(),
            args,
            working_dir: PathBuf::from(&ctx.install_path),
            timeout_secs: 10,
        })
    }
}

/// 在 release 的 assets 中找 cygwin.zip（不含 with-Service）
fn find_cygwin_asset(release: &serde_json::Value) -> Option<String> {
    let assets = release.get("assets")?.as_array()?;
    for asset in assets {
        let name = asset.get("name")?.as_str()?;
        // 名称如 "Redis-8.8.0-Windows-x64-cygwin.zip"
        if name.contains("cygwin")
            && !name.contains("with-Service")
            && name.ends_with(".zip")
        {
            let url = asset.get("browser_download_url")?.as_str()?;
            return Some(url.to_string());
        }
    }
    None
}
