use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField, ConfigFieldType,
    ConfigSchema, HealthCheckSpec, MirrorSource, SoftwareCategory,
};

use super::{
    DataDirContext, HealthContext, InstallContext, SoftwareProvider, StartCommand, StartContext,
    resolve_data_dir,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

fn config_u64(c: &serde_json::Value, key: &str, default: u64) -> u64 {
    c.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

fn config_bool(c: &serde_json::Value, key: &str, default: bool) -> bool {
    c.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

fn config_str(c: &serde_json::Value, key: &str, default: &str) -> String {
    c.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| default.to_string())
}

/// InfluxDB 官方分发基址（`dl.influxdata.com` 目录不可枚举，只能按版本号拼 URL）。
const RELEASES_BASE: &str = "https://dl.influxdata.com/influxdb/releases";
/// 版本清单来源：`influxdata/influxdb` 的 Release（tag 形如 `v3.11.4`）。
/// 该 Release **不带资产**（二进制另发在 `dl.influxdata.com`），故版本号取自 tag。
const RELEASES_URL: &str = "https://api.github.com/repos/influxdata/influxdb/releases?per_page=30";
/// 远程版本发现最多取几个（每个版本一次请求已是上限，给最新 3 个足够）。
const REMOTE_MAX_VERSIONS: usize = 3;

/// 归档 URL（内置 catalog 与远程发现共用同一拼装规则；Windows 为 zip，其余平台为 linux tar.gz）。
fn archive_url(version: &str) -> String {
    #[cfg(windows)]
    let name = format!("influxdb3-core-{}-windows_amd64.zip", version);
    #[cfg(not(windows))]
    let name = format!("influxdb3-core-{}-linux-amd64.tar.gz", version);
    format!("{}/{}", RELEASES_BASE, name)
}

fn archive_format() -> ArchiveFormat {
    if cfg!(windows) {
        ArchiveFormat::Zip
    } else {
        ArchiveFormat::TarGz
    }
}

/// 纯解析：从 `influxdata/influxdb` 的 Release 列表提取 InfluxDB 3 正式版号，按版本号降序、最多 N 个。
///
/// 该仓库是 v1 / v2 / v3 共用仓库，标签混在一起，必须严格过滤：
/// - 跳过 `prerelease`；
/// - 只保留 `3.x.y` 三段纯数字（`v2.9.1` / `v1.13.1` 是 influxd 老架构，
///   `v3.11.0-rc1` 之类预发布名也因非纯数字被排除）。
fn parse_v3_releases(releases: &[serde_json::Value]) -> Vec<String> {
    let mut versions: Vec<String> = Vec::new();
    for r in releases {
        if r.get("prerelease").and_then(|p| p.as_bool()).unwrap_or(false) {
            continue;
        }
        let Some(tag) = r.get("tag_name").and_then(|t| t.as_str()) else {
            continue;
        };
        let ver = tag.strip_prefix('v').unwrap_or(tag);
        if !ver.starts_with("3.") {
            continue;
        }
        let Ok(re) = regex::Regex::new(r"^\d+\.\d+\.\d+$") else {
            continue;
        };
        if !re.is_match(ver) || versions.iter().any(|v| v == ver) {
            continue;
        }
        versions.push(ver.to_string());
    }
    versions.sort_by(|a, b| crate::commands::software::compare_versions(b, a));
    versions.truncate(REMOTE_MAX_VERSIONS);
    versions
}

/// InfluxDB 3 Core（influxdb3）：Rust 重写的新架构时序数据库。
/// 与 InfluxDB 2.x（influxd）是两个独立产品，不共享二进制/配置/数据，
/// 故单独一个 provider（key=influxdb3），互不干扰。
pub struct Influxdb3Provider;

impl Influxdb3Provider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Influxdb3Provider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for Influxdb3Provider {
    fn key(&self) -> &str {
        "influxdb3"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        // InfluxDB 3 Core Windows 包为 zip，UNIX 为 tar.gz。
        // dl.influxdata.com 目录不可枚举，故内置版本为静态列表（新版本由 fetch_remote_versions 补）。
        let format = archive_format();
        let versions = [
            "3.11.2", "3.10.0", "3.4.0", "3.3.0",
        ]
        .iter()
        .map(|v| CatalogVersion {
            version: v.to_string(),
            mirrors: vec![MirrorSource {
                name: "i18n:influxdbOfficial".to_string(),
                url: archive_url(v),
                builtin: None,
            }],
            archive: ArchiveInfo {
                format: format.clone(),
                size: None,
                sha256: None,
            },
        })
        .collect();

        CatalogEntry {
            key: "influxdb3".to_string(),
            name: "InfluxDB 3".to_string(),
            description: "时序数据库（InfluxDB 3 Core，新架构）".to_string(),
            description_i18n: Some("catalogDesc.influxdb3".to_string()),
            category: SoftwareCategory::TimeSeries,
            icon: "mdi:chart-line".to_string(),
            versions,
            default_version: "3.11.2".to_string(),
        }
    }

    /// 动态拉取 InfluxDB 3 Core 正式版（GitHub Release 的 `v3.x.y` 标签 + 官方分发 URL 拼装）。
    /// 拉取失败返回 None，不阻塞其他软件（与 minio / consul 同口径）。
    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            // blocking builder 无 read_timeout，timeout 是「连接→读体完成」的总 deadline：
            // Release 列表约 165 KB，本机实测 5.2s，慢网下 15s 会在读体中途被掐断
            // （报成 decoding 错误，症状像解析失败）→ 统一放宽到 60s，详见 rustfs.rs 同名注释。
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .ok()?;
        let resp = client
            .get(RELEASES_URL)
            .header("User-Agent", "OPX")
            .header("Accept", "application/vnd.github+json")
            .send()
            .ok()?;
        if !resp.status().is_success() {
            eprintln!("[influxdb3] GitHub API 返回 {}", resp.status());
            return None;
        }
        let releases: Vec<serde_json::Value> = resp.json().ok()?;
        let format = archive_format();
        let versions: Vec<CatalogVersion> = parse_v3_releases(&releases)
            .into_iter()
            .map(|v| CatalogVersion {
                mirrors: vec![MirrorSource {
                    name: "i18n:influxdbOfficial".to_string(),
                    url: archive_url(&v),
                    builtin: None,
                }],
                version: v,
                // 官方不在 Release / 分发目录旁发布校验和，size/sha256 留空（与内置版本一致）
                archive: ArchiveInfo { format: format.clone(), size: None, sha256: None },
            })
            .collect();
        if versions.is_empty() {
            None
        } else {
            Some(versions)
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
        // InfluxDB 3 Core 以纯 flag 启动，数据目录在 start_command 解析并创建。
        Ok(())
    }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        // InfluxDB 3 Core 为 Rust 原生二进制，免 JVM，直接运行 influxdb3 serve。
        // serve 需要 --object-store/--data-dir/--node-id，HTTP 端口由 --http-bind 指定（默认 8181）。
        let port = config_u64(&ctx.config, "port", 8181);
        let data_dir = resolve_data_dir(&ctx.config, "data_dir", "data", &ctx.install_path);
        std::fs::create_dir_all(&data_dir)?;

        let bin = if cfg!(windows) { "influxdb3.exe" } else { "influxdb3" };
        // 认证开关：
        // - auth_enabled=false（默认）：--without-auth 本地零认证，/health 返回 200。
        // - auth_enabled=true：--admin-token-file 携带 token，同时 --disable-authz health,ping
        //   放行健康检查（否则 /health 返回 401，opx 无法判定存活）；数据写入/查询需带 token。
        // 注意 http-bind 须用 127.0.0.1 显式 IPv4（仅 ":port" 会绑定到 IPv6 链路本地地址，
        // 导致 health_check 的 127.0.0.1 连不上）。
        let mut args = vec![
            "serve".to_string(),
            "--object-store=file".to_string(),
            format!("--data-dir={}", data_dir.to_string_lossy()),
            "--node-id=opx-node".to_string(),
            format!("--http-bind=127.0.0.1:{}", port),
        ];
        if config_bool(&ctx.config, "auth_enabled", false) {
            let token = config_str(&ctx.config, "admin_token", "");
            if token.is_empty() {
                return Err(anyhow::anyhow!("启用认证前请先配置管理员 Token（admin_token）"));
            }
            // token 文件格式与 `influxdb3 create token --offline` 产出一致：
            // {"token":"<value>","name":"_admin"}。每次启动重写，保证与配置同步。
            let token_file = data_dir.join("admin-token.json");
            std::fs::write(
                &token_file,
                serde_json::json!({ "token": token, "name": "_admin" }).to_string(),
            )?;
            args.push(format!("--admin-token-file={}", token_file.to_string_lossy()));
            args.push("--disable-authz=health,ping".to_string());
        } else {
            args.push("--without-auth".to_string());
        }

        Ok(StartCommand {
            program: bin.to_string(),
            args,
            env_vars: std::collections::BTreeMap::new(),
            working_dir: PathBuf::from(&ctx.install_path),
            creation_flags: CREATE_NO_WINDOW,
            // InfluxDB 3 Core 首次启动无需额外初始化（本地 file 存储即开即用）
            first_run_init: None,
        })
    }

    fn health_check(&self, ctx: &HealthContext) -> HealthCheckSpec {
        let port = ctx
            .config
            .get("port")
            .and_then(|v| v.as_u64())
            .map(|p| p as u16)
            .unwrap_or(8181);
        // InfluxDB 3 Core 提供 GET /health（--without-auth 下返回 200）
        HealthCheckSpec::Http {
            url: format!("http://127.0.0.1:{}/health", port),
            expected_status: 200,
            timeout_ms: 1000,
        }
    }

    fn data_dirs(&self, ctx: &DataDirContext) -> Vec<PathBuf> {
        // serve --data-dir 即数据目录，备份它
        vec![resolve_data_dir(
            &ctx.config,
            "data_dir",
            "data",
            &ctx.install_path,
        )]
    }

    fn config_schema(&self) -> Option<ConfigSchema> {
        Some(ConfigSchema {
            fields: vec![
                ConfigField {
                    key: "port".to_string(),
                    label_i18n: "configField.influxdbPort".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(8181),
                    section: None,
                    description_i18n: Some("configField.influxdbPortDesc".to_string()),
                },
                ConfigField {
                    key: "data_dir".to_string(),
                    label_i18n: "configField.influxdb3DataDir".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("data"),
                    section: None,
                    description_i18n: Some("configField.influxdb3DataDirDesc".to_string()),
                },
                ConfigField {
                    key: "auth_enabled".to_string(),
                    label_i18n: "configField.influxdb3AuthEnabled".to_string(),
                    field_type: ConfigFieldType::Boolean,
                    default_value: serde_json::json!(false),
                    section: None,
                    description_i18n: Some("configField.influxdb3AuthEnabledDesc".to_string()),
                },
                ConfigField {
                    key: "admin_token".to_string(),
                    label_i18n: "configField.influxdb3AdminToken".to_string(),
                    field_type: ConfigFieldType::Password,
                    default_value: serde_json::json!(""),
                    section: None,
                    description_i18n: Some("configField.influxdb3AdminTokenDesc".to_string()),
                },
            ],
            // admin_token 为持久化配置（写入 admin-token.json 供 --admin-token-file 使用），
            // 不设 ephemeral（重启后仍需用同一 token 访问数据）。
            ephemeral_keys: vec![],
            // 认证联动：auth_enabled=true 才显示 admin_token 且必填；false 时隐藏该字段。
            field_rules: vec![crate::models::software::FieldRule {
                field_key: "admin_token".to_string(),
                visible_when: Some(crate::models::software::FieldCondition {
                    key: "auth_enabled".to_string(),
                    equals: serde_json::json!(true),
                }),
                required: true,
            }],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> serde_json::Value {
        serde_json::json!({ "port": 8181, "data_dir": "data" })
    }

    fn test_install_path() -> String {
        std::env::temp_dir()
            .join("opx_test_influxdb3")
            .to_string_lossy()
            .to_string()
    }

    #[test]
    fn key_and_category() {
        let p = Influxdb3Provider::new();
        assert_eq!(p.key(), "influxdb3");
        assert_eq!(p.catalog_entry().category, SoftwareCategory::TimeSeries);
    }

    #[test]
    fn start_command_uses_influxdb3_serve() {
        let p = Influxdb3Provider::new();
        let install_path = test_install_path();
        let _ = std::fs::create_dir_all(&install_path);
        let ctx = StartContext {
            installed_id: "influxdb3-1".to_string(),
            install_path: install_path.clone(),
            version: "3.11.2".to_string(),
            config: test_config(),
            custom_start_command: None,
            init_password: None,
            jdk_install_path: None,
            mysql_install_path: None,
        };
        let cmd = p.start_command(&ctx).expect("start_command ok");
        assert!(cmd.program.contains("influxdb3"));
        assert!(cmd.args.iter().any(|a| a == "serve"));
        assert!(cmd.args.iter().any(|a| a.contains("--http-bind=127.0.0.1:8181")));
        assert!(cmd.args.iter().any(|a| a.contains("--object-store=file")));
        assert!(cmd.args.iter().any(|a| a == "--without-auth"));
    }

    #[test]
    fn start_command_with_auth_writes_token_file_and_disables_health_authz() {
        let p = Influxdb3Provider::new();
        let install_path = test_install_path();
        let _ = std::fs::create_dir_all(&install_path);
        let ctx = StartContext {
            installed_id: "influxdb3-1".to_string(),
            install_path: install_path.clone(),
            version: "3.11.2".to_string(),
            config: serde_json::json!({
                "port": 8181,
                "data_dir": "data",
                "auth_enabled": true,
                "admin_token": "apiv3_test-token",
            }),
            custom_start_command: None,
            init_password: None,
            jdk_install_path: None,
            mysql_install_path: None,
        };
        let cmd = p.start_command(&ctx).expect("start_command ok");
        // 开启认证 → 不再 --without-auth，改用 --admin-token-file，并放行 health/ping
        assert!(!cmd.args.iter().any(|a| a == "--without-auth"));
        assert!(cmd.args.iter().any(|a| a.starts_with("--admin-token-file=")));
        assert!(cmd.args.iter().any(|a| a == "--disable-authz=health,ping"));
        // token 文件已写入数据目录且格式正确
        let token_file = std::path::Path::new(&install_path)
            .join("data")
            .join("admin-token.json");
        let content = std::fs::read_to_string(&token_file).expect("token file written");
        let v: serde_json::Value = serde_json::from_str(&content).expect("valid json");
        assert_eq!(v["token"], "apiv3_test-token");
        assert_eq!(v["name"], "_admin");
    }

    #[test]
    fn health_check_is_http_8181_health() {
        let p = Influxdb3Provider::new();
        let hc = p.health_check(&HealthContext {
            installed_id: "influxdb3-1".to_string(),
            install_path: test_install_path(),
            port: 0,
            config: test_config(),
        });
        match hc {
            HealthCheckSpec::Http {
                url,
                expected_status,
                timeout_ms,
            } => {
                assert!(url.ends_with(":8181/health"));
                assert_eq!(expected_status, 200);
                assert_eq!(timeout_ms, 1000);
            }
            _ => panic!("expected Http health check"),
        }
    }

    fn release(tag: &str, prerelease: bool) -> serde_json::Value {
        serde_json::json!({ "tag_name": tag, "prerelease": prerelease })
    }

    #[test]
    fn parse_keeps_only_v3_stable_newest_first() {
        let releases = vec![
            release("v3.11.4", false),
            release("v3.11.3", false),
            // 同仓库的 v1 / v2 标签：influxd 老架构，与本 provider 无关
            release("v2.9.1", false),
            release("v1.13.1", false),
            release("v3.11.5-rc1", false),
            release("v3.12.0", true),
            release("v3.10.6", false),
            release("garbage", false),
        ];
        assert_eq!(
            parse_v3_releases(&releases),
            vec!["3.11.4", "3.11.3", "3.10.6"]
        );
    }

    #[test]
    fn parse_handles_empty_input_and_duplicates() {
        assert!(parse_v3_releases(&[]).is_empty());
        let dup = vec![release("v3.9.0", false), release("v3.9.0", false)];
        assert_eq!(parse_v3_releases(&dup), vec!["3.9.0"]);
    }

    #[test]
    fn archive_url_matches_platform_package_name() {
        #[cfg(windows)]
        assert_eq!(
            archive_url("3.11.4"),
            "https://dl.influxdata.com/influxdb/releases/influxdb3-core-3.11.4-windows_amd64.zip"
        );
        #[cfg(not(windows))]
        assert_eq!(
            archive_url("3.11.4"),
            "https://dl.influxdata.com/influxdb/releases/influxdb3-core-3.11.4-linux-amd64.tar.gz"
        );
    }

    #[test]
    fn catalog_versions_use_same_url_rule() {
        let entry = Influxdb3Provider::new().catalog_entry();
        for v in &entry.versions {
            assert_eq!(v.mirrors[0].url, archive_url(&v.version));
            assert_eq!(v.archive.format, archive_format());
        }
    }
}
