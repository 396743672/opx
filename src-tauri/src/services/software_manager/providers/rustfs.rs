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
            // 版本 1：1.0.0-beta.8（内置 zip，离线）
            versions.push(CatalogVersion {
                version: "1.0.0-beta.8".to_string(),
                mirrors: {
                    let mut m = vec![];
                    let sha = builtin_manifest()
                        .get_builtin("rustfs", "1.0.0-beta.8")
                        .map(|e| e.sha256.clone())
                        .unwrap_or_default();
                    let size = builtin_manifest()
                        .get_builtin("rustfs", "1.0.0-beta.8")
                        .map(|e| e.size)
                        .unwrap_or(0);
                    m.push(MirrorSource {
                        name: "i18n:builtinVersion".to_string(),
                        url: "builtin://software/rustfs/1.0.0-beta.8.zip".to_string(),
                        builtin: Some(BuiltinInfo {
                            version: "1.0.0-beta.8".to_string(),
                            sha256: sha,
                            size: size,
                        }),
                    });
                    m
                },
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
            // 版本 2：latest（网络 zip）
            versions.push(CatalogVersion {
                version: "latest".to_string(),
                mirrors: vec![MirrorSource {
                    name: "i18n:rustfsOfficial".to_string(),
                    url: "https://dl.rustfs.com/artifacts/rustfs/release/rustfs-windows-x86_64-latest.zip".to_string(),
                    builtin: None,
                }],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
        }

        // 注：RustFS 本设计 Windows-only（与 MySQL/Redis/Nginx/MinIO 决策一致）。
        // Unix 上 RustFS 二进制名是 rustfs（无 .exe），如需 Unix 支持须单独适配。

        CatalogEntry {
            key: "rustfs".to_string(),
            name: "RustFS".to_string(),
            description: "Rust 实现的 S3 兼容对象存储".to_string(),
            category: SoftwareCategory::Database,
            icon: "mdi:cloud".to_string(),
            versions,
            default_version: "1.0.0-beta.8".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
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

        Ok(StartCommand {
            program: "rustfs.exe".to_string(),
            args: vec![
                data_dir,
                "--address".to_string(),
                format!("127.0.0.1:{}", api_port),
                "--access-key".to_string(),
                access_key,
                "--secret-key".to_string(),
                secret_key,
            ],
            env_vars,
            working_dir: PathBuf::from(&ctx.install_path),
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
        HealthCheckSpec::Http {
            url: format!("http://127.0.0.1:{}/health", port),
            expected_status: 200,
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
                    description_i18n: Some("configField.apiPort.desc".to_string()),
                },
                ConfigField {
                    key: "console_port".to_string(),
                    label_i18n: "configField.consolePort".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(9001),
                    section: None,
                    description_i18n: Some("configField.consolePort.desc".to_string()),
                },
                ConfigField {
                    key: "data_dir".to_string(),
                    label_i18n: "configField.dataDir".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("./data"),
                    section: None,
                    description_i18n: Some("configField.dataDir.desc".to_string()),
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
        })
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rustfs_has_two_versions() {
        let entry = RustfsProvider::new().catalog_entry();
        assert_eq!(entry.versions.len(), 2);
        let versions: Vec<_> = entry.versions.iter().map(|v| v.version.as_str()).collect();
        assert!(versions.contains(&"1.0.0-beta.8"));
        assert!(versions.contains(&"latest"));
    }

    #[test]
    fn rustfs_beta_version_has_builtin_zip() {
        let entry = RustfsProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "1.0.0-beta.8")
            .expect("应有 1.0.0-beta.8 版本");
        assert!(v.mirrors[0].builtin.is_some());
        assert_eq!(v.archive.format, ArchiveFormat::Zip);
    }

    #[test]
    fn rustfs_latest_version_is_network_zip() {
        let entry = RustfsProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "latest")
            .expect("应有 latest 版本");
        assert!(v.mirrors[0].builtin.is_none());
        assert_eq!(v.archive.format, ArchiveFormat::Zip);
    }

    #[test]
    fn rustfs_default_version_is_beta() {
        let entry = RustfsProvider::new().catalog_entry();
        assert_eq!(entry.default_version, "1.0.0-beta.8");
    }

    #[test]
    fn rustfs_start_command_uses_address_access_key_secret_key() {
        let p = RustfsProvider::new();
        let ctx = super::StartContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/rustfs/v1".to_string(),
            version: "v1".to_string(),
            config: serde_json::json!({
                "api_port": 9000,
                "console_port": 9001,
                "data_dir": "./data",
                "access_key": "rustfsadmin",
                "secret_key": "rustfsadmin"
            }),
            custom_start_command: None,
        };
        let cmd = p.start_command(&ctx).unwrap();
        assert_eq!(cmd.program, "rustfs.exe");
        assert_eq!(
            cmd.args,
            vec![
                "./data".to_string(),
                "--address".to_string(),
                "127.0.0.1:9000".to_string(),
                "--access-key".to_string(),
                "rustfsadmin".to_string(),
                "--secret-key".to_string(),
                "rustfsadmin".to_string(),
            ]
        );
        assert_eq!(cmd.env_vars.get("RUSTFS_CONSOLE_ENABLE").unwrap(), "true");
        assert_eq!(cmd.env_vars.get("RUSTFS_CONSOLE_ADDRESS").unwrap(), "127.0.0.1:9001");
        assert_eq!(cmd.working_dir, std::path::PathBuf::from("apps/rustfs/v1"));
        assert!(cmd.first_run_init.is_none());
        assert_eq!(cmd.creation_flags, CREATE_NO_WINDOW);
    }

    #[test]
    fn rustfs_start_command_defaults_when_config_missing() {
        let p = RustfsProvider::new();
        let ctx = super::StartContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/rustfs/v1".to_string(),
            version: "v1".to_string(),
            config: serde_json::json!({}),
            custom_start_command: None,
        };
        let cmd = p.start_command(&ctx).unwrap();
        assert!(cmd.args.contains(&"./data".to_string()));
        assert!(cmd.args.contains(&"127.0.0.1:9000".to_string()));
        assert!(cmd.args.contains(&"rustfsadmin".to_string()));
        assert_eq!(cmd.env_vars.get("RUSTFS_CONSOLE_ENABLE").unwrap(), "true");
        assert_eq!(cmd.env_vars.get("RUSTFS_CONSOLE_ADDRESS").unwrap(), "127.0.0.1:9001");
    }

    #[test]
    fn rustfs_health_check_uses_health_endpoint() {
        let p = RustfsProvider::new();
        let ctx = super::HealthContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/rustfs/v1".to_string(),
            port: 9005,
            config: serde_json::json!({}),
        };
        match p.health_check(&ctx) {
            crate::models::software::HealthCheckSpec::Http { url, expected_status, timeout_ms } => {
                assert_eq!(url, "http://127.0.0.1:9005/health");
                assert_eq!(expected_status, 200);
                assert_eq!(timeout_ms, 1000);
            }
            _ => panic!("应为 Http"),
        }
    }

    #[test]
    fn rustfs_health_check_uses_config_api_port_over_ctx_port() {
        let p = RustfsProvider::new();
        let ctx = super::HealthContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/rustfs/v1".to_string(),
            port: 9005,
            config: serde_json::json!({"api_port": 9002}),
        };
        match p.health_check(&ctx) {
            crate::models::software::HealthCheckSpec::Http { url, .. } => {
                assert_eq!(url, "http://127.0.0.1:9002/health");
            }
            _ => panic!("应为 Http"),
        }
    }

    #[test]
    fn rustfs_health_check_falls_back_to_9000_when_port_zero() {
        let p = RustfsProvider::new();
        let ctx = super::HealthContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/rustfs/v1".to_string(),
            port: 0,
            config: serde_json::json!({}),
        };
        match p.health_check(&ctx) {
            crate::models::software::HealthCheckSpec::Http { url, .. } => {
                assert_eq!(url, "http://127.0.0.1:9000/health");
            }
            _ => panic!("应为 Http"),
        }
    }

    #[test]
    fn rustfs_config_schema_has_five_fields() {
        let p = RustfsProvider::new();
        let schema = p.config_schema().expect("RustFS 应有 schema");
        assert_eq!(schema.fields.len(), 5);
        let keys: Vec<_> = schema.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"api_port"));
        assert!(keys.contains(&"console_port"));
        assert!(keys.contains(&"data_dir"));
        assert!(keys.contains(&"access_key"));
        assert!(keys.contains(&"secret_key"));
        for f in &schema.fields {
            assert!(f.section.is_none(), "RustFS 字段 {} 不应有 section", f.key);
        }
    }

    #[test]
    fn rustfs_config_file_path_returns_none() {
        let p = RustfsProvider::new();
        let ctx = super::ConfigContext {
            install_path: "apps/rustfs/v1".to_string(),
            version: "v1".to_string(),
            config: serde_json::json!({}),
        };
        assert!(p.config_file_path(&ctx).is_none());
    }
}
