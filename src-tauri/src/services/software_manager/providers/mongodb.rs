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
            remove_envs: Vec::new(),
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

