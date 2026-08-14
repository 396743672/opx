use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, MirrorSource, SoftwareCategory,
};

use super::{
    ConfigContext, FirstRunInit, HealthContext, InstallContext, SoftwareProvider, StartCommand,
    StartContext, WorkingDirContext,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

pub struct PostgreSqlProvider;

impl PostgreSqlProvider {
    pub fn new() -> Self { Self }
}
impl Default for PostgreSqlProvider {
    fn default() -> Self { Self::new() }
}

impl SoftwareProvider for PostgreSqlProvider {
    fn key(&self) -> &str { "postgresql" }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];
        #[cfg(windows)]
        {
            // 执行时核实：PostgreSQL 官方 Windows binaries 由 EnterpriseDB 提供，
            // URL 形如 https://get.enterprisedb.com/postgresql/postgresql-<ver>-windows-x64-binaries.zip
            versions.push(CatalogVersion {
                version: "16.4".to_string(),
                mirrors: vec![MirrorSource {
                    name: "i18n:postgresqlOfficial".to_string(),
                    url: "https://get.enterprisedb.com/postgresql/postgresql-16.4-1-windows-x64-binaries.zip".to_string(),
                    builtin: None,
                }],
                archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
            });
        }
        CatalogEntry {
            key: "postgresql".to_string(),
            name: "PostgreSQL".to_string(),
            description: "关系型数据库".to_string(),
            description_i18n: Some("catalogDesc.postgresql".to_string()),
            category: SoftwareCategory::Database,
            icon: "mdi:database".to_string(),
            versions,
            default_version: "16.4".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> { Ok(()) }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        let working_dir = PathBuf::from(&ctx.install_path);
        let data_dir = working_dir.join("data");

        // 首次初始化：initdb -D data -U postgres --encoding=UTF8
        // 用户填了初始化密码 → 写临时 pwfile 到 app tmp，用 --pwfile + scram-sha-256；
        // 没填 → 保持 --auth=trust（本地开发无密码登录）
        // initialized 门控（对齐 mysql.rs）：仅「未初始化」时消费 init_password；
        // 已初始化后重传密码不消费（first_run_init 会被跳过），并防御性清理残留 pwfile。
        let mut init_args = vec![
            "-D".to_string(), data_dir.to_string_lossy().to_string(),
            "-U".to_string(), "postgres".to_string(),
            "--encoding=UTF8".to_string(),
        ];
        let pwfile_path = crate::utils::paths::tmp_dir().join(format!("pgpass-{}.tmp", ctx.installed_id));
        let initialized = ctx.config.get("initialized").and_then(|v| v.as_bool()).unwrap_or(false);
        if initialized {
            // 已初始化不应再有初始化密码临时文件，删除可能残留的明文文件
            let _ = std::fs::remove_file(&pwfile_path);
            init_args.push("--auth=trust".to_string());
        } else if let Some(pw) = &ctx.init_password {
            if !pw.is_empty() {
                std::fs::write(&pwfile_path, pw)?;
                init_args.push(format!("--pwfile={}", pwfile_path.to_string_lossy()));
                init_args.push("--auth=scram-sha-256".to_string());
                init_args.push("--auth-host=scram-sha-256".to_string());
            } else {
                init_args.push("--auth=trust".to_string());
            }
        } else {
            init_args.push("--auth=trust".to_string());
        }

        let init_command = StartCommand {
            program: "bin/initdb.exe".to_string(),
            args: init_args,
            env_vars: std::collections::BTreeMap::new(),
            working_dir: working_dir.clone(),
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: None,
        };

        Ok(StartCommand {
            program: "bin/postgres.exe".to_string(),
            args: vec!["-D".to_string(), data_dir.to_string_lossy().to_string()],
            env_vars: std::collections::BTreeMap::new(),
            working_dir,
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: Some(Box::new(FirstRunInit {
                init_command,
                temp_secret_output: None,
            })),
        })
    }

    fn health_check(&self, ctx: &HealthContext) -> HealthCheckSpec {
        let port = ctx.config.get("port").and_then(|v| v.as_u64()).map(|p| p as u16)
            .unwrap_or(if ctx.port > 0 { ctx.port } else { 5432 });
        HealthCheckSpec::Tcp { port, timeout_ms: 1000 }
    }

    fn config_schema(&self) -> Option<ConfigSchema> {
        Some(ConfigSchema {
            fields: vec![
                ConfigField {
                    key: "port".to_string(),
                    label_i18n: "configField.port".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(5432),
                    section: None,
                    description_i18n: Some("configField.portDesc".to_string()),
                },
                ConfigField {
                    key: "listen_addresses".to_string(),
                    label_i18n: "configField.listenAddresses".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("localhost"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "max_connections".to_string(),
                    label_i18n: "configField.maxConnections".to_string(),
                    field_type: ConfigFieldType::Number,
                    default_value: serde_json::json!(100),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "shared_buffers".to_string(),
                    label_i18n: "configField.sharedBuffers".to_string(),
                    field_type: ConfigFieldType::Size { units: vec!["MB".to_string(), "GB".to_string()] },
                    default_value: serde_json::json!("128MB"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "init_password".to_string(),
                    label_i18n: "configField.initPassword".to_string(),
                    field_type: ConfigFieldType::Password,
                    default_value: serde_json::json!(""),
                    section: None,
                    description_i18n: Some("configField.initPasswordDesc".to_string()),
                },
            ],
            // init_password 为一次性敏感字段：仅首次初始化消费，绝不落盘
            ephemeral_keys: vec!["init_password".to_string()],
        })
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> {
        // postgres.exe -D <install>/data 只读 <install>/data/postgresql.conf，
        // 相对 install_path 返回 data/ 子目录下的实际配置文件
        Some(PathBuf::from("data/postgresql.conf"))
    }

    fn working_dir(&self, ctx: &WorkingDirContext) -> PathBuf {
        PathBuf::from(&ctx.install_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::software_manager::providers::{HealthContext, StartContext};

    fn provider() -> PostgreSqlProvider { PostgreSqlProvider::new() }

    #[test]
    fn key_is_postgresql() { assert_eq!(provider().key(), "postgresql"); }

    #[test]
    fn catalog_is_database_category() {
        assert_eq!(provider().catalog_entry().category, SoftwareCategory::Database);
        assert_eq!(provider().catalog_entry().icon, "mdi:database");
    }

    #[test]
    fn health_check_tcp_5432_by_default() {
        let ctx = HealthContext {
            installed_id: "x".into(), install_path: "/p".into(), port: 0,
            config: serde_json::json!({}),
        };
        match provider().health_check(&ctx) {
            HealthCheckSpec::Tcp { port, .. } => assert_eq!(port, 5432),
            other => panic!("expected Tcp, got {:?}", other),
        }
    }

    #[test]
    fn health_check_uses_config_port() {
        let ctx = HealthContext {
            installed_id: "x".into(), install_path: "/p".into(), port: 0,
            config: serde_json::json!({ "port": 15432 }),
        };
        match provider().health_check(&ctx) {
            HealthCheckSpec::Tcp { port, .. } => assert_eq!(port, 15432),
            other => panic!("expected Tcp, got {:?}", other),
        }
    }

    #[test]
    fn config_file_is_postgresql_conf_relative() {
        let ctx = ConfigContext {
            install_path: "/p".into(), version: "16".into(), config: serde_json::json!({}),
        };
        assert_eq!(provider().config_file_path(&ctx).as_deref(), Some(std::path::Path::new("data/postgresql.conf")));
    }

    #[test]
    fn start_command_uses_postgres_exe_foreground() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/pg".into(), version: "16".into(),
            config: serde_json::json!({}), custom_start_command: None, init_password: None,
            jdk_install_path: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        assert_eq!(cmd.program, "bin/postgres.exe");
        assert!(cmd.args.iter().any(|a| a == "-D"));
        assert!(cmd.first_run_init.is_some(), "PG 首次启动需 initdb 初始化");
    }

    #[test]
    fn config_schema_has_port_and_shared_buffers() {
        let schema = provider().config_schema().unwrap();
        let keys: Vec<&str> = schema.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"port"));
        assert!(keys.contains(&"shared_buffers"));
        assert!(keys.contains(&"max_connections"));
    }

    #[test]
    fn start_command_with_init_password_uses_pwfile_and_scram() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/pg".into(), version: "16".into(),
            config: serde_json::json!({}), custom_start_command: None,
            init_password: Some("secret".into()),
            jdk_install_path: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        let init = cmd.first_run_init.as_ref().unwrap();
        assert!(init.init_command.args.iter().any(|a| a.starts_with("--pwfile=")),
            "设置密码时 initdb 应带 --pwfile");
        assert!(init.init_command.args.iter().any(|a| a.contains("scram")),
            "设置密码时认证应使用 scram-sha-256");
        assert!(!init.init_command.args.iter().any(|a| a.contains("trust")), "scram 时不应有 trust");
    }

    #[test]
    fn start_command_without_password_keeps_trust() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/pg".into(), version: "16".into(),
            config: serde_json::json!({}), custom_start_command: None, init_password: None,
            jdk_install_path: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        let init = cmd.first_run_init.as_ref().unwrap();
        assert!(init.init_command.args.iter().any(|a| a.contains("trust")),
            "未设密码时应保持 --auth=trust");
        assert!(!init.init_command.args.iter().any(|a| a.starts_with("--pwfile=")), "trust 时不应有 pwfile");
    }

    #[test]
    fn start_command_empty_password_keeps_trust() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/pg".into(), version: "16".into(),
            config: serde_json::json!({}), custom_start_command: None,
            init_password: Some(String::new()),
            jdk_install_path: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        let init = cmd.first_run_init.as_ref().unwrap();
        assert!(init.init_command.args.iter().any(|a| a.contains("trust")));
        assert!(!init.init_command.args.iter().any(|a| a.starts_with("--pwfile=")));
    }

    #[test]
    fn start_command_initialized_keeps_trust_and_no_pwfile() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/pg".into(), version: "16".into(),
            config: serde_json::json!({ "initialized": true }), custom_start_command: None,
            init_password: Some("secret".into()),
            jdk_install_path: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        let init = cmd.first_run_init.as_ref().unwrap();
        assert!(init.init_command.args.iter().any(|a| a.contains("trust")),
            "已初始化后重传密码不应消费，保持 trust");
        assert!(!init.init_command.args.iter().any(|a| a.starts_with("--pwfile=")),
            "已初始化后不应再写 pwfile");
    }

    #[test]
    fn config_schema_has_init_password_ephemeral() {
        let schema = provider().config_schema().unwrap();
        // 断言 init_password 字段存在（find 不到则 unwrap panic）
        schema.fields.iter().find(|f| f.key == "init_password").unwrap();
        assert!(schema.ephemeral_keys.contains(&"init_password".to_string()),
            "init_password 必须是 ephemeral（不落盘）");
    }
}
