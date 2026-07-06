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
            // 版本 1：RELEASE.2021-04-22（内置 zip，离线）
            versions.push(CatalogVersion {
                version: "RELEASE.2021-04-22".to_string(),
                mirrors: {
                    let mut m = vec![];
                    let sha = builtin_manifest()
                        .get_builtin("minio", "RELEASE.2021-04-22")
                        .map(|e| e.sha256.clone())
                        .unwrap_or_default();
                    let size = builtin_manifest()
                        .get_builtin("minio", "RELEASE.2021-04-22")
                        .map(|e| e.size)
                        .unwrap_or(0);
                    m.push(MirrorSource {
                        name: "i18n:builtinVersion".to_string(),
                        url: "builtin://software/minio/RELEASE.2021-04-22.zip".to_string(),
                        builtin: Some(BuiltinInfo {
                            version: "RELEASE.2021-04-22".to_string(),
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
            // 版本 2：latest（网络 exe）
            versions.push(CatalogVersion {
                version: "latest".to_string(),
                mirrors: vec![MirrorSource {
                    name: "i18n:minioOfficial".to_string(),
                    url: "https://dl.min.io/server/minio/release/windows-amd64/minio.exe".to_string(),
                    builtin: None,
                }],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Executable,
                    size: None,
                    sha256: None,
                },
            });
        }

        // 注：MinIO 本设计 Windows-only（与 MySQL/Redis/Nginx 决策一致）。
        // Unix 上 MinIO 二进制名是 minio（无 .exe），如需 Unix 支持须单独适配。

        CatalogEntry {
            key: "minio".to_string(),
            name: "MinIO".to_string(),
            description: "S3 兼容对象存储".to_string(),
            category: SoftwareCategory::Database,
            icon: "mdi:cloud".to_string(),
            versions,
            default_version: "RELEASE.2021-04-22".to_string(),
        }
    }

    fn post_install(&self, ctx: &InstallContext) -> Result<()> {
        // 预创建 data_dir，避免 MinIO 启动时因目录不存在而崩溃
        let data_dir = ctx.install_dir().join("data");
        std::fs::create_dir_all(&data_dir)?;
        Ok(())
    }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        let api_port = config_u64(&ctx.config, "api_port", 9000);
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

        // MinIO 2021-04-22 不支持 --console-address（该参数在 2021-10 后引入），
        // 旧版控制台通过 MINIO_BROWSER_ADDRESS 环境变量设置，或默认在 api_port 上
        env_vars.insert(
            "MINIO_BROWSER_ADDRESS".to_string(),
            format!(":{}", config_u64(&ctx.config, "console_port", 9001)),
        );

        Ok(StartCommand {
            program: "minio.exe".to_string(),
            args: vec![
                "server".to_string(),
                abs_data_dir,
                "--address".to_string(),
                format!(":{}", api_port),
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
    fn minio_has_two_versions() {
        let entry = MinioProvider::new().catalog_entry();
        assert_eq!(entry.versions.len(), 2);
        let versions: Vec<_> = entry.versions.iter().map(|v| v.version.as_str()).collect();
        assert!(versions.contains(&"RELEASE.2021-04-22"));
        assert!(versions.contains(&"latest"));
    }

    #[test]
    fn minio_release_version_has_builtin_zip() {
        let entry = MinioProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "RELEASE.2021-04-22")
            .expect("应有 RELEASE.2021-04-22 版本");
        assert!(v.mirrors[0].builtin.is_some());
        assert_eq!(v.archive.format, ArchiveFormat::Zip);
    }

    #[test]
    fn minio_latest_version_is_network_executable() {
        let entry = MinioProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "latest")
            .expect("应有 latest 版本");
        assert!(v.mirrors[0].builtin.is_none());
        assert_eq!(v.archive.format, ArchiveFormat::Executable);
    }

    #[test]
    fn minio_default_version_is_release() {
        let entry = MinioProvider::new().catalog_entry();
        assert_eq!(entry.default_version, "RELEASE.2021-04-22");
    }

    #[test]
    fn minio_start_command_uses_server_data_address_console() {
        let p = MinioProvider::new();
        let ctx = super::StartContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/minio/RELEASE.2021-04-22".to_string(),
            version: "RELEASE.2021-04-22".to_string(),
            config: serde_json::json!({
                "api_port": 9000,
                "console_port": 9001,
                "data_dir": "./data",
                "access_key": "minioadmin",
                "secret_key": "minioadmin"
            }),
            custom_start_command: None,
        };
        let cmd = p.start_command(&ctx).unwrap();
        assert_eq!(cmd.program, "minio.exe");
        assert!(cmd.args.contains(&"server".to_string()));
        // data_dir 解析为绝对路径（install_path + data_dir）
        assert!(cmd
            .args
            .iter()
            .any(|a| a.ends_with("/data") || a.ends_with("\\data")));
        assert!(cmd.args.contains(&"--address".to_string()));
        assert!(cmd.args.contains(&":9000".to_string()));
        // MinIO 2021-04-22 无 --console-address，改用 MINIO_BROWSER_ADDRESS 环境变量
        assert!(!cmd.args.contains(&"--console-address".to_string()));
        assert_eq!(cmd.env_vars.get("MINIO_ROOT_USER").unwrap(), "minioadmin");
        assert_eq!(cmd.env_vars.get("MINIO_ROOT_PASSWORD").unwrap(), "minioadmin");
        assert_eq!(cmd.env_vars.get("MINIO_BROWSER_ADDRESS").unwrap(), ":9001");
        assert_eq!(cmd.working_dir, std::path::PathBuf::from("apps/minio/RELEASE.2021-04-22"));
        assert!(cmd.first_run_init.is_none());
        assert_eq!(cmd.creation_flags, CREATE_NO_WINDOW);
    }

    #[test]
    fn minio_start_command_defaults_when_config_missing() {
        let p = MinioProvider::new();
        let ctx = super::StartContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/minio/v1".to_string(),
            version: "v1".to_string(),
            config: serde_json::json!({}),
            custom_start_command: None,
        };
        let cmd = p.start_command(&ctx).unwrap();
        assert!(cmd.args.contains(&":9000".to_string()));
        // 控制台端口通过 MINIO_BROWSER_ADDRESS 环境变量设置（2021-04-22 无 --console-address）
        assert_eq!(cmd.env_vars.get("MINIO_BROWSER_ADDRESS").unwrap(), ":9001");
        assert!(cmd
            .args
            .iter()
            .any(|a| a.ends_with("/data") || a.ends_with("\\data")));
        assert_eq!(cmd.env_vars.get("MINIO_ROOT_USER").unwrap(), "minioadmin");
        assert_eq!(cmd.env_vars.get("MINIO_ROOT_PASSWORD").unwrap(), "minioadmin");
    }

    #[test]
    fn minio_health_check_uses_tcp_port() {
        let p = MinioProvider::new();
        let ctx = super::HealthContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/minio/v1".to_string(),
            port: 9000,
            config: serde_json::json!({}),
        };
        match p.health_check(&ctx) {
            crate::models::software::HealthCheckSpec::Tcp { port, .. } => {
                assert_eq!(port, 9000);
            }
            _ => panic!("应为 Tcp"),
        }
    }

    #[test]
    fn minio_health_check_uses_config_api_port_over_ctx_port() {
        let p = MinioProvider::new();
        let ctx = super::HealthContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/minio/v1".to_string(),
            port: 9000,
            config: serde_json::json!({"api_port": 9002}),
        };
        match p.health_check(&ctx) {
            crate::models::software::HealthCheckSpec::Tcp { port, .. } => {
                assert_eq!(port, 9002);
            }
            _ => panic!("应为 Tcp"),
        }
    }

    #[test]
    fn minio_health_check_falls_back_to_9000_when_port_zero() {
        let p = MinioProvider::new();
        let ctx = super::HealthContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/minio/v1".to_string(),
            port: 0,
            config: serde_json::json!({}),
        };
        match p.health_check(&ctx) {
            crate::models::software::HealthCheckSpec::Tcp { port, .. } => {
                assert_eq!(port, 9000);
            }
            _ => panic!("应为 Tcp"),
        }
    }

    #[test]
    fn minio_config_schema_has_five_fields() {
        let p = MinioProvider::new();
        let schema = p.config_schema().expect("MinIO 应有 schema");
        assert_eq!(schema.fields.len(), 5);
        let keys: Vec<_> = schema.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"api_port"));
        assert!(keys.contains(&"console_port"));
        assert!(keys.contains(&"data_dir"));
        assert!(keys.contains(&"access_key"));
        assert!(keys.contains(&"secret_key"));
        for f in &schema.fields {
            assert!(f.section.is_none(), "MinIO 字段 {} 不应有 section", f.key);
        }
    }

    #[test]
    fn minio_config_file_path_returns_none() {
        let p = MinioProvider::new();
        let ctx = super::ConfigContext {
            install_path: "apps/minio/v1".to_string(),
            version: "v1".to_string(),
            config: serde_json::json!({}),
        };
        assert!(p.config_file_path(&ctx).is_none());
    }
}
