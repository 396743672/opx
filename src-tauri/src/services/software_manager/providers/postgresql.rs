use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, LogSource, MirrorSource, SoftwareCategory,
};

use super::{
    ConfigContext, FirstRunInit, HealthContext, InstallContext, LogContext, SoftwareProvider,
    StartCommand, StartContext, WorkingDirContext, default_log_sources,
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
            icon: "mdi:elephant".to_string(),
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
            remove_envs: Vec::new(),
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: None,
        };

        Ok(StartCommand {
            program: "bin/postgres.exe".to_string(),
            args: vec!["-D".to_string(), data_dir.to_string_lossy().to_string()],
            env_vars: std::collections::BTreeMap::new(),
            working_dir,
            remove_envs: Vec::new(),
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
            field_rules: vec![],
        })
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> {
        // postgres.exe -D <install>/data 只读 <install>/data/postgresql.conf，
        // 相对 install_path 返回 data/ 子目录下的实际配置文件
        Some(PathBuf::from("data/postgresql.conf"))
    }

    fn log_sources(&self, ctx: &LogContext) -> Vec<LogSource> {
        // C 扩展：stdout 为结构化级别日志，启用级别筛选下拉
        let mut sources = default_log_sources(ctx);
        for s in &mut sources {
            s.has_levels = true;
        }
        sources
    }

    fn working_dir(&self, ctx: &WorkingDirContext) -> PathBuf {
        PathBuf::from(&ctx.install_path)
    }
}

