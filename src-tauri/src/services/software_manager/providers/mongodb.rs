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

        let mut args = vec![
            "--dbpath".to_string(), abs_dbpath,
            "--bind_ip".to_string(), bind_ip,
            "--port".to_string(), port.to_string(),
            "--logpath".to_string(), log_path.to_string_lossy().to_string(),
        ];
        // 可选认证：auth_enabled=true 时开启访问控制（用户需在 mongosh 手动创建首个用户）
        if ctx.config.get("auth_enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            args.push("--auth".to_string());
        }

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
                ConfigField {
                    key: "auth_enabled".to_string(),
                    label_i18n: "configField.authEnabled".to_string(),
                    field_type: ConfigFieldType::Boolean,
                    default_value: serde_json::json!(false),
                    section: None,
                    description_i18n: Some("configField.authEnabledDesc".to_string()),
                },
            ],
            ephemeral_keys: vec![],
        })
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> { None }
}

fn config_str(c: &serde_json::Value, key: &str, default: &str) -> String {
    c.get(key).and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| default.to_string())
}
fn config_u64(c: &serde_json::Value, key: &str, default: u64) -> u64 {
    c.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> MongoDbProvider { MongoDbProvider::new() }

    #[test]
    fn key_is_mongodb() { assert_eq!(provider().key(), "mongodb"); }

    #[test]
    fn catalog_is_database() {
        assert_eq!(provider().catalog_entry().category, SoftwareCategory::Database);
        assert_eq!(provider().catalog_entry().icon, "mdi:leaf");
    }

    #[test]
    fn no_config_file() {
        let ctx = ConfigContext { install_path: "/p".into(), version: "7".into(), config: serde_json::json!({}) };
        assert!(provider().config_file_path(&ctx).is_none(), "MongoDB 走纯 CLI，无配置文件");
    }

    #[test]
    fn start_command_maps_port_and_dbpath() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/mg".into(), version: "7".into(),
            config: serde_json::json!({ "port": 27018, "bind_ip": "127.0.0.1" }),
            custom_start_command: None, init_password: None,
            jdk_install_path: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        assert_eq!(cmd.program, "bin/mongod.exe");
        assert!(cmd.args.iter().any(|a| a == "--port"));
        assert!(cmd.args.iter().any(|a| a == "27018"));
        assert!(cmd.args.iter().any(|a| a == "--bind_ip"));
        assert!(cmd.first_run_init.is_none(), "MongoDB 无需首次初始化");
    }

    #[test]
    fn health_check_tcp_27017_default() {
        let ctx = HealthContext { installed_id: "x".into(), install_path: "/p".into(), port: 0, config: serde_json::json!({}) };
        match provider().health_check(&ctx) {
            HealthCheckSpec::Tcp { port, .. } => assert_eq!(port, 27017),
            o => panic!("expected Tcp, got {:?}", o),
        }
    }

    #[test]
    fn config_schema_has_port_bind_ip_dbpath() {
        let schema = provider().config_schema().unwrap();
        let keys: Vec<&str> = schema.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"port"));
        assert!(keys.contains(&"bind_ip"));
        assert!(keys.contains(&"dbpath"));
    }

    #[test]
    fn start_command_enables_auth_when_configured() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/mg".into(), version: "7".into(),
            config: serde_json::json!({ "auth_enabled": true }),
            custom_start_command: None, init_password: None,
            jdk_install_path: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        assert!(cmd.args.iter().any(|a| a == "--auth"), "auth_enabled=true 应带 --auth");
    }

    #[test]
    fn start_command_no_auth_by_default() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/mg".into(), version: "7".into(),
            config: serde_json::json!({}),
            custom_start_command: None, init_password: None,
            jdk_install_path: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        assert!(!cmd.args.iter().any(|a| a == "--auth"), "默认不带 --auth");
    }

    #[test]
    fn config_schema_has_auth_field() {
        let schema = provider().config_schema().unwrap();
        let keys: Vec<&str> = schema.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"auth_enabled"));
        assert!(!keys.contains(&"root_user"), "root_user 不应存在（不消费的死凭据）");
        assert!(!keys.contains(&"root_password"), "root_password 不应存在（不消费的死凭据）");
    }
}
