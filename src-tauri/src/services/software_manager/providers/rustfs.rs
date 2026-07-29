use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, MirrorSource, SoftwareCategory,
};

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
            // latest（网络 zip）
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
            description_i18n: Some("catalogDesc.rustfs".to_string()),
            category: SoftwareCategory::Database,
            icon: "mdi:cloud".to_string(),
            versions,
            default_version: "latest".to_string(),
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
        })
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> {
        None
    }
}
