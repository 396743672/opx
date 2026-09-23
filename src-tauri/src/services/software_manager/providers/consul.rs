//! Consul provider：服务注册与发现（HashiCorp 官方分发）。
//!
//! 关键事实（2026-09-21 现场核实，勿凭直觉改）：
//! - **版本线**：Consul 自 2.0.0（2026-05-22）改用 V.M.F 版本模型，2.0.x 支持到 2028-04-30；
//!   1.22.x 于 2026-10-31 EOL。故内置版本取 2.0.4（当前最新社区版）。
//! - **下载源**：`releases.hashicorp.com`（CloudFront）。URL 不含 `github.com`，
//!   因此 `utils::download::resolve_candidates` 不会给它加加速前缀——本机实测直连 0.68s 可达。
//! - **包结构**：`consul_<ver>_windows_amd64.zip` 解压即 `consul.exe` + `LICENSE.txt`，
//!   无顶层目录 → 直解到 install_path 即 `<install_path>/consul.exe`（无需 post_install 改名）。
//! - **CLI**：官方 v2.0.x 文档中 `-dev` / `-server` / `-bootstrap-expect` / `-bind` /
//!   `-client` / `-data-dir` / `-http-port` 均仍受支持（仅 `-join`/`-join-wan` 自 1.15 弃用）。
//!   HTTP API 默认端口 8500，由 `-http-port` 覆盖。
//! - **健康检查**：`GET /v1/status/leader` 返回 Raft leader，无需 ACL（官方 API 文档 v2.0.x 实核）。
//!
//! 模式取舍：默认 `dev`（单节点、零配置、启动即用，但**数据不持久化**）；
//! `server` 为单节点 server 模式，写 `<install>/data` 持久化，适合需要保留 KV/Catalog 的场景。

use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField, ConfigFieldType,
    ConfigSchema, HealthCheckSpec, MirrorSource, SoftwareCategory,
};

use super::{
    HealthContext, InstallContext, LogContext, SoftwareProvider, StartCommand, StartContext,
    default_log_sources,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

/// Consul 官方分发基址（CloudFront CDN，国内可直连）。
const RELEASES_BASE: &str = "https://releases.hashicorp.com/consul";
/// 内置默认版本（最新社区版，2026-09-10 发布）。
const DEFAULT_VERSION: &str = "2.0.4";
/// 2.0.4 Windows amd64 包大小与 sha256（取自官方 consul_2.0.4_SHA256SUMS，已实下载比对）。
const DEFAULT_VERSION_SIZE: u64 = 72_737_934;
const DEFAULT_VERSION_SHA256: &str =
    "53430f0d0d28207005a40116f8d318aef023d3e44cad03affa29ddc3b6d43172";
/// HTTP API 默认端口（官方文档：`-http-port` 覆盖的默认值即 8500）。
const DEFAULT_HTTP_PORT: u16 = 8500;
/// 远程版本发现最多取几个（每个版本还需一次 SHA256SUMS 请求，避免刷新升级时请求过多）。
#[cfg(windows)]
const REMOTE_MAX_VERSIONS: usize = 3;

/// Windows amd64 归档文件名（导出供单测与远程发现复用同一拼装规则）。
fn archive_name(version: &str) -> String {
    format!("consul_{}_windows_amd64.zip", version)
}

fn archive_url(version: &str) -> String {
    format!("{}/{}/{}", RELEASES_BASE, version, archive_name(version))
}

pub struct ConsulProvider;

impl ConsulProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ConsulProvider {
    fn default() -> Self {
        Self::new()
    }
}

fn config_str(c: &serde_json::Value, key: &str, default: &str) -> String {
    c.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| default.to_string())
}

fn config_u16(c: &serde_json::Value, key: &str, default: u16) -> u16 {
    c.get(key)
        .and_then(|v| v.as_u64())
        .filter(|v| *v > 0 && *v <= u16::MAX as u64)
        .map(|v| v as u16)
        .unwrap_or(default)
}

