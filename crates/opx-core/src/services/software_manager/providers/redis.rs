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
        // 包自带的 redis.conf 是「只监听本机 + 保护模式开」的安全默认值，
        // 但 opx 的实际场景就是要对外提供服务：安装后按系统配置改写监听地址，
        // 并在「对外监听且没密码」时关掉保护模式（否则外部连接一律 DENIED）。
        let conf_path = ctx.install_dir().join("redis.conf");
        if conf_path.exists() {
            let raw = std::fs::read_to_string(&conf_path)?;
            let bind = crate::services::software_manager::netutils::default_service_bind();
            let fixed = rewrite_redis_conf(&raw, &bind);
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

    /// *nix 上 `post_install` 要跑 `make`（上游不提供预编译二进制）。
    fn post_install_builds(&self) -> bool {
        crate::utils::platform::current_os() != "windows"
    }

    /// 与 `post_install` 写进 redis.conf 的值保持一致（否则表单默认值会把配置改回去）。
    fn install_defaults(&self) -> serde_json::Value {
        let bind = crate::services::software_manager::netutils::default_service_bind();
        let protected = if bind_is_external(&bind) && !bind.is_empty() {
            "no"
        } else {
            "yes"
        };
        serde_json::json!({ "bind": bind, "protected-mode": protected })
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
                    field_type: ConfigFieldType::BindAddress,
                    default_value: serde_json::json!(
                        crate::services::software_manager::netutils::default_service_bind()
                    ),
                    section: None,
                    description_i18n: Some("configField.redisBindDesc".to_string()),
                },
                // protected mode 是 Redis 的安全网：**只认「默认用户有没有密码」**，
                // 与 bind 无关。无密码 + 对外监听时它会把所有外部连接 DENIED，
                // 表现为「端口通了但一操作就报 DENIED … protected mode」。
                // 用 Select(yes/no) 而非 Boolean：Redis 配置只认 yes/no，
                // 写成 `protected-mode false` 会直接启动失败（config 解析报错）。
                ConfigField {
                    key: "protected-mode".to_string(),
                    label_i18n: "configField.protectedMode".to_string(),
                    field_type: ConfigFieldType::Select {
                        options: vec!["yes".to_string(), "no".to_string()],
                        labels: vec![],
                    },
                    default_value: serde_json::json!("yes"),
                    section: None,
                    description_i18n: Some("configField.protectedModeDesc".to_string()),
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
        // 只绑某块网卡时 redis-cli 连 127.0.0.1 会被拒 → 优雅停止退化成强杀
        let host = crate::services::software_manager::netutils::bind_from_config(&ctx.config)
            .map(|b| crate::services::software_manager::netutils::probe_host_for_bind(&b))
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let mut args = vec!["-h".to_string(), host, "-p".to_string(), port.to_string()];
        if let Some(pw) = ctx.config.get("requirepass").and_then(|v| v.as_str()) {
            if !pw.is_empty() {
                args.push("-a".to_string());
                args.push(pw.to_string());
            }
        }
        args.push("shutdown".to_string());

        // *nix 上源码编译产物在 <install>/src/redis-cli；Windows 包内在安装根。
        let root = PathBuf::from(&ctx.install_path);
        let cli = if crate::utils::platform::current_os() == "windows" {
            root.join(exe_name("redis-cli"))
        } else {
            let built = root.join("src").join("redis-cli");
            if built.exists() { built } else { root.join(exe_name("redis-cli")) }
        };
        Some(GracefulStopCommand {
            program: cli.to_string_lossy().to_string(),
            args,
            working_dir: PathBuf::from(&ctx.install_path),
            env_vars: std::collections::BTreeMap::new(),
            timeout_secs: 10,
        })
    }
}

/// `bind` 里是否含有对外可达的地址（`0.0.0.0` / 具体网卡 IP 都算）。
///
/// 不能复用 `probe_host_for_bind`：通配地址会被它归一成 `127.0.0.1`（连接用），
/// 而判断「是否对外」时 `0.0.0.0` 恰恰是最外露的一种。
fn bind_is_external(bind: &str) -> bool {
    bind.split_whitespace()
        .map(|t| t.trim().trim_start_matches('-'))
        .filter(|t| !t.is_empty())
        .any(|t| match t.parse::<std::net::IpAddr>() {
            Ok(ip) => !ip.is_loopback(),
            Err(_) => !t.eq_ignore_ascii_case("localhost"),
        })
}

