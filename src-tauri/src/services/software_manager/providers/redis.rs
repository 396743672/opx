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
            default_version: "8.8.0".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
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
        let port = ctx
            .config
            .get("port")
            .and_then(|v| v.as_u64())
            .map(|p| p as u16)
            .unwrap_or(6379);
        Ok(StartCommand {
            program: "redis-server.exe".to_string(),
            args: vec![
                "redis.conf".to_string(),
                "--port".to_string(),
                port.to_string(),
            ],
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
                    field_type: ConfigFieldType::Text,
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
        })
    }

    fn config_file_path(&self, ctx: &ConfigContext) -> Option<PathBuf> {
        Some(PathBuf::from(&ctx.install_path).join("redis.conf"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redis_749_has_builtin_as_first_mirror() {
        let entry = RedisProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "7.4.9")
            .expect("应有 7.4.9 版本");
        assert!(v.mirrors[0].builtin.is_some());
        assert_eq!(v.mirrors[0].builtin.as_ref().unwrap().version, "7.4.9");
    }

    #[test]
    fn redis_880_and_827_have_no_builtin() {
        let entry = RedisProvider::new().catalog_entry();
        for v in &entry.versions {
            if v.version != "7.4.9" {
                for m in &v.mirrors {
                    assert!(m.builtin.is_none(), "版本 {} 不应有 builtin", v.version);
                }
            }
        }
    }

    #[test]
    fn redis_fetch_remote_versions_method_exists() {
        let provider = RedisProvider::new();
        // 仅验证方法存在（编译通过），不断言返回值（网络可能失败）
        let _ = provider.fetch_remote_versions();
    }

    #[test]
    fn redis_start_command_uses_redis_conf_and_port() {
        let p = RedisProvider::new();
        let ctx = super::StartContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/redis/7.4.9".to_string(),
            version: "7.4.9".to_string(),
            config: serde_json::json!({"port": 6380}),
            custom_start_command: None,
        };
        let cmd = p.start_command(&ctx).unwrap();
        assert_eq!(cmd.program, "redis-server.exe");
        assert!(cmd.args.contains(&"redis.conf".to_string()));
        assert!(cmd.args.contains(&"--port".to_string()));
        assert!(cmd.args.contains(&"6380".to_string()));
        assert_eq!(cmd.working_dir, std::path::PathBuf::from("apps/redis/7.4.9"));
        assert!(cmd.first_run_init.is_none());
        assert_eq!(cmd.creation_flags, CREATE_NO_WINDOW);
    }

    #[test]
    fn redis_start_command_defaults_to_6379_when_config_missing() {
        let p = RedisProvider::new();
        let ctx = super::StartContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/redis/7.4.9".to_string(),
            version: "7.4.9".to_string(),
            config: serde_json::json!({}),
            custom_start_command: None,
        };
        let cmd = p.start_command(&ctx).unwrap();
        assert!(cmd.args.contains(&"--port".to_string()));
        assert!(cmd.args.contains(&"6379".to_string()));
    }

    #[test]
    fn redis_health_check_uses_tcp_port() {
        let p = RedisProvider::new();
        let ctx = super::HealthContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/redis/7.4.9".to_string(),
            port: 6379,
            config: serde_json::json!({}),
        };
        match p.health_check(&ctx) {
            crate::models::software::HealthCheckSpec::Tcp { port, .. } => {
                assert_eq!(port, 6379);
            }
            _ => panic!("应为 Tcp"),
        }
    }

    #[test]
    fn redis_health_check_uses_config_port_over_ctx_port() {
        let p = RedisProvider::new();
        let ctx = super::HealthContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/redis/7.4.9".to_string(),
            port: 6379,
            config: serde_json::json!({"port": 6390}),
        };
        match p.health_check(&ctx) {
            crate::models::software::HealthCheckSpec::Tcp { port, .. } => {
                assert_eq!(port, 6390, "config.port 应优先于 ctx.port");
            }
            _ => panic!("应为 Tcp"),
        }
    }

    #[test]
    fn redis_health_check_falls_back_to_6379_when_port_zero() {
        let p = RedisProvider::new();
        let ctx = super::HealthContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/redis/7.4.9".to_string(),
            port: 0,
            config: serde_json::json!({}),
        };
        match p.health_check(&ctx) {
            crate::models::software::HealthCheckSpec::Tcp { port, .. } => {
                assert_eq!(port, 6379);
            }
            _ => panic!("应为 Tcp"),
        }
    }

    #[test]
    fn redis_config_schema_has_five_fields() {
        let p = RedisProvider::new();
        let schema = p.config_schema().expect("Redis 应有 schema");
        assert_eq!(schema.fields.len(), 5);
        let keys: Vec<_> = schema.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"port"));
        assert!(keys.contains(&"bind"));
        assert!(keys.contains(&"maxmemory"));
        assert!(keys.contains(&"maxmemory-policy"));
        assert!(keys.contains(&"requirepass"));
        // Redis 是 KeyValue 格式（无 [section]），所有字段 section 必须为 None
        for f in &schema.fields {
            assert!(f.section.is_none(), "Redis 字段 {} 不应有 section", f.key);
        }
    }

    #[test]
    fn redis_config_file_path_returns_redis_conf() {
        let p = RedisProvider::new();
        let ctx = super::ConfigContext {
            install_path: "apps/redis/7.4.9".to_string(),
            version: "7.4.9".to_string(),
            config: serde_json::json!({}),
        };
        let path = p.config_file_path(&ctx).expect("应有路径");
        assert!(path.to_string_lossy().ends_with("redis.conf"));
    }
}