impl SoftwareProvider for ConsulProvider {
    fn key(&self) -> &str {
        "consul"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];
        #[cfg(windows)]
        {
            versions.push(CatalogVersion {
                version: DEFAULT_VERSION.to_string(),
                mirrors: vec![MirrorSource {
                    name: "i18n:consulOfficial".to_string(),
                    url: archive_url(DEFAULT_VERSION),
                    builtin: None,
                }],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: Some(DEFAULT_VERSION_SIZE),
                    sha256: Some(DEFAULT_VERSION_SHA256.to_string()),
                },
            });
        }
        CatalogEntry {
            key: "consul".to_string(),
            name: "Consul".to_string(),
            description: "服务注册与发现".to_string(),
            description_i18n: Some("catalogDesc.consul".to_string()),
            category: SoftwareCategory::Registry,
            icon: "mdi:hexagon-multiple".to_string(),
            versions,
            default_version: DEFAULT_VERSION.to_string(),
        }
    }

    /// 动态拉取 HashiCorp 官方新版本（仅社区版 / Windows amd64）。
    /// 拉取失败返回 None，不阻塞其他软件（与 minio / influxdb 等实现一致）。
    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        #[cfg(windows)]
        {
            consul_remote_versions()
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
        // 包内即 consul.exe（无顶层目录、无版本后缀），无需重命名
        Ok(())
    }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        let working_dir = PathBuf::from(&ctx.install_path);
        let mode = config_str(&ctx.config, "mode", "dev");
        let bind = config_str(&ctx.config, "bind", "127.0.0.1");
        let http_port = config_u16(&ctx.config, "http_port", DEFAULT_HTTP_PORT);

        let mut args = vec!["agent".to_string()];
        if mode == "server" {
            // 单节点 server 模式：-bootstrap-expect=1 让其自行选举，data-dir 落盘持久化
            let data_dir = working_dir.join("data");
            let _ = std::fs::create_dir_all(&data_dir);
            args.push("-server".to_string());
            args.push("-bootstrap-expect=1".to_string());
            args.push(format!("-data-dir={}", data_dir.to_string_lossy()));
            // -bind 是集群通信地址（8300/8301/8302），-client 是 API/DNS 地址。
            // 二者都跟随用户配置的地址，避免「配了地址但 API 仍只监听回环」的困惑。
            args.push(format!("-bind={}", bind));
            args.push(format!("-client={}", bind));
        } else {
            // dev 模式：单节点、免配置、启动即用（数据不持久化）
            args.push("-dev".to_string());
            args.push(format!("-bind={}", bind));
        }
        args.push(format!("-http-port={}", http_port));

        Ok(StartCommand {
            program: "consul.exe".to_string(),
            args,
            env_vars: std::collections::BTreeMap::new(),
            working_dir,
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: None,
            remove_envs: Vec::new(),
        })
    }

    fn health_check(&self, ctx: &HealthContext) -> HealthCheckSpec {
        // /v1/status/leader 免 ACL，返回 Raft leader；Consul 未就绪时返回 500
        let port = ctx
            .config
            .get("http_port")
            .and_then(|v| v.as_u64())
            .filter(|v| *v > 0 && *v <= u16::MAX as u64)
            .map(|v| v as u16)
            .unwrap_or(if ctx.port > 0 {
                ctx.port
            } else {
                DEFAULT_HTTP_PORT
            });
        HealthCheckSpec::Http {
            url: format!("http://127.0.0.1:{}/v1/status/leader", port),
            expected_status: 200,
            timeout_ms: 1000,
        }
    }

    fn config_schema(&self) -> Option<ConfigSchema> {
        Some(ConfigSchema {
            fields: vec![
                ConfigField {
                    key: "mode".to_string(),
                    label_i18n: "configField.consulMode".to_string(),
                    field_type: ConfigFieldType::Select {
                        options: vec!["dev".to_string(), "server".to_string()],
                        labels: vec![],
                    },
                    default_value: serde_json::json!("dev"),
                    section: None,
                    description_i18n: Some("configField.consulModeDesc".to_string()),
                },
                ConfigField {
                    key: "bind".to_string(),
                    label_i18n: "configField.consulBind".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("127.0.0.1"),
                    section: None,
                    description_i18n: Some("configField.consulBindDesc".to_string()),
                },
                ConfigField {
                    key: "http_port".to_string(),
                    label_i18n: "configField.consulHttpPort".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(DEFAULT_HTTP_PORT),
                    section: None,
                    description_i18n: Some("configField.consulHttpPortDesc".to_string()),
                },
            ],
            ephemeral_keys: vec![],
            field_rules: vec![],
        })
    }

    fn log_sources(&self, ctx: &LogContext) -> Vec<crate::models::software::LogSource> {
        // Consul 日志形如 `2026-09-21T13:00:00.000Z [INFO]  agent: ...`，
        // 内置默认级别正则（\b(ERROR|WARN|INFO|...)\b）即可匹配，仅需启用级别筛选
        let mut sources = default_log_sources(ctx);
        for s in &mut sources {
            s.has_levels = true;
        }
        sources
    }
}

