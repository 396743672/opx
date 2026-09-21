use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, MirrorSource, SoftwareCategory,
};

use super::{
    ConfigContext, DataDirContext, HealthContext, InstallContext, LogContext, LogSource,
    LogSourceKind, SoftwareProvider, StartCommand, StartContext, default_log_sources,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

/// 官方版本清单：`current.json` = 每个 major 的**当前**版本（另有 `full.json` = 全部历史版本）。
/// MongoDB 官方在社区论坛把它作为「脚本取二进制」的推荐索引；无正式 API 文档，但结构长期稳定。
#[cfg(windows)]
const VERSION_MANIFEST_URL: &str = "https://downloads.mongodb.org/current.json";
/// 远程版本发现最多取几个（下拉里给最新几个即可，避免被历史版本灌满；与 consul 同口径）。
#[cfg(windows)]
const REMOTE_MAX_VERSIONS: usize = 3;

/// 纯解析：从 current.json 提取「Windows x86_64 社区版」可安装版本，按版本号降序、最多 N 个。
///
/// 过滤规则必须严格——错了会让用户下到装不上/不能用的包：
/// - 跳过 `development_release`（开发版）与 `release_candidate`（候选版）；
/// - 只取 `target == "windows" && arch == "x86_64" && edition == "base"`：
///   `enterprise` 是企业版，无授权装完也起不来；`target`/`arch` 不符则平台不匹配。
#[cfg(windows)]
fn parse_windows_versions(manifest: &serde_json::Value) -> Vec<CatalogVersion> {
    let mut out: Vec<CatalogVersion> = Vec::new();
    let Some(versions) = manifest.get("versions").and_then(|v| v.as_array()) else {
        return out;
    };
    for v in versions {
        if v.get("development_release").and_then(|b| b.as_bool()).unwrap_or(false)
            || v.get("release_candidate").and_then(|b| b.as_bool()).unwrap_or(false)
        {
            continue;
        }
        let Some(version) = v.get("version").and_then(|s| s.as_str()) else {
            continue;
        };
        let Some(archive) = v
            .get("downloads")
            .and_then(|d| d.as_array())
            .and_then(|downloads| {
                downloads.iter().find(|d| {
                    d.get("target").and_then(|t| t.as_str()) == Some("windows")
                        && d.get("arch").and_then(|a| a.as_str()) == Some("x86_64")
                        && d.get("edition").and_then(|e| e.as_str()) == Some("base")
                })
            })
            .and_then(|d| d.get("archive"))
        else {
            continue;
        };
        let Some(url) = archive.get("url").and_then(|u| u.as_str()) else {
            continue;
        };
        out.push(CatalogVersion {
            version: version.to_string(),
            mirrors: vec![MirrorSource {
                name: "i18n:mongodbOfficial".to_string(),
                url: url.to_string(),
                builtin: None,
            }],
            archive: ArchiveInfo {
                format: ArchiveFormat::Zip,
                // current.json 不提供包大小
                size: None,
                sha256: archive.get("sha256").and_then(|s| s.as_str()).map(|s| s.to_string()),
            },
        });
    }
    out.sort_by(|a, b| crate::commands::software::compare_versions(&b.version, &a.version));
    out.truncate(REMOTE_MAX_VERSIONS);
    out
}

pub struct MongoDbProvider;

impl MongoDbProvider { pub fn new() -> Self { Self } }
impl Default for MongoDbProvider { fn default() -> Self { Self::new() } }

impl SoftwareProvider for MongoDbProvider {
    fn key(&self) -> &str { "mongodb" }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];
        #[cfg(windows)]
        {
            // 执行时核实：fastdl.mongodb.org Windows zip URL
            versions.push(CatalogVersion {
                version: "7.0.12".to_string(),
                mirrors: vec![MirrorSource {
                    name: "i18n:mongodbOfficial".to_string(),
                    url: "https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-7.0.12.zip".to_string(),
                    builtin: None,
                }],
                archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
            });
        }
        CatalogEntry {
            key: "mongodb".to_string(),
            name: "MongoDB".to_string(),
            description: "文档数据库".to_string(),
            description_i18n: Some("catalogDesc.mongodb".to_string()),
            category: SoftwareCategory::Database,
            icon: "mdi:leaf".to_string(),
            versions,
            default_version: "7.0.12".to_string(),
        }
    }

    /// 动态拉取 MongoDB 官方新版本（current.json，仅 Windows x86_64 社区版）。
    /// 拉取失败返回 None，不阻塞其他软件（与 minio / consul 同口径）。
    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        #[cfg(windows)]
        {
            let client = reqwest::blocking::Client::builder()
                // 关掉环境变量代理探测（ALL_PROXY 等）：宿主若设了不支持 CONNECT 的 HTTP 代理，
                // 本可直连的 fastdl/downloads.mongodb.org 反被劫持而失败
                .no_proxy()
                // blocking builder 无 read_timeout，而 timeout 是「连接→读体完成」的总 deadline：
                // current.json 约 400 KB，慢网下 15s 会在读体中途被掐断（报成 decoding 错误，
                // 症状像解析失败）→ 故总时长放宽到 60s，见 rustfs.rs 同名注释。
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .ok()?;
            let resp = client
                .get(VERSION_MANIFEST_URL)
                .header("User-Agent", "OPX")
                .send()
                .ok()?;
            if !resp.status().is_success() {
                eprintln!("[mongodb] 版本清单返回 {}", resp.status());
                return None;
            }
            let manifest: serde_json::Value = resp.json().ok()?;
            let versions = parse_windows_versions(&manifest);
            if versions.is_empty() {
                None
            } else {
                Some(versions)
            }
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    fn post_install(&self, ctx: &InstallContext) -> Result<()> {
        std::fs::create_dir_all(ctx.install_dir().join("data"))?;
        Ok(())
    }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        let port = config_u64(&ctx.config, "port", 27017);
        let bind_ip = config_str(&ctx.config, "bind_ip", "127.0.0.1");
        let dbpath = config_str(&ctx.config, "dbpath", "./data");
        let abs_dbpath = if std::path::Path::new(&dbpath).is_absolute() {
            dbpath.clone()
        } else {
            let clean = dbpath.strip_prefix("./").or_else(|| dbpath.strip_prefix(".\\")).unwrap_or(&dbpath);
            PathBuf::from(&ctx.install_path).join(clean).to_string_lossy().replace('\\', "/")
        };
        let _ = std::fs::create_dir_all(&abs_dbpath);
        let log_path = PathBuf::from(&ctx.install_path).join("data").join("mongod.log");
        let _ = std::fs::create_dir_all(log_path.parent().unwrap());

        let args = vec![
            "--dbpath".to_string(), abs_dbpath,
            "--bind_ip".to_string(), bind_ip,
            "--port".to_string(), port.to_string(),
            "--logpath".to_string(), log_path.to_string_lossy().to_string(),
        ];

        Ok(StartCommand {
            program: "bin/mongod.exe".to_string(),
            args,
            env_vars: std::collections::BTreeMap::new(),
            working_dir: PathBuf::from(&ctx.install_path),
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: None,
        })
    }

    fn health_check(&self, ctx: &HealthContext) -> HealthCheckSpec {
        let port = ctx.config.get("port").and_then(|v| v.as_u64()).map(|p| p as u16)
            .unwrap_or(if ctx.port > 0 { ctx.port } else { 27017 });
        HealthCheckSpec::Tcp { port, timeout_ms: 1000 }
    }

    fn config_schema(&self) -> Option<ConfigSchema> {
        Some(ConfigSchema {
            fields: vec![
                ConfigField {
                    key: "port".to_string(),
                    label_i18n: "configField.port".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(27017),
                    section: None,
                    description_i18n: Some("configField.portDesc".to_string()),
                },
                ConfigField {
                    key: "bind_ip".to_string(),
                    label_i18n: "configField.bindIp".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("127.0.0.1"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "dbpath".to_string(),
                    label_i18n: "configField.dataDir".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("./data"),
                    section: None,
                    description_i18n: Some("configField.dataDirDesc".to_string()),
                },
            ],
            ephemeral_keys: vec![],
            field_rules: vec![],
        })
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> { None }

    fn log_sources(&self, ctx: &LogContext) -> Vec<LogSource> {
        let mut sources = default_log_sources(ctx);
        // MongoDB 自带结构化日志：data/mongod.log
        let mongo_log = Path::new(&ctx.install_path).join("data").join("mongod.log");
        if mongo_log.exists() {
            sources.push(crate::services::software_manager::log_viewer::attach_archives(LogSource {
                path: mongo_log.to_string_lossy().to_string(),
                kind: LogSourceKind::ProviderFile,
                has_levels: true,
                level_pattern: None,
                label: None,
                archives: vec![],
            }));
        }
        sources
    }

    fn data_dirs(&self, ctx: &DataDirContext) -> Vec<PathBuf> {
        // dbpath 来自配置（默认 ./data），可能与默认 <install_path>/data 不同
        let dbpath = ctx
            .config
            .get("dbpath")
            .and_then(|v| v.as_str())
            .unwrap_or("./data");
        let abs = if Path::new(dbpath).is_absolute() {
            dbpath.to_string()
        } else {
            let clean = dbpath
                .strip_prefix("./")
                .or_else(|| dbpath.strip_prefix(".\\"))
                .unwrap_or(dbpath);
            Path::new(&ctx.install_path)
                .join(clean)
                .to_string_lossy()
                .to_string()
        };
        vec![PathBuf::from(abs)]
    }
}

fn config_str(c: &serde_json::Value, key: &str, default: &str) -> String {
    c.get(key).and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| default.to_string())
}
fn config_u64(c: &serde_json::Value, key: &str, default: u64) -> u64 {
    c.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// 真实 current.json 的结构（截取字段），含各类干扰项：
    /// 企业版条目、macOS 条目、候选版、开发版。
    fn manifest() -> serde_json::Value {
        serde_json::json!({
            "versions": [
                { "version": "8.3.11", "development_release": false, "release_candidate": false,
                  "downloads": [
                    { "target": "windows", "arch": "x86_64", "edition": "enterprise",
                      "archive": { "url": "https://downloads.mongodb.com/windows/mongodb-windows-x86_64-enterprise-8.3.11.zip", "sha256": "ee" } },
                    { "target": "macos", "arch": "x86_64", "edition": "base",
                      "archive": { "url": "https://fastdl.mongodb.org/osx/mongodb-macos-x86_64-8.3.11.tgz", "sha256": "mm" } },
                    { "target": "windows", "arch": "x86_64", "edition": "base",
                      "archive": { "url": "https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-8.3.11.zip", "sha256": "aa" } } ] },
                { "version": "8.1.0-rc1", "development_release": false, "release_candidate": true,
                  "downloads": [ { "target": "windows", "arch": "x86_64", "edition": "base",
                      "archive": { "url": "https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-8.1.0-rc1.zip", "sha256": "rc" } } ] },
                { "version": "9.0.0", "development_release": true, "release_candidate": false,
                  "downloads": [ { "target": "windows", "arch": "x86_64", "edition": "base",
                      "archive": { "url": "https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-9.0.0.zip", "sha256": "dev" } } ] },
                { "version": "8.0.32", "development_release": false, "release_candidate": false,
                  "downloads": [ { "target": "windows", "arch": "x86_64", "edition": "base",
                      "archive": { "url": "https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-8.0.32.zip", "sha256": "bb" } } ] },
                { "version": "7.0.43", "development_release": false, "release_candidate": false,
                  "downloads": [ { "target": "windows", "arch": "x86_64", "edition": "base",
                      "archive": { "url": "https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-7.0.43.zip", "sha256": "cc" } } ] },
                { "version": "6.0.29", "development_release": false, "release_candidate": false,
                  "downloads": [ { "target": "windows", "arch": "x86_64", "edition": "base",
                      "archive": { "url": "https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-6.0.29.zip", "sha256": "dd" } } ] }
            ]
        })
    }

    #[test]
    fn parse_picks_windows_base_newest_first_and_truncates() {
        let vs = parse_windows_versions(&manifest());
        // 8.1.0-rc1（候选版）与 9.0.0（开发版）被过滤；6.0.29 因超出上限被截断
        assert_eq!(
            vs.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
            vec!["8.3.11", "8.0.32", "7.0.43"]
        );
        let first = &vs[0];
        assert_eq!(first.mirrors.len(), 1);
        assert_eq!(first.mirrors[0].name, "i18n:mongodbOfficial");
        // 社区版（base）而非企业版：URL 必须是 fastdl 上不带 -enterprise- 的那个
        assert_eq!(
            first.mirrors[0].url,
            "https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-8.3.11.zip"
        );
        assert_eq!(first.archive.format, ArchiveFormat::Zip);
        assert_eq!(first.archive.sha256.as_deref(), Some("aa"));
    }

    #[test]
    fn parse_skips_versions_without_windows_base_build() {
        let no_base = serde_json::json!({ "versions": [
            { "version": "8.3.11", "downloads": [
                { "target": "windows", "arch": "aarch64", "edition": "base",
                  "archive": { "url": "u", "sha256": "x" } },
                { "target": "linux", "arch": "x86_64", "edition": "base",
                  "archive": { "url": "u", "sha256": "x" } } ] } ] });
        assert!(parse_windows_versions(&no_base).is_empty());
        // 结构异常（缺 versions / 非数组）不应 panic，而是返回空
        assert!(parse_windows_versions(&serde_json::json!({})).is_empty());
        assert!(parse_windows_versions(&serde_json::json!({ "versions": "oops" })).is_empty());
    }
}

