use anyhow::Result;
use std::path::PathBuf;

use opx_core::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, FieldRule, HealthCheckSpec, LogSource, MirrorSource,
    SoftwareCategory,
};

use super::{
    ConfigContext, FirstRunInit, GracefulStopCommand, HealthContext, InstallContext, LogContext,
    SoftwareProvider, StartCommand, StartContext, StopContext, WorkingDirContext,
    default_log_sources, exe_name,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

/// PostgreSQL 官方 Git 镜像的标签列表（`postgres/postgres` 是 PostgreSQL 全球开发组维护的官方镜像）。
/// 标签规范：stable 为 `REL_<major>_<minor>`（如 `REL_18_6`），预发布为 `REL_19_BETA3` / `REL_18_RC1`。
/// 注意 9.x 及更早是 `REL9_6_24` 形式（无下划线），不匹配本规则即天然被跳过。
const TAGS_URL: &str = "https://api.github.com/repos/postgres/postgres/tags?per_page=100";
/// EDB 官方 Windows x64 二进制基址（PostgreSQL 官方 Windows 包由 EDB 构建并分发）。
const EDB_BASE: &str = "https://get.enterprisedb.com/postgresql";
/// 远程版本发现最多取几个 major（每个 major 只取当前最新 minor；EDB 只构建当前支持版本）。
const REMOTE_MAX_VERSIONS: usize = 3;

/// 指定平台的官方二进制包 URL + 归档格式（EDB 构建，Windows/Linux/macOS 均提供）。
/// 构建号 `-1` 为 EDB 首个构建；实测不存在的版本会返回 403（可据此判断版本真伪）。
fn archive_url_for(version: &str, os: &str) -> (String, ArchiveFormat) {
    match os {
        "linux" => (
            format!("{}/postgresql-{}-linux-x64-binaries.tar.gz", EDB_BASE, version),
            ArchiveFormat::TarGz,
        ),
        "macos" => (
            format!("{}/postgresql-{}-osx-x86_64-binaries.tar.gz", EDB_BASE, version),
            ArchiveFormat::TarGz,
        ),
        _ => (
            format!("{}/postgresql-{}-1-windows-x64-binaries.zip", EDB_BASE, version),
            ArchiveFormat::Zip,
        ),
    }
}

/// 纯解析：从标签列表提取「当前支持的 major + 各 major 最新 minor」，按 major 降序取前 N 个。
/// 例：`REL_18_6` → `18.6`；`REL_19_BETA3` / `REL_18_RC1` 因第二段非纯数字自动跳过。
/// 与 OS 无关（纯标签解析），不可 cfg 锁——`fetch_remote_versions` 在非 Windows 也调用它。
fn parse_supported_versions(tags: &[serde_json::Value]) -> Vec<String> {
    use std::collections::BTreeMap;
    let mut latest_minor: BTreeMap<u64, u64> = BTreeMap::new();
    for t in tags {
        let Some(name) = t.get("name").and_then(|n| n.as_str()) else {
            continue;
        };
        let Some(rest) = name.strip_prefix("REL_") else {
            continue;
        };
        // 段数必须恰好两段：`REL_18_1_1` 之类（历史上的补丁重发）不取
        let mut parts = rest.split('_');
        let (Some(major), Some(minor), None) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(major), Ok(minor)) = (major.parse::<u64>(), minor.parse::<u64>()) else {
            continue;
        };
        latest_minor
            .entry(major)
            .and_modify(|m| {
                if minor > *m {
                    *m = minor;
                }
            })
            .or_insert(minor);
    }
    // major 降序、每个 major 取最新 minor
    latest_minor
        .into_iter()
        .rev()
        .take(REMOTE_MAX_VERSIONS)
        .map(|(major, minor)| format!("{}.{}", major, minor))
        .collect()
}

pub struct PostgreSqlProvider;

impl PostgreSqlProvider {
    pub fn new() -> Self { Self }
}
impl Default for PostgreSqlProvider {
    fn default() -> Self { Self::new() }
}

