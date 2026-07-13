use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, BuiltinInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, MirrorSource, SoftwareCategory,
};
use crate::services::software_manager::providers::builtin_manifest;

use super::{
    ConfigContext, HealthContext, InstallContext, SoftwareProvider, StartCommand, StartContext,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

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
        let mut versions = vec![];

        #[cfg(windows)]
        {
            versions.push(CatalogVersion {
                version: "8.8.0".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "i18n:redisWindowsGithub".to_string(),
                        url: "https://github.com/redis-windows/redis-windows/releases/download/8.8.0/Redis-8.8.0-Windows-x64-cygwin.zip".to_string(),
                        builtin: None,
                    },
                ],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
            versions.push(CatalogVersion {
                version: "8.2.7".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "i18n:redisWindowsGithub".to_string(),
                        url: "https://github.com/redis-windows/redis-windows/releases/download/8.2.7/Redis-8.2.7-Windows-x64-cygwin.zip".to_string(),
                        builtin: None,
                    },
                ],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
            versions.push(CatalogVersion {
                version: "7.4.9".to_string(),
                mirrors: {
                    let mut m = vec![];
                    let sha = builtin_manifest()
                        .get_builtin("redis", "7.4.9")
                        .map(|e| e.sha256.clone())
                        .unwrap_or_default();
                    let size = builtin_manifest()
                        .get_builtin("redis", "7.4.9")
                        .map(|e| e.size)
                        .unwrap_or(0);
                    m.push(MirrorSource {
                        name: "i18n:builtinVersion".to_string(),
                        url: "builtin://software/redis/7.4.9.zip".to_string(),
                        builtin: Some(BuiltinInfo {
                            version: "7.4.9".to_string(),
                            sha256: sha,
                            size: size,
                        }),
                    });
                    m.push(MirrorSource {
                        name: "i18n:redisWindowsGithub".to_string(),
                        url: "https://github.com/redis-windows/redis-windows/releases/download/7.4.9/Redis-7.4.9-Windows-x64-cygwin.zip".to_string(),
                        builtin: None,
                    });
                    m
                },
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
        }

        // 注：Redis 本设计 Windows-only（与 MySQL 决策一致）。
        // Unix 上 Redis 二进制名是 redis-server（无 .exe），配置路径与启动命令均不同，
        // 如需 Unix 支持须单独适配。本 provider 不在 Unix catalog 注册版本。

        CatalogEntry {
            key: "redis".to_string(),
            name: "Redis".to_string(),
            description: "内存键值存储".to_string(),
            category: SoftwareCategory::Cache,
            icon: "mdi:database".to_string(),
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
        Ok(())
    }

    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        // redis-windows GitHub Releases API（社区维护的 Windows Redis 移植）
        let url = "https://api.github.com/repos/redis-windows/redis-windows/releases?per_page=20";
        let response = reqwest::blocking::Client::builder()
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
            program: "redis-server.exe".to_string(),
            args: vec!["redis.conf".to_string()],
            env_vars: std::collections::BTreeMap::new(),
            working_dir: PathBuf::from(&ctx.install_path),
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
        })
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> {
        // 仅返回相对文件名，调用方（write_config_form / read_config_source）
        // 会自行拼接 install_path，避免双路径拼接 bug。
        Some(PathBuf::from("redis.conf"))
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
