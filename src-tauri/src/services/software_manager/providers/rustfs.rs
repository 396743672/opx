use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, MirrorSource, SoftwareCategory,
};

use super::{
    ConfigContext, DataDirContext, HealthContext, InstallContext, SoftwareProvider, StartCommand,
    StartContext, resolve_data_dir,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

/// 上游 GitHub Release 列表（`rustfs/rustfs`）。
#[cfg(windows)]
const RELEASES_URL: &str = "https://api.github.com/repos/rustfs/rustfs/releases?per_page=15";
/// 远程版本发现最多取几个。
#[cfg(windows)]
const REMOTE_MAX_VERSIONS: usize = 3;

/// 版本化 Windows 资产名：`rustfs-windows-x86_64-v<tag>.zip`。
///
/// 注意不能只按前缀匹配——stable release 里同时挂着 `rustfs-windows-x86_64-latest.zip`，
/// 那是个**会移动**的资产（内容随上游更新而变），会与 sha256 校验、断点续传的前提冲突。
fn asset_name(version: &str) -> String {
    format!("rustfs-windows-x86_64-v{}.zip", version)
}

/// 纯解析：从 GitHub Release 列表提取**正式版**（跳过 draft / prerelease）的 Windows 包。
///
/// 事实（2026-09-21 核实）：RustFS 目前只有 `1.0.0` 是正式版，其后全是
/// `1.0.1-preview.x` / `1.0.0-rc.x` 预发布。故这里刻意**只收正式版**——
/// 把 preview 当「可升级」推给用户会让人在运维环境里装上未发布的二进制。
/// 现状下本函数返回 `[1.0.0]`（与内置版本相同，合并时去重），等正式版发布即自动出现。
#[cfg(windows)]
fn parse_stable_releases(releases: &[serde_json::Value]) -> Vec<CatalogVersion> {
    let mut out: Vec<CatalogVersion> = Vec::new();
    for r in releases {
        if r.get("draft").and_then(|v| v.as_bool()).unwrap_or(false)
            || r.get("prerelease").and_then(|v| v.as_bool()).unwrap_or(false)
        {
            continue;
        }
        let Some(tag) = r.get("tag_name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(assets) = r.get("assets").and_then(|v| v.as_array()) else {
            continue;
        };
        // 必须精确匹配版本化资产（排除 -latest.zip / .sbom.json 等）
        let wanted = asset_name(tag);
        let Some(asset) = assets
            .iter()
            .find(|a| a.get("name").and_then(|v| v.as_str()) == Some(wanted.as_str()))
        else {
            continue;
        };
        out.push(CatalogVersion {
            version: tag.to_string(),
            mirrors: vec![MirrorSource {
                name: "i18n:rustfsOfficial".to_string(),
                // URL 含 github.com → 运行时经 download::resolve_url 自动加加速前缀。
                // 不加官网 CDN 作兜底：实测 CDN 只镜像正式版，预发布资产在 CDN 上是 404。
                url: format!(
                    "https://github.com/rustfs/rustfs/releases/download/{}/{}",
                    tag, wanted
                ),
                builtin: None,
            }],
            archive: ArchiveInfo {
                format: ArchiveFormat::Zip,
                size: asset.get("size").and_then(|v| v.as_u64()),
                sha256: asset
                    .get("digest")
                    .and_then(|v| v.as_str())
                    .and_then(|d| d.strip_prefix("sha256:"))
                    .map(|s| s.to_string()),
            },
        });
    }
    out.sort_by(|a, b| crate::commands::software::compare_versions(&b.version, &a.version));
    out.truncate(REMOTE_MAX_VERSIONS);
    out
}

fn config_str(ctx_config: &serde_json::Value, key: &str, default: &str) -> String {
    ctx_config
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| default.to_string())
}