impl SoftwareProvider for PostgreSqlProvider {
    fn key(&self) -> &str { "postgresql" }

    fn catalog_entry(&self) -> CatalogEntry {
        // P2-1：运行时按 OS 选官方包，替代编译期 #[cfg(windows)] 锁（非 Windows 构建不再空目录）。
        // ⚠️ Linux/macOS 的 EDB 官方 URL 待跨平台环境验证（本会话仅 Windows 可验证）。
        let version = "16.4".to_string();
        let (url, format) = archive_url_for(&version, opx_core::utils::platform::current_os());
        let versions = vec![CatalogVersion {
            version: version.clone(),
            mirrors: vec![MirrorSource {
                name: "i18n:postgresqlOfficial".to_string(),
                url,
                builtin: None,
            }],
            archive: ArchiveInfo { format, size: None, sha256: None },
        }];
        CatalogEntry {
            key: "postgresql".to_string(),
            name: "PostgreSQL".to_string(),
            description: "关系型数据库".to_string(),
            description_i18n: Some("catalogDesc.postgresql".to_string()),
            category: SoftwareCategory::Database,
            icon: "mdi:elephant".to_string(),
            versions,
            default_version: "16.4".to_string(),
        }
    }

    /// 动态拉取 PostgreSQL 官方支持的版本（官方 Git 镜像标签，Windows x64 二进制由 EDB 构建）。
    ///
    /// 为什么走 GitHub 标签而不是 postgresql.org：官方 `versions.json` 所在域名在部分网络下不可达，
    /// 而 `postgres/postgres` 标签是官方镜像、内容等价（RELEASE 与 git tag 一一对应），且 API 可达。
    /// 拉取失败返回 None，不阻塞其他软件（与 minio / consul 同口径）。
    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        // P2-1：运行时按 OS 选官方包，替代编译期 #[cfg(windows)] 锁。
        let os = opx_core::utils::platform::current_os();
        let mut builder = reqwest::blocking::Client::builder()
            // blocking builder 无 read_timeout，timeout 是「连接→读体完成」的总 deadline：
            // 标签列表约 48 KB 虽小，但同一类失败（读体中途被掐断、报成 decoding 错误）
            // 在本机已实测复现，故统一放宽到 60s，详见 rustfs.rs 同名注释。
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(60));
        // Windows 上 GitHub 走系统代理不可达，强制直连；其他平台用默认代理。
        #[cfg(windows)]
        {
            builder = builder.no_proxy();
        }
        let client = builder.build().ok()?;
        let resp = client
            .get(TAGS_URL)
            .header("User-Agent", "OPX")
            .header("Accept", "application/vnd.github+json")
            .send()
            .ok()?;
        if !resp.status().is_success() {
            eprintln!("[postgresql] GitHub 标签 API 返回 {}", resp.status());
            return None;
        }
        let tags: Vec<serde_json::Value> = resp.json().ok()?;
        let versions: Vec<CatalogVersion> = parse_supported_versions(&tags)
            .into_iter()
            .map(|v| {
                let (url, format) = archive_url_for(&v, os);
                CatalogVersion {
                    mirrors: vec![MirrorSource {
                        name: "i18n:postgresqlOfficial".to_string(),
                        url,
                        builtin: None,
                    }],
                    version: v,
                    // EDB 不发布校验和文件（.sha256 / .md5 均 403），故 size/sha256 留空
                    archive: ArchiveInfo { format, size: None, sha256: None },
                }
            })
            .collect();
        if versions.is_empty() {
            None
        } else {
            Some(versions)
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> { Ok(()) }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        let working_dir = PathBuf::from(&ctx.install_path);
        let data_dir = working_dir.join("data");

        // 首次初始化：initdb -D data -U postgres --encoding=UTF8
        // 用户填了初始化密码 → 写临时 pwfile 到 app tmp，用 --pwfile + scram-sha-256；
        // 没填 → 保持 --auth=trust（本地开发无密码登录）
        // initialized 门控（对齐 mysql.rs）：仅「未初始化」时消费 init_password；
        // 已初始化后重传密码不消费（first_run_init 会被跳过），并防御性清理残留 pwfile。
        let mut init_args = vec![
            "-D".to_string(), data_dir.to_string_lossy().to_string(),
            "-U".to_string(), "postgres".to_string(),
            "--encoding=UTF8".to_string(),
        ];
        let pwfile_path = opx_core::utils::paths::tmp_dir().join(format!("pgpass-{}.tmp", ctx.installed_id));
        let initialized = ctx.config.get("initialized").and_then(|v| v.as_bool()).unwrap_or(false);
        if initialized {
            // 已初始化不应再有初始化密码临时文件，删除可能残留的明文文件
            let _ = std::fs::remove_file(&pwfile_path);
            init_args.push("--auth=trust".to_string());
        } else if let Some(pw) = &ctx.init_password {
            if !pw.is_empty() {
                std::fs::write(&pwfile_path, pw)?;
                init_args.push(format!("--pwfile={}", pwfile_path.to_string_lossy()));
                init_args.push("--auth=scram-sha-256".to_string());
                init_args.push("--auth-host=scram-sha-256".to_string());
            } else {
                init_args.push("--auth=trust".to_string());
            }
        } else {
            init_args.push("--auth=trust".to_string());
        }

        let init_command = StartCommand {
            program: format!("bin/{}", exe_name("initdb")),
            args: init_args,
            env_vars: std::collections::BTreeMap::new(),
            working_dir: working_dir.clone(),
            remove_envs: Vec::new(),
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: None,
        };

        Ok(StartCommand {
            program: format!("bin/{}", exe_name("postgres")),
            args: vec!["-D".to_string(), data_dir.to_string_lossy().to_string()],
            env_vars: std::collections::BTreeMap::new(),
            working_dir,
            remove_envs: Vec::new(),
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: Some(Box::new(FirstRunInit {
                init_command,
            })),
        })
    }

    fn health_check(&self, ctx: &HealthContext) -> HealthCheckSpec {
        let port = ctx.config.get("port").and_then(|v| v.as_u64()).map(|p| p as u16)
            .unwrap_or(if ctx.port > 0 { ctx.port } else { 5432 });
        HealthCheckSpec::Tcp { port, timeout_ms: 1000 }
    }

    fn config_schema(&self) -> Option<ConfigSchema> {
        Some(ConfigSchema {
            fields: vec![
                ConfigField {
                    key: "port".to_string(),
                    label_i18n: "configField.port".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(5432),
                    section: None,
                    description_i18n: Some("configField.portDesc".to_string()),
                },
                ConfigField {
                    key: "listen_addresses".to_string(),
                    label_i18n: "configField.listenAddresses".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("localhost"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "max_connections".to_string(),
                    label_i18n: "configField.maxConnections".to_string(),
                    field_type: ConfigFieldType::Number,
                    default_value: serde_json::json!(100),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "shared_buffers".to_string(),
                    label_i18n: "configField.sharedBuffers".to_string(),
                    field_type: ConfigFieldType::Size { units: vec!["MB".to_string(), "GB".to_string()] },
                    default_value: serde_json::json!("128MB"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "init_password".to_string(),
                    label_i18n: "configField.initPassword".to_string(),
                    field_type: ConfigFieldType::Password,
                    default_value: serde_json::json!(""),
                    section: None,
                    description_i18n: Some("configField.initPasswordDesc".to_string()),
                },
            ],
            // init_password 为一次性敏感字段：仅首次初始化消费，绝不落盘
            ephemeral_keys: vec!["init_password".to_string()],
            // 初始化凭据：首启前必填（避免无密码 postgres 弱口令），首启后由前端按 last_started_at 锁定禁用
            field_rules: vec![FieldRule {
                field_key: "init_password".to_string(),
                visible_when: None,
                required: true,
            }],
        })
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> {
        // postgres.exe -D <install>/data 只读 <install>/data/postgresql.conf，
        // 相对 install_path 返回 data/ 子目录下的实际配置文件
        Some(PathBuf::from("data/postgresql.conf"))
    }

    fn log_sources(&self, ctx: &LogContext) -> Vec<LogSource> {
        // C 扩展：stdout 为结构化级别日志，启用级别筛选下拉
        let mut sources = default_log_sources(ctx);
        for s in &mut sources {
            s.has_levels = true;
        }
        sources
    }

    fn working_dir(&self, ctx: &WorkingDirContext) -> PathBuf {
        PathBuf::from(&ctx.install_path)
    }

    /// P1-3：语义化优雅停止。`pg_ctl stop -D <data> -m fast -w` 向 postmaster 发停止信号并等待
    /// 其自行退出（无需 DB 密码，pg_ctl 用 data 目录下的 pidfile 定位进程）。`-m fast` 回滚
    /// 未提交事务后退出；`-w` 等待完成。避免 `taskkill /F` 强杀导致共享内存/数据文件损坏。
    fn graceful_stop_command(&self, ctx: &StopContext) -> Option<GracefulStopCommand> {
        let data_dir = PathBuf::from(&ctx.install_path).join("data");
        Some(GracefulStopCommand {
            program: PathBuf::from(&ctx.install_path)
                .join("bin")
                .join(exe_name("pg_ctl"))
                .to_string_lossy()
                .to_string(),
            args: vec![
                "stop".to_string(),
                "-D".to_string(),
                data_dir.to_string_lossy().to_string(),
                "-m".to_string(),
                "fast".to_string(),
                "-w".to_string(),
            ],
            working_dir: PathBuf::from(&ctx.install_path),
            env_vars: std::collections::BTreeMap::new(),
            timeout_secs: 15,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(names: &[&str]) -> Vec<serde_json::Value> {
        names
            .iter()
            .map(|n| serde_json::json!({ "name": n }))
            .collect()
    }

    #[test]
    fn parse_takes_latest_minor_per_major_newest_first() {
        let list = tags(&[
            // 预发布：第二段非纯数字，必须跳过（否则会把 beta 当正式版推给用户）
            "REL_19_BETA3",
            "REL_19_BETA2",
            "REL_18_RC1",
            "REL_18_6",
            "REL_18_4",
            "REL_17_11",
            "REL_17_9",
            "REL_16_15",
            "REL_16_4",
            // 9.x 及更早的标签格式不同（REL9_6_24），不应被当成 major 9 混进来
            "REL9_6_24",
            "release-6-3",
        ]);
        assert_eq!(parse_supported_versions(&list), vec!["18.6", "17.11", "16.15"]);
    }

    #[test]
    fn parse_handles_empty_and_malformed_input() {
        assert!(parse_supported_versions(&[]).is_empty());
        let list = tags(&["REL_18_1_1", "REL_", "REL_18_", "garbage"]);
        assert!(parse_supported_versions(&list).is_empty());
    }

    #[test]
    fn archive_url_uses_edb_build_one() {
        assert_eq!(
            archive_url_for("18.6", "windows").0,
            "https://get.enterprisedb.com/postgresql/postgresql-18.6-1-windows-x64-binaries.zip"
        );
    }

    #[test]
    fn archive_url_for_selects_platform_format() {
        let (w_url, w_fmt) = archive_url_for("16.4", "windows");
        assert!(w_url.ends_with("-windows-x64-binaries.zip"));
        assert_eq!(w_fmt, ArchiveFormat::Zip);
        let (l_url, l_fmt) = archive_url_for("16.4", "linux");
        assert!(l_url.ends_with("-linux-x64-binaries.tar.gz"));
        assert_eq!(l_fmt, ArchiveFormat::TarGz);
        let (m_url, m_fmt) = archive_url_for("16.4", "macos");
        assert!(m_url.ends_with("-osx-x86_64-binaries.tar.gz"));
        assert_eq!(m_fmt, ArchiveFormat::TarGz);
    }

    #[test]
    fn catalog_builtin_version_uses_same_url_rule() {
        let entry = PostgreSqlProvider::new().catalog_entry();
        assert_eq!(
            entry.versions[0].mirrors[0].url,
            archive_url_for("16.4", "windows").0
        );
    }
}