// ===== 远程版本发现 =====

/// 从 HashiCorp releases API 拉取社区版版本清单。
/// API 返回按发布时间倒序，故直接取前 N 个（跳过企业版 / 预发布 / 缺 Windows amd64 构建的）。
#[cfg(windows)]
fn consul_remote_versions() -> Option<Vec<CatalogVersion>> {
    let client = reqwest::blocking::Client::builder()
        // 关掉环境变量代理探测（ALL_PROXY 等）：宿主若设了不支持的代理，
        // 本可直连的 HashiCorp CDN 反被劫持而失败
        .no_proxy()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .ok()?;
    let resp = client
        .get("https://api.releases.hashicorp.com/v1/releases/consul?limit=20")
        .header("User-Agent", "OPX")
        .send()
        .ok()?;
    if !resp.status().is_success() {
        eprintln!("[consul] HashiCorp releases API 返回 {}", resp.status());
        return None;
    }
    let json: Vec<serde_json::Value> = resp.json().ok()?;

    let mut out = Vec::new();
    for version in parse_oss_windows_versions(&json)
        .into_iter()
        .take(REMOTE_MAX_VERSIONS)
    {
        // sha256 只能从每个版本的 SHA256SUMS 单独取（releases API 不返回哈希）；
        // 取不到则留 None（下载侧不校验，不影响安装）
        let sha256 = fetch_sha256(&client, &version);
        out.push(CatalogVersion {
            version: version.clone(),
            mirrors: vec![MirrorSource {
                name: "i18n:consulOfficial".to_string(),
                url: archive_url(&version),
                builtin: None,
            }],
            archive: ArchiveInfo {
                format: ArchiveFormat::Zip,
                size: None,
                sha256,
            },
        });
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// 拉取指定版本的官方 SHA256SUMS 并取出 Windows amd64 包的哈希（失败返回 None）。
#[cfg(windows)]
fn fetch_sha256(client: &reqwest::blocking::Client, version: &str) -> Option<String> {
    let url = format!("{}/{}/consul_{}_SHA256SUMS", RELEASES_BASE, version, version);
    let resp = client
        .get(&url)
        .header("User-Agent", "OPX")
        .send()
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body = resp.text().ok()?;
    parse_sha256_for_windows_amd64(&body)
}

/// 纯解析：从 releases API 响应提取「社区版 + 具备 Windows amd64 构建」的版本号（保持原顺序）。
/// 抽成独立函数以便单测——过滤规则（排除 `+ent` / 预发布）出错会让用户装到企业版包而启动失败。
#[cfg(windows)]
fn parse_oss_windows_versions(releases: &[serde_json::Value]) -> Vec<String> {
    let mut out = Vec::new();
    for r in releases {
        let Some(version) = r.get("version").and_then(|v| v.as_str()) else {
            continue;
        };
        // `+ent` / `+ent.fips1403` 为企业版，`-beta1` 等为预发布：都不纳入
        if version.contains('+') || version.contains('-') {
            continue;
        }
        let has_win_amd64 = r
            .get("builds")
            .and_then(|v| v.as_array())
            .map(|builds| {
                builds.iter().any(|b| {
                    b.get("os").and_then(|v| v.as_str()) == Some("windows")
                        && b.get("arch").and_then(|v| v.as_str()) == Some("amd64")
                        && b.get("url")
                            .and_then(|v| v.as_str())
                            .map(|u| u.ends_with(&archive_name(version)))
                            .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if has_win_amd64 {
            out.push(version.to_string());
        }
    }
    out
}

/// 纯解析：从 SHA256SUMS 文本取出 Windows amd64 包的行（`<hash>  <filename>`）。
#[cfg(windows)]
fn parse_sha256_for_windows_amd64(sums: &str) -> Option<String> {
    for line in sums.lines() {
        let mut parts = line.split_whitespace();
        let (Some(hash), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if hash.len() == 64 && name.ends_with("_windows_amd64.zip") {
            return Some(hash.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> ConsulProvider {
        ConsulProvider::new()
    }

    fn start_ctx(config: serde_json::Value) -> StartContext {
        StartContext {
            installed_id: "x".into(),
            install_path: "/c".into(),
            version: DEFAULT_VERSION.into(),
            config,
            custom_start_command: None,
            init_password: None,
            jdk_install_path: None,
            mysql_install_path: None,
        }
    }

    #[test]
    fn key_is_consul() {
        assert_eq!(provider().key(), "consul");
    }

    #[test]
    fn catalog_is_registry_category() {
        let entry = provider().catalog_entry();
        assert_eq!(entry.category, SoftwareCategory::Registry);
        assert_eq!(entry.icon, "mdi:hexagon-multiple");
        assert_eq!(entry.default_version, DEFAULT_VERSION);
    }

    #[cfg(windows)]
    #[test]
    fn catalog_version_is_official_with_hash() {
        let entry = provider().catalog_entry();
        let v = &entry.versions[0];
        assert_eq!(v.version, "2.0.4");
        assert_eq!(v.mirrors[0].name, "i18n:consulOfficial");
        assert_eq!(
            v.mirrors[0].url,
            "https://releases.hashicorp.com/consul/2.0.4/consul_2.0.4_windows_amd64.zip"
        );
        assert_eq!(v.archive.format, ArchiveFormat::Zip);
        assert_eq!(v.archive.sha256.as_deref(), Some(DEFAULT_VERSION_SHA256));
        assert_eq!(v.archive.size, Some(DEFAULT_VERSION_SIZE));
    }

    #[test]
    fn start_command_dev_mode_by_default() {
        let cmd = provider().start_command(&start_ctx(serde_json::json!({}))).unwrap();
        assert_eq!(cmd.program, "consul.exe");
        assert!(cmd.args.iter().any(|a| a == "agent"));
        assert!(cmd.args.iter().any(|a| a == "-dev"));
        assert!(cmd.args.iter().any(|a| a == "-http-port=8500"));
        assert!(!cmd.args.iter().any(|a| a.starts_with("-data-dir")));
        assert!(cmd.first_run_init.is_none());
    }

    #[test]
    fn start_command_server_mode_when_configured() {
        let ctx = start_ctx(serde_json::json!({ "mode": "server", "bind": "127.0.0.1" }));
        let cmd = provider().start_command(&ctx).unwrap();
        assert!(cmd.args.iter().any(|a| a == "-server"));
        assert!(cmd.args.iter().any(|a| a == "-bootstrap-expect=1"));
        assert!(cmd.args.iter().any(|a| a.starts_with("-data-dir=")));
        assert!(cmd.args.iter().any(|a| a == "-client=127.0.0.1"));
        assert!(!cmd.args.iter().any(|a| a == "-dev"), "server 模式不应带 -dev");
    }

    #[test]
    fn start_command_honours_http_port() {
        let ctx = start_ctx(serde_json::json!({ "http_port": 9500 }));
        let cmd = provider().start_command(&ctx).unwrap();
        assert!(cmd.args.iter().any(|a| a == "-http-port=9500"));
    }

    #[test]
    fn health_check_http_8500() {
        let ctx = HealthContext {
            installed_id: "x".into(),
            install_path: "/c".into(),
            port: 0,
            config: serde_json::json!({}),
        };
        match provider().health_check(&ctx) {
            HealthCheckSpec::Http {
                url,
                expected_status,
                ..
            } => {
                assert_eq!(expected_status, 200);
                assert_eq!(url, "http://127.0.0.1:8500/v1/status/leader");
            }
            o => panic!("expected Http, got {:?}", o),
        }
    }

    #[test]
    fn health_check_follows_configured_port() {
        let ctx = HealthContext {
            installed_id: "x".into(),
            install_path: "/c".into(),
            port: 0,
            config: serde_json::json!({ "http_port": 9500 }),
        };
        match provider().health_check(&ctx) {
            HealthCheckSpec::Http { url, .. } => assert!(url.contains(":9500/")),
            o => panic!("expected Http, got {:?}", o),
        }
    }

    #[test]
    fn config_schema_has_mode_select_field() {
        let schema = provider().config_schema().unwrap();
        let mode_field = schema.fields.iter().find(|f| f.key == "mode").unwrap();
        match &mode_field.field_type {
            ConfigFieldType::Select { options, .. } => {
                assert!(options.contains(&"dev".to_string()));
                assert!(options.contains(&"server".to_string()));
            }
            o => panic!("mode 应为 Select, got {:?}", o),
        }
        assert_eq!(mode_field.default_value, serde_json::json!("dev"));
        let port_field = schema.fields.iter().find(|f| f.key == "http_port").unwrap();
        assert_eq!(port_field.default_value, serde_json::json!(8500));
        assert!(matches!(port_field.field_type, ConfigFieldType::Port));
    }

    #[cfg(windows)]
    #[test]
    fn parse_oss_versions_skips_enterprise_and_prerelease() {
        let releases = serde_json::json!([
            { "version": "2.0.4+ent.fips1403", "builds": [
                { "os": "windows", "arch": "amd64", "url": "https://releases.hashicorp.com/consul/2.0.4+ent.fips1403/consul_2.0.4+ent.fips1403_windows_amd64.zip" }] },
            { "version": "2.0.5-beta1", "builds": [
                { "os": "windows", "arch": "amd64", "url": "https://releases.hashicorp.com/consul/2.0.5-beta1/consul_2.0.5-beta1_windows_amd64.zip" }] },
            { "version": "2.0.4", "builds": [
                { "os": "linux", "arch": "amd64", "url": "https://releases.hashicorp.com/consul/2.0.4/consul_2.0.4_linux_amd64.zip" },
                { "os": "windows", "arch": "amd64", "url": "https://releases.hashicorp.com/consul/2.0.4/consul_2.0.4_windows_amd64.zip" }] },
            { "version": "2.0.3", "builds": [
                { "os": "darwin", "arch": "arm64", "url": "https://releases.hashicorp.com/consul/2.0.3/consul_2.0.3_darwin_arm64.zip" },
                { "os": "windows", "arch": "amd64", "url": "https://releases.hashicorp.com/consul/2.0.3/consul_2.0.3_windows_amd64.zip" }] },
            { "version": "2.0.2+ent", "builds": [
                { "os": "windows", "arch": "amd64", "url": "https://releases.hashicorp.com/consul/2.0.2+ent/consul_2.0.2+ent_windows_amd64.zip" }] },
        ]);
        let releases = releases.as_array().unwrap();
        assert_eq!(parse_oss_windows_versions(releases), vec!["2.0.4", "2.0.3"]);
    }

    #[cfg(windows)]
    #[test]
    fn parse_sha256_picks_windows_amd64_only() {
        let sums = "\
9676144d944a78a1503d7466d52e520c9088b49962e2ebbc30cf1cf93f584764  consul_2.0.4_windows_386.zip
53430f0d0d28207005a40116f8d318aef023d3e44cad03affa29ddc3b6d43172  consul_2.0.4_windows_amd64.zip
aaaa  consul_2.0.4_linux_amd64.zip
";
        assert_eq!(
            parse_sha256_for_windows_amd64(sums).as_deref(),
            Some("53430f0d0d28207005a40116f8d318aef023d3e44cad03affa29ddc3b6d43172")
        );
        assert!(parse_sha256_for_windows_amd64("garbage\n").is_none());
    }
}
