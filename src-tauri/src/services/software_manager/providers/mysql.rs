use anyhow::Result;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, BuiltinInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, MirrorSource, SoftwareCategory,
};
use crate::services::software_manager::providers::builtin_manifest;

use super::{
    ConfigContext, FirstRunInit, HealthContext, InstallContext, SoftwareProvider, StartCommand,
    StartContext, WorkingDirContext,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

pub struct MySqlProvider;

impl MySqlProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MySqlProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for MySqlProvider {
    fn key(&self) -> &str {
        "mysql"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];

        #[cfg(windows)]
        {
            versions.push(CatalogVersion {
                version: "8.4.10".to_string(),
                mirrors: {
                    let mut m = vec![];
                    let sha = builtin_manifest()
                        .get_builtin("mysql", "8.4.10")
                        .map(|e| e.sha256.clone())
                        .unwrap_or_default();
                    let size = builtin_manifest()
                        .get_builtin("mysql", "8.4.10")
                        .map(|e| e.size)
                        .unwrap_or(0);
                    m.push(MirrorSource {
                        name: "i18n:builtinVersion".to_string(),
                        url: "builtin://software/mysql/8.4.10.zip".to_string(),
                        builtin: Some(BuiltinInfo {
                            version: "8.4.10".to_string(),
                            sha256: sha,
                            size: size,
                        }),
                    });
                    m.push(MirrorSource {
                        name: "i18n:mysqlOfficialCdn".to_string(),
                        url: "https://cdn.mysql.com/archives/mysql-8.4/mysql-8.4.10-winx64.zip".to_string(),
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

        // 注：MySQL 本设计 Windows-only（spec 第 770 行明确子目录 mysql-{ver}-winx64）。
        // Unix 版本目录名不同（mysql-{ver}-linux-glibc2.28-x86_64）、二进制名不同（mysqld 无 .exe）、
        // 配置文件名不同（my.cnf vs my.ini），如需 Unix 支持须单独适配 subdir/program/config_file 路径。
        // 因此本 provider 不在 Unix catalog 注册版本。

        CatalogEntry {
            key: "mysql".to_string(),
            name: "MySQL".to_string(),
            description: "关系型数据库".to_string(),
            category: SoftwareCategory::Database,
            icon: "mdi:database".to_string(),
            versions,
            default_version: "8.4.10".to_string(),
        }
    }

    fn post_install(&self, ctx: &InstallContext) -> Result<()> {
        // 解压已剥掉 mysql-{version}-winx64 顶层目录，install_dir 即 MySQL 程序目录根，
        // my.ini 直接放 install_dir，basedir/datadir 指向 install_dir。
        // 兼容旧结构：若仍存在 mysql-{version}-winx64 子目录，则沿用该子目录。
        let mysql_subdir_name = format!("mysql-{}-winx64", ctx.version);
        let mysql_dir = ctx.install_dir().join(&mysql_subdir_name);

        let (my_ini_path, basedir) = if mysql_dir.is_dir() {
            (mysql_dir.join("my.ini"), mysql_dir)
        } else {
            (ctx.install_dir().join("my.ini"), ctx.install_dir().to_path_buf())
        };

        let basedir_forward = basedir.to_string_lossy().replace('\\', "/");
        let mut file = File::create(&my_ini_path)?;
        let content = format!(
            "[mysql]\ndefault-character-set=utf8mb4\n\n[mysqld]\nport=3306\nbasedir={}\ndatadir={}/data\ncharacter-set-server=utf8mb4\ndefault-storage-engine=INNODB\n",
            basedir_forward, basedir_forward
        );
        file.write_all(content.as_bytes())?;
        Ok(())
    }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        let mysql_subdir = format!("mysql-{}-winx64", ctx.version);
        let working_dir = PathBuf::from(&ctx.install_path).join(&mysql_subdir);

        let init_command = StartCommand {
            program: "bin/mysqld.exe".to_string(),
            args: vec![
                "--initialize-insecure".to_string(),
                "--basedir=.".to_string(),
                "--datadir=./data".to_string(),
            ],
            env_vars: std::collections::BTreeMap::new(),
            working_dir: working_dir.clone(),
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: None,
        };

        Ok(StartCommand {
            program: "bin/mysqld.exe".to_string(),
            args: vec![
                "--defaults-file=my.ini".to_string(),
                "--console".to_string(),
            ],
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
        let port = ctx
            .config
            .get("port")
            .and_then(|v| v.as_u64())
            .map(|p| p as u16)
            .unwrap_or(if ctx.port > 0 { ctx.port } else { 3306 });
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
                    default_value: serde_json::json!(3306),
                    section: Some("[mysqld]".to_string()),
                    description_i18n: Some("configField.portDesc".to_string()),
                },
                ConfigField {
                    key: "bind-address".to_string(),
                    label_i18n: "configField.bindAddress".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("0.0.0.0"),
                    section: Some("[mysqld]".to_string()),
                    description_i18n: None,
                },
                ConfigField {
                    key: "max_connections".to_string(),
                    label_i18n: "configField.maxConnections".to_string(),
                    field_type: ConfigFieldType::Number,
                    default_value: serde_json::json!(151),
                    section: Some("[mysqld]".to_string()),
                    description_i18n: None,
                },
                ConfigField {
                    key: "character-set-server".to_string(),
                    label_i18n: "configField.charset".to_string(),
                    field_type: ConfigFieldType::Select {
                        options: vec![
                            "utf8mb4".to_string(),
                            "utf8".to_string(),
                            "latin1".to_string(),
                        ],
                    },
                    default_value: serde_json::json!("utf8mb4"),
                    section: Some("[mysqld]".to_string()),
                    description_i18n: None,
                },
                ConfigField {
                    key: "innodb_buffer_pool_size".to_string(),
                    label_i18n: "configField.innodbBufferPool".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("128M"),
                    section: Some("[mysqld]".to_string()),
                    description_i18n: None,
                },
            ],
        })
    }

    fn config_file_path(&self, ctx: &ConfigContext) -> Option<PathBuf> {
        let mysql_subdir = format!("mysql-{}-winx64", ctx.version);
        Some(PathBuf::from(&ctx.install_path).join(mysql_subdir).join("my.ini"))
    }

    fn working_dir(&self, ctx: &WorkingDirContext) -> PathBuf {
        let mysql_subdir = format!("mysql-{}-winx64", ctx.version);
        PathBuf::from(&ctx.install_path).join(mysql_subdir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_8410_has_builtin() {
        let entry = MySqlProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "8.4.10")
            .expect("应有 8.4.10 版本");
        assert!(v.mirrors[0].builtin.is_some());
        assert_eq!(v.mirrors[0].builtin.as_ref().unwrap().version, "8.4.10");
    }

    #[test]
    fn mysql_fetch_remote_versions_method_exists() {
        let provider = MySqlProvider::new();
        let _ = provider.fetch_remote_versions();
    }

    #[test]
    fn mysql_start_command_uses_defaults_file_and_console() {
        let p = MySqlProvider::new();
        let ctx = super::StartContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/mysql/8.4.10".to_string(),
            version: "8.4.10".to_string(),
            config: serde_json::json!({}),
            custom_start_command: None,
        };
        let cmd = p.start_command(&ctx).unwrap();
        assert!(cmd.program.contains("mysqld.exe"));
        assert!(cmd.args.contains(&"--defaults-file=my.ini".to_string()));
        assert!(cmd.args.contains(&"--console".to_string()));
        assert_eq!(
            cmd.working_dir,
            std::path::PathBuf::from("apps/mysql/8.4.10/mysql-8.4.10-winx64")
        );
        assert_eq!(cmd.creation_flags, CREATE_NO_WINDOW);
    }

    #[test]
    fn mysql_start_command_has_first_run_init_with_initialize_insecure() {
        let p = MySqlProvider::new();
        let ctx = super::StartContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/mysql/8.4.10".to_string(),
            version: "8.4.10".to_string(),
            config: serde_json::json!({}),
            custom_start_command: None,
        };
        let cmd = p.start_command(&ctx).unwrap();
        let fri = cmd.first_run_init.expect("MySQL 应有首次初始化");
        assert!(fri.init_command.args.contains(&"--initialize-insecure".to_string()));
        assert!(fri.init_command.args.contains(&"--basedir=.".to_string()));
        assert!(fri.init_command.args.contains(&"--datadir=./data".to_string()));
        // 防递归：init 命令不应再嵌套 first_run_init
        assert!(fri.init_command.first_run_init.is_none());
        // init 与正式启动共享 working_dir
        assert_eq!(fri.init_command.working_dir, cmd.working_dir);
        // --initialize-insecure 不产生临时密码
        assert!(fri.temp_secret_output.is_none());
        assert!(fri.init_command.env_vars.is_empty());
    }

    #[test]
    fn mysql_health_check_uses_tcp_port_from_config_or_default_3306() {
        let p = MySqlProvider::new();
        let ctx = super::HealthContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/mysql/8.4.10".to_string(),
            port: 0,
            config: serde_json::json!({"port": 3307}),
        };
        match p.health_check(&ctx) {
            crate::models::software::HealthCheckSpec::Tcp { port, .. } => {
                assert_eq!(port, 3307);
            }
            _ => panic!("应为 Tcp"),
        }
    }

    #[test]
    fn mysql_health_check_uses_ctx_port_when_config_missing() {
        let p = MySqlProvider::new();
        let ctx = super::HealthContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/mysql/8.4.10".to_string(),
            port: 3308,
            config: serde_json::json!({}),
        };
        match p.health_check(&ctx) {
            crate::models::software::HealthCheckSpec::Tcp { port, .. } => {
                assert_eq!(port, 3308);
            }
            _ => panic!("应为 Tcp"),
        }
    }

    #[test]
    fn mysql_health_check_falls_back_to_3306_when_config_missing() {
        let p = MySqlProvider::new();
        let ctx = super::HealthContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/mysql/8.4.10".to_string(),
            port: 0,
            config: serde_json::json!({}),
        };
        match p.health_check(&ctx) {
            crate::models::software::HealthCheckSpec::Tcp { port, .. } => {
                assert_eq!(port, 3306);
            }
            _ => panic!("应为 Tcp"),
        }
    }

    #[test]
    fn mysql_config_schema_has_five_fields_in_mysqld_section() {
        let p = MySqlProvider::new();
        let schema = p.config_schema().expect("MySQL 应有 schema");
        assert_eq!(schema.fields.len(), 5);
        assert_eq!(schema.fields[0].key, "port");
        assert_eq!(schema.fields[0].section.as_deref(), Some("[mysqld]"));
    }

    #[test]
    fn mysql_config_file_path_returns_my_ini_in_subdir() {
        let p = MySqlProvider::new();
        let ctx = super::ConfigContext {
            install_path: "apps/mysql/8.4.10".to_string(),
            version: "8.4.10".to_string(),
            config: serde_json::json!({}),
        };
        let path = p.config_file_path(&ctx).expect("应有路径");
        assert!(path.to_string_lossy().contains("mysql-8.4.10-winx64"));
        assert!(path.to_string_lossy().ends_with("my.ini"));
    }

    #[test]
    fn mysql_working_dir_is_install_path_plus_subdir() {
        let p = MySqlProvider::new();
        let ctx = super::WorkingDirContext {
            install_path: "apps/mysql/8.4.10".to_string(),
            version: "8.4.10".to_string(),
        };
        assert_eq!(
            p.working_dir(&ctx),
            std::path::PathBuf::from("apps/mysql/8.4.10/mysql-8.4.10-winx64")
        );
    }
}
