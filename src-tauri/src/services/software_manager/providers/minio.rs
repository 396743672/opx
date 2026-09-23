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

/// 兜底版本（上游 MinIO）：产物是裸 `minio.exe`（文件名带版本后缀，装后需重命名）。
/// 主版本 SILO 的产物则是 tar.gz 内的 `silo.exe`。两者二进制名不同，
/// 故启动命令与安装后处理都需按版本区分——否则已装的兜底实例会找不到可执行文件。
const MINIO_LEGACY_VERSION: &str = "RELEASE.2025-04-22";

/// 动态版本列表最多展示多少个（SILO 约 2~4 周发一版，10 个约覆盖半年）
#[cfg(windows)]
const SILO_MAX_REMOTE_VERSIONS: usize = 10;

/// 从 SILO 官方 GitHub Releases 拉取 Windows amd64 版本。
///
/// ⚠️ 只接受资产名以 `silo_` 开头的版本：SILO 在 2026-08-06 才由 minio 改名，
/// 此前 release 的 Windows 资产是 `minio_*.tar.gz`（内含 `minio.exe`），而
/// `start_command` 仅对 `MINIO_LEGACY_VERSION` 使用 `minio.exe` —— 若把改名前的
/// 版本也列出来，用户装完会「找不到 silo.exe」启动失败。
/// size / sha256 直接取自 GitHub 资产的 `size` 与 `digest`（形如 `sha256:<hex>`）字段。
#[cfg(windows)]
fn silo_remote_versions() -> Option<Vec<CatalogVersion>> {
    let url = "https://api.github.com/repos/pgsty/silo/releases?per_page=30";
    let client = reqwest::blocking::Client::builder()
        // 关掉环境变量代理探测（ALL_PROXY 等）：reqwest 默认会读，宿主若设了
        // 不支持 CONNECT 的 HTTP 代理，公网直连被劫持后必失败
        .no_proxy()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .ok()?;
    let resp = client
        .get(url)
        .header("User-Agent", "OPX")
        .header("Accept", "application/vnd.github+json")
        .send()
        .ok()?;
    if !resp.status().is_success() {
        eprintln!("[minio] SILO GitHub API 返回 {}", resp.status());
        return None;
    }
    let releases: Vec<serde_json::Value> = resp.json().ok()?;
    let out = parse_silo_releases(&releases);

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// 纯解析：从 SILO 的 GitHub Releases JSON 提取可直接安装的 Windows amd64 版本条目。
/// 抽成独立函数以便单测（网络调用不便测试，过滤规则却是出错代价最高的一环）。
#[cfg(windows)]
fn parse_silo_releases(releases: &[serde_json::Value]) -> Vec<CatalogVersion> {
    let mut out = Vec::new();
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
        // 主包：silo_<时间戳>_windows_amd64.tar.gz（排除 .sbom.json 等附带资产）
        let Some(asset) = assets.iter().find(|a| {
            a.get("name")
                .and_then(|v| v.as_str())
                .is_some_and(|n| n.starts_with("silo_") && n.ends_with("_windows_amd64.tar.gz"))
        }) else {
            continue;
        };
        let Some(name) = asset.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        out.push(CatalogVersion {
            version: tag.to_string(),
            mirrors: vec![MirrorSource {
                name: "i18n:siloOfficial".to_string(),
                // URL 含 github.com → 运行时经 download::resolve_url 自动加加速前缀
                url: format!("https://github.com/pgsty/silo/releases/download/{tag}/{name}"),
                builtin: None,
            }],
            archive: ArchiveInfo {
                format: ArchiveFormat::TarGz,
                size: asset.get("size").and_then(|v| v.as_u64()),
                sha256: asset
                    .get("digest")
                    .and_then(|v| v.as_str())
                    .and_then(|d| d.strip_prefix("sha256:"))
                    .map(|s| s.to_string()),
            },
        });
        if out.len() >= SILO_MAX_REMOTE_VERSIONS {
            break;
        }
    }
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

pub struct MinioProvider;

impl MinioProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MinioProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for MinioProvider {
    fn key(&self) -> &str {
        "minio"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];

        #[cfg(windows)]
        {
            // 主版本：SILO（MinIO 的社区维护 fork，pgsty/silo）。
            // 上游自 RELEASE.2025-05-24 起以「精简控制台」为由删除约 11 万行代码、控制台退化为
            // 纯对象浏览器，RELEASE.2025-10 起停发社区版预编译二进制，仓库已归档且官方拒修 CVE。
            // SILO 在同一代码库上恢复完整 Web 控制台、持续发布带 sha256/sigstore 的预编译二进制
            // 并跟进安全修复，故作为主版本；RELEASE.2025-04-22（官方最后完整控制台版）保留为兜底。
            versions.push(CatalogVersion {
                version: "RELEASE.2026-09-16T00-00-00Z".to_string(),
                mirrors: vec![MirrorSource {
                    name: "i18n:siloOfficial".to_string(),
                    // URL 含 github.com → 运行时经 download::resolve_url 自动加 ghfast.top 前缀。
                    // 包内为 LICENSE / NOTICE / README.md / silo.exe，无顶层目录，直解到安装根。
                    url: "https://github.com/pgsty/silo/releases/download/RELEASE.2026-09-16T00-00-00Z/silo_20260916000000.0.0_windows_amd64.tar.gz".to_string(),
                    builtin: None,
                }],
                archive: ArchiveInfo {
                    format: ArchiveFormat::TarGz,
                    // 官方 checksums.txt 逐字节核对（本地 sha256 一致）
                    size: Some(34_373_100),
                    sha256: Some(
                        "f99f4c376754aeea50c982afdd7aa3fd62ee9393e0ad603be570055c2342ee41".to_string(),
                    ),
                },
            });

            // 兜底版本：RELEASE.2025-04-22 —— 上游最后一个保留完整 Web 控制台的官方版本，
            // 已固化到自有 Release（tag res-v1），仅保留自有源。
            versions.push(CatalogVersion {
                version: "RELEASE.2025-04-22".to_string(),
                mirrors: vec![
                    // ⚠️ 资产名必须与上游逐字一致：ArchiveFormat::Executable 的缓存文件名取 URL
                    // basename（installer.rs），改名会导致已缓存文件失效、用户重新下载 121MB。
                    MirrorSource {
                        name: "i18n:selfHosted".to_string(),
                        url: format!(
                            "{}/minio.windows-amd64.RELEASE.2025-04-22T22-12-26Z.exe",
                            super::RESOURCE_BASE
                        ),
                        builtin: None,
                    },
                ],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Executable,
                    // 实测值（`minio --version` 自报 RELEASE.2025-04-22T22-12-26Z）
                    size: Some(121_008_640),
                    sha256: Some(
                        "2ceb3b3d68bdf1c4def9702cb02c5c8adb235197d1c8f2eaad24136833ab9a57".to_string(),
                    ),
                },
            });
        }

        // 注：本 provider 目前仅注册 Windows 版本（与 MySQL/Redis/Nginx 决策一致）。
        // SILO 上游同时发布 linux / darwin 的 amd64、arm64 包（二进制名 silo，无 .exe），
        // 若日后需要 Unix 支持，补对应 tar.gz 条目即可。

        CatalogEntry {
            key: "minio".to_string(),
            name: "MinIO".to_string(),
            description: "S3 兼容对象存储".to_string(),
            description_i18n: Some("catalogDesc.minio".to_string()),
            category: SoftwareCategory::Storage,
            icon: "mdi:cloud".to_string(),
            versions,
            default_version: "RELEASE.2026-09-16T00-00-00Z".to_string(),
        }
    }

    /// 动态拉取 SILO 上游新版本（Windows amd64）。
    /// 拉取失败返回 None，不阻塞其他软件（与 influxdb / jdk / jre 等实现一致）。
    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        #[cfg(windows)]
        {
            silo_remote_versions()
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    fn post_install(&self, ctx: &InstallContext) -> Result<()> {
        let data_dir = ctx.install_dir().join("data");
        std::fs::create_dir_all(&data_dir)?;
        // 兜底版本（上游 MinIO）下载到的是带版本后缀的裸 exe，需重命名为 minio.exe。
        // 主版本 SILO 的 tar.gz 解出即 silo.exe，无需处理——重命名反而会让启动命令找不到它。
        if ctx.version == MINIO_LEGACY_VERSION {
            if let Ok(entries) = std::fs::read_dir(ctx.install_dir()) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(false, |e| e == "exe")
                        && path.file_stem().map_or(true, |n| n != "minio")
                    {
                        let target = ctx.install_dir().join("minio.exe");
                        let _ = std::fs::rename(&path, &target);
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        let api_port = config_u64(&ctx.config, "api_port", 9000);
        let console_port = config_u64(&ctx.config, "console_port", 9001);
        let data_dir = config_str(&ctx.config, "data_dir", "./data");
        let access_key = config_str(&ctx.config, "access_key", "minioadmin");
        let secret_key = config_str(&ctx.config, "secret_key", "minioadmin");

        let mut env_vars = std::collections::BTreeMap::new();
        env_vars.insert("MINIO_ROOT_USER".to_string(), access_key);
        env_vars.insert("MINIO_ROOT_PASSWORD".to_string(), secret_key);

        // data_dir 解析为绝对路径（相对于 install_path），避免 minio 在错误目录创建
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

        // 二进制名按版本区分：SILO 是 silo.exe，兜底的上游 MinIO 是 minio.exe
        let program = if ctx.version == MINIO_LEGACY_VERSION {
            "minio.exe"
        } else {
            "silo.exe"
        };

        // 控制台端口统一用 --console-address 指定：SILO 与上游 MinIO 的 server 均支持该参数
        // （两者环境变量的官方名都是 MINIO_CONSOLE_ADDRESS，而非 MINIO_BROWSER_ADDRESS）。
        // 不指定时它会在每次启动随机选控制台端口，导致前端展示的 console_port 失效。
        let args = vec![
            "server".to_string(),
            abs_data_dir,
            "--address".to_string(),
            format!(":{}", api_port),
            "--console-address".to_string(),
            format!(":{}", console_port),
        ];

        Ok(StartCommand {
            program: program.to_string(),
            args,
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
        // 用 TCP 检查端口监听（比 HTTP 健康检查更可靠，不依赖具体端点路径）
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
                    default_value: serde_json::json!("minioadmin"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "secret_key".to_string(),
                    label_i18n: "configField.secretKey".to_string(),
                    field_type: ConfigFieldType::Password,
                    default_value: serde_json::json!("minioadmin"),
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
        // MinIO 数据目录来自配置 data_dir（默认 ./data），需按 install_path 解析绝对路径
        vec![resolve_data_dir(&ctx.config, "data_dir", "./data", &ctx.install_path)]
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn asset(name: &str, size: u64, digest: &str) -> serde_json::Value {
        serde_json::json!({ "name": name, "size": size, "digest": digest })
    }

    fn release(tag: &str, assets: Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({
            "tag_name": tag,
            "draft": false,
            "prerelease": false,
            "assets": assets,
        })
    }

    #[test]
    fn parse_picks_silo_windows_amd64_with_size_and_sha256() {
        let releases = vec![release(
            "RELEASE.2026-09-16T00-00-00Z",
            vec![
                asset(
                    "silo_20260916000000.0.0_windows_arm64.tar.gz",
                    30_711_564,
                    "sha256:aaa",
                ),
                asset(
                    "silo_20260916000000.0.0_windows_amd64.tar.gz",
                    34_373_100,
                    "sha256:f99f",
                ),
                asset(
                    "silo_20260916000000.0.0_windows_amd64.tar.gz.sbom.json",
                    417_604,
                    "sha256:bbb",
                ),
            ],
        )];
        let vs = parse_silo_releases(&releases);
        assert_eq!(vs.len(), 1);
        assert_eq!(vs[0].version, "RELEASE.2026-09-16T00-00-00Z");
        assert_eq!(vs[0].archive.size, Some(34_373_100));
        assert_eq!(vs[0].archive.sha256.as_deref(), Some("f99f"));
        assert_eq!(vs[0].mirrors.len(), 1);
        assert_eq!(
            vs[0].mirrors[0].url,
            "https://github.com/pgsty/silo/releases/download/RELEASE.2026-09-16T00-00-00Z/silo_20260916000000.0.0_windows_amd64.tar.gz"
        );
    }

    /// 回归：2026-08-06 改名前发布的是 `minio_*.tar.gz`（内含 minio.exe），
    /// 纳入列表会让用户装完「找不到 silo.exe」启动失败，必须跳过。
    #[test]
    fn parse_skips_pre_rename_minio_assets() {
        let releases = vec![release(
            "RELEASE.2026-08-04T00-00-00Z",
            vec![asset(
                "minio_20260804000000.0.0_windows_amd64.tar.gz",
                33_855_650,
                "sha256:ccc",
            )],
        )];
        assert!(parse_silo_releases(&releases).is_empty());
    }

    #[test]
    fn parse_skips_prerelease_and_draft() {
        let mut pre = release(
            "RELEASE.2026-10-01T00-00-00Z",
            vec![asset("silo_x_windows_amd64.tar.gz", 1, "sha256:d")],
        );
        pre["prerelease"] = serde_json::json!(true);
        let mut draft = release(
            "RELEASE.2026-10-02T00-00-00Z",
            vec![asset("silo_y_windows_amd64.tar.gz", 1, "sha256:e")],
        );
        draft["draft"] = serde_json::json!(true);
        assert!(parse_silo_releases(&[pre, draft]).is_empty());
    }

    #[test]
    fn parse_caps_version_count() {
        let releases: Vec<_> = (0..25)
            .map(|i| {
                release(
                    &format!("RELEASE.2026-01-{i:02}T00-00-00Z"),
                    vec![asset("silo_x_windows_amd64.tar.gz", 1, "sha256:f")],
                )
            })
            .collect();
        assert_eq!(
            parse_silo_releases(&releases).len(),
            SILO_MAX_REMOTE_VERSIONS
        );
    }
}