/// 改写 redis.conf 里与「能不能从外部连上」相关的三行：`bind` / `protected-mode` /
/// 裸 `requirepass`（包内自带的空值写法是语法错误，原样启动会直接失败）。
///
/// 未出现的指令追加到文件末尾 —— Redis 对同名单值指令「后者生效」，追加是安全的。
fn rewrite_redis_conf(raw: &str, bind: &str) -> String {
    let bind = bind.trim();
    let has_password = raw.lines().any(|line| {
        let t = line.trim();
        !t.starts_with('#')
            && t.starts_with("requirepass")
            && !t["requirepass".len()..].trim().is_empty()
    });
    let need_protected = !(bind_is_external(bind) && !has_password);

    let mut out: Vec<String> = Vec::new();
    let mut saw_bind = false;
    let mut saw_protected = false;
    for line in raw.lines() {
        let t = line.trim();
        let is_directive = !t.starts_with('#') && !t.is_empty();
        if is_directive && (t == "bind" || t.starts_with("bind ")) {
            out.push(format!("bind {bind}"));
            saw_bind = true;
            continue;
        }
        if is_directive && t.starts_with("protected-mode") {
            out.push(format!(
                "protected-mode {}",
                if need_protected { "yes" } else { "no" }
            ));
            saw_protected = true;
            continue;
        }
        // 裸 `requirepass`（无值）会被 Redis 判语法错误
        if is_directive && t.starts_with("requirepass") && t["requirepass".len()..].trim().is_empty()
        {
            out.push(format!("#{}", line));
            continue;
        }
        out.push(line.to_string());
    }
    if !saw_bind {
        out.push(format!("bind {bind}"));
    }
    if !saw_protected {
        out.push(format!(
            "protected-mode {}",
            if need_protected { "yes" } else { "no" }
        ));
    }
    out.join("\n")
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

#[cfg(test)]
mod tests {
    use super::{bind_is_external, rewrite_redis_conf};

    const SAMPLE: &str = "\
# Redis configuration file
bind 127.0.0.1 -::1
protected-mode yes
port 6379
requirepass
";

    /// 安装后必须能对外连：bind 换成系统地址，且「无密码 + 对外监听」要关保护模式。
    #[test]
    fn rewrites_bind_and_disables_protected_mode_without_password() {
        let out = rewrite_redis_conf(SAMPLE, "192.168.200.128");
        assert!(out.contains("bind 192.168.200.128"), "{out}");
        // 旧行（含 -::1）必须被替换而不是追加
        assert!(!out.contains("-::1"), "{out}");
        assert!(out.contains("protected-mode no"), "{out}");
        // 裸 requirepass 是语法错误，必须被注释掉
        assert!(out.contains("#requirepass"), "{out}");
    }

    /// 有密码时保护模式留着也无害（Redis 只在「默认用户无密码」时才拦外部连接）。
    #[test]
    fn keeps_protected_mode_when_password_set() {
        let raw = "bind 127.0.0.1\nprotected-mode yes\nrequirepass s3cret\n";
        let out = rewrite_redis_conf(raw, "10.0.0.5");
        assert!(out.contains("bind 10.0.0.5"), "{out}");
        assert!(out.contains("protected-mode yes"), "{out}");
        assert!(out.contains("requirepass s3cret"), "{out}");
    }

    /// 只绑本机时保护模式无需关闭（本来也出不去）。
    #[test]
    fn keeps_protected_mode_for_loopback_bind() {
        let out = rewrite_redis_conf("bind 0.0.0.0\nprotected-mode no\n", "127.0.0.1");
        assert!(out.contains("bind 127.0.0.1"), "{out}");
        assert!(out.contains("protected-mode yes"), "{out}");
    }

    /// 缺指令时要追加（Redis 后者生效），不能静默不改。
    #[test]
    fn appends_missing_directives() {
        let out = rewrite_redis_conf("port 6379\n", "192.168.1.2");
        assert!(out.contains("bind 192.168.1.2"), "{out}");
        assert!(out.contains("protected-mode no"), "{out}");
    }

    #[test]
    fn bind_external_detection() {
        assert!(bind_is_external("0.0.0.0"));
        assert!(bind_is_external("::"));
        assert!(bind_is_external("192.168.200.128"));
        assert!(bind_is_external("127.0.0.1 10.0.0.5"));
        assert!(!bind_is_external("127.0.0.1 -::1"));
        assert!(!bind_is_external("localhost"));
    }
}