fn config_u64(ctx_config: &serde_json::Value, key: &str, default: u64) -> u64 {
    ctx_config.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

pub struct RustfsProvider;

impl RustfsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RustfsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for RustfsProvider {
    fn key(&self) -> &str {
        "rustfs"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];

        #[cfg(windows)]
        {
            // 1.0.0（上游 GitHub Release 资产）
            // 原官网 CDN（dl.rustfs.com）不支持 Range（请求 0-99 却返回 200 全量），故改以 GitHub 资产为主：
            // 支持断点续传，且 URL 含 github.com 会被 download::resolve_url 自动加上 ghfast.top 前缀。
            // 版本固定为 1.0.0 而非 "latest"——latest 内容可变，与 sha256 校验、断点续传的前提冲突。
            versions.push(CatalogVersion {
                version: "1.0.0".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "i18n:rustfsOfficial".to_string(),
                        url: "https://github.com/rustfs/rustfs/releases/download/1.0.0/rustfs-windows-x86_64-v1.0.0.zip".to_string(),
                        builtin: None,
                    },
                    // 兜底：上游官网 CDN（实测无 Range 支持，多源回退时自动退化为整包重下）
                    MirrorSource {
                        name: "i18n:rustfsCdn".to_string(),
                        url: "https://dl.rustfs.com/artifacts/rustfs/release/rustfs-windows-x86_64-v1.0.0.zip".to_string(),
                        builtin: None,
                    },
                ],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    // 官方 SHA256SUMS 与 GitHub 独立 digest 双向互证（105,040,603 B）
                    size: Some(105_040_603),
                    sha256: Some(
                        "4ccf5858ce8e6f70f01af2394c8cc0e0878ee77faa6c20d3179153476554b7d8".to_string(),
                    ),
                },
            });
        }

        // 注：RustFS 本设计 Windows-only（与 MySQL/Redis/Nginx/MinIO 决策一致）。
        // Unix 上 RustFS 二进制名是 rustfs（无 .exe），如需 Unix 支持须单独适配。

        CatalogEntry {
            key: "rustfs".to_string(),
            name: "RustFS".to_string(),
            description: "Rust 实现的 S3 兼容对象存储".to_string(),
            description_i18n: Some("catalogDesc.rustfs".to_string()),
            category: SoftwareCategory::Storage,
            icon: "mdi:cloud".to_string(),
            versions,
            default_version: "1.0.0".to_string(),
        }
    }

    /// 动态拉取 RustFS 正式版（GitHub Release；预发布一律不收，见 `parse_stable_releases`）。
    /// 拉取失败返回 None，不阻塞其他软件（与 minio / consul 同口径）。
    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        #[cfg(windows)]
        {
            let client = reqwest::blocking::Client::builder()
                // 关掉环境变量代理探测（ALL_PROXY 等）：宿主若设了不支持 CONNECT 的 HTTP 代理，
                // 本可直连的 api.github.com 反被劫持而失败
                .no_proxy()
                // 超时取「connect 收紧 + 总时长放宽」：blocking builder 没有 read_timeout
                // （只有 async 的 Client 才有），而 timeout 是「连接→响应体读完」的总 deadline。
                // 实测 GitHub Release 列表 906 KB 在本机要 15.8s，15s 上限会在读体中途掐断，
                // 且被 reqwest 报成 "error decoding response body"（症状像解析失败，实为传输被截断）
                // → 表现为「版本发现静默失效」。故总时长放宽到 60s。
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
                eprintln!("[rustfs] GitHub API 返回 {}", resp.status());
                return None;
            }
            let releases: Vec<serde_json::Value> = resp.json().ok()?;
            let versions = parse_stable_releases(&releases);
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
        // 预创建 data_dir，避免 RustFS 启动时因目录不存在而崩溃
        let data_dir = ctx.install_dir().join("data");
        std::fs::create_dir_all(&data_dir)?;
        Ok(())
    }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        let api_port = config_u64(&ctx.config, "api_port", 9000);
        let console_port = config_u64(&ctx.config, "console_port", 9001);
        let data_dir = config_str(&ctx.config, "data_dir", "./data");
        let access_key = config_str(&ctx.config, "access_key", "rustfsadmin");
        let secret_key = config_str(&ctx.config, "secret_key", "rustfsadmin");

        let mut env_vars = std::collections::BTreeMap::new();
        env_vars.insert("RUSTFS_CONSOLE_ENABLE".to_string(), "true".to_string());
        env_vars.insert(
            "RUSTFS_CONSOLE_ADDRESS".to_string(),
            format!("127.0.0.1:{}", console_port),
        );

        // data_dir 解析为绝对路径，确保启动时目录存在
        let abs_data_dir = if std::path::Path::new(&data_dir).is_absolute() {
            data_dir.to_string()
        } else {
            let clean = data_dir
                .strip_prefix("./")
                .or_else(|| data_dir.strip_prefix(".\\"))
                .unwrap_or(&data_dir);
            std::path::PathBuf::from(&ctx.install_path)
                .join(clean)
                .to_string_lossy()
                .replace('\\', "/")
        };
        let _ = std::fs::create_dir_all(&abs_data_dir);

        // RustFS 命令格式（参考官方文档）：
        // rustfs --address :9000 --access-key <key> --secret-key <key> --console-enable <data_dir>
        Ok(StartCommand {
            program: "rustfs.exe".to_string(),
            args: vec![
                "--address".to_string(),
                format!(":{}", api_port),
                "--access-key".to_string(),
                access_key,
                "--secret-key".to_string(),
                secret_key,
                "--console-enable".to_string(),
                abs_data_dir,
            ],
            env_vars,
            working_dir: PathBuf::from(&ctx.install_path),
            remove_envs: Vec::new(),
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: None,
        })
    }

    fn health_check(&self, ctx: &HealthContext) -> HealthCheckSpec {
        let port = ctx
            .config
            .get("api_port")
            .and_then(|v| v.as_u64())
            .map(|p| p as u16)
            .unwrap_or(if ctx.port > 0 { ctx.port } else { 9000 });
        // 用 TCP 检查端口监听（RustFS 健康检查端点未明确文档化，TCP 更可靠）
        HealthCheckSpec::Tcp {
            port,
            timeout_ms: 1000,
        }
    }

    fn config_schema(&self) -> Option<ConfigSchema> {
        Some(ConfigSchema {
            fields: vec![
                ConfigField {
                    key: "api_port".to_string(),
                    label_i18n: "configField.apiPort".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(9000),
                    section: None,
                    description_i18n: Some("configField.apiPortDesc".to_string()),
                },
                ConfigField {
                    key: "console_port".to_string(),
                    label_i18n: "configField.consolePort".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(9001),
                    section: None,
                    description_i18n: Some("configField.consolePortDesc".to_string()),
                },
                ConfigField {
                    key: "data_dir".to_string(),
                    label_i18n: "configField.dataDir".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("./data"),
                    section: None,
                    description_i18n: Some("configField.dataDirDesc".to_string()),
                },
                ConfigField {
                    key: "access_key".to_string(),
                    label_i18n: "configField.accessKey".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("rustfsadmin"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "secret_key".to_string(),
                    label_i18n: "configField.secretKey".to_string(),
                    field_type: ConfigFieldType::Password,
                    default_value: serde_json::json!("rustfsadmin"),
                    section: None,
                    description_i18n: None,
                },
            ],
            ephemeral_keys: vec![],
            field_rules: vec![],
        })
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> {
        None
    }

    fn data_dirs(&self, ctx: &DataDirContext) -> Vec<PathBuf> {
        // RustFS 数据目录来自配置 data_dir（默认 ./data），需按 install_path 解析绝对路径
        vec![resolve_data_dir(&ctx.config, "data_dir", "./data", &ctx.install_path)]
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool, asset_names: &[&str]) -> serde_json::Value {
        serde_json::json!({
            "tag_name": tag,
            "draft": false,
            "prerelease": prerelease,
            "assets": asset_names.iter().map(|n| serde_json::json!({
                "name": n,
                "size": 105_000_000u64,
                "digest": "sha256:4ccf5858ce8e6f70f01af2394c8cc0e0878ee77faa6c20d3179153476554b7d8"
            })).collect::<Vec<_>>()
        })
    }

    #[test]
    fn parse_skips_prereleases_and_drafts() {
        let releases = vec![
            release("1.0.1-preview.8", true, &["rustfs-windows-x86_64-v1.0.1-preview.8.zip"]),
            release("1.0.0", false, &[
                "rustfs-windows-x86_64-latest.zip",
                "rustfs-windows-x86_64-v1.0.0.zip",
            ]),
        ];
        let vs = parse_stable_releases(&releases);
        assert_eq!(
            vs.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
            vec!["1.0.0"]
        );
    }

    #[test]
    fn parse_prefers_versioned_asset_not_moving_latest() {
        // 只有 -latest.zip（会移动）时不应采纳，否则 sha256 与内容会随上游变化而失配
        let releases = vec![release("1.0.0", false, &["rustfs-windows-x86_64-latest.zip"])];
        assert!(parse_stable_releases(&releases).is_empty());

        let releases = vec![release("1.0.0", false, &[
            "rustfs-windows-x86_64-latest.zip",
            "rustfs-windows-x86_64-v1.0.0.zip",
        ])];
        let vs = parse_stable_releases(&releases);
        assert_eq!(
            vs[0].mirrors[0].url,
            "https://github.com/rustfs/rustfs/releases/download/1.0.0/rustfs-windows-x86_64-v1.0.0.zip"
        );
        assert_eq!(vs[0].mirrors[0].name, "i18n:rustfsOfficial");
        assert_eq!(vs[0].archive.size, Some(105_000_000));
        assert_eq!(
            vs[0].archive.sha256.as_deref(),
            Some("4ccf5858ce8e6f70f01af2394c8cc0e0878ee77faa6c20d3179153476554b7d8")
        );
    }

    #[test]
    fn parse_sorts_desc_and_truncates() {
        let releases = vec![
            release("1.0.1", false, &["rustfs-windows-x86_64-v1.0.1.zip"]),
            release("1.10.0", false, &["rustfs-windows-x86_64-v1.10.0.zip"]),
            release("1.2.0", false, &["rustfs-windows-x86_64-v1.2.0.zip"]),
            release("1.9.0", false, &["rustfs-windows-x86_64-v1.9.0.zip"]),
        ];
        // 1.10.0 > 1.9.0（数字分段比较，不能按字符串比）
        assert_eq!(
            parse_stable_releases(&releases)
                .iter()
                .map(|v| v.version.as_str())
                .collect::<Vec<_>>(),
            vec!["1.10.0", "1.9.0", "1.2.0"]
        );
    }
}
