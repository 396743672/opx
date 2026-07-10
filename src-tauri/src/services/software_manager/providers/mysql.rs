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

/// 获取系统内存（MB）和 CPU 核数，用于动态生成 MySQL 配置
/// 失败时回退到保守默认值（4GB 内存 / 4 核）
fn get_system_info() -> (u64, usize) {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    // sysinfo 0.31: total_memory() 返回 KB
    let total_mem_mb = sys.total_memory() / 1024 / 1024;
    let cpu_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let mem = if total_mem_mb > 0 { total_mem_mb } else { 4096 };
    let cpu = if cpu_count > 0 { cpu_count } else { 4 };
    (mem, cpu)
}

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
        // extract_zip_flatten 已剥掉 mysql-{version}-winx64 顶层目录，
        // install_dir 即 MySQL 程序目录根，my.ini 直接放 install_dir
        let my_ini_path = ctx.install_dir().join("my.ini");
        let basedir = ctx.install_dir().to_path_buf();

        let basedir_forward = basedir.to_string_lossy().replace('\\', "/");

        // 根据系统内存/CPU 动态生成配置
        let (total_mem_mb, cpu_count) = get_system_info();
        // innodb_buffer_pool_size: 系统内存的 50%，限制在 128M~4G
        let buffer_pool_mb = (total_mem_mb / 2).max(128).min(4096);
        // thread_cache_size: CPU 核数 * 4
        let thread_cache = cpu_count * 4;
        // io_threads: CPU 核数（最少 4），用于 innodb_read_io_threads / innodb_write_io_threads
        let io_threads = cpu_count.max(4);

        let mut file = File::create(&my_ini_path)?;
        let content = format!(
            "[mysql]\ndefault-character-set=utf8mb4\n\n[mysqld]\n\
port=3306\n\
bind-address=127.0.0.1\n\
basedir=\"{basedir}\"\n\
datadir=\"{basedir}/data\"\n\
socket=\"{basedir}/mysql.sock\"\n\
character-set-server=utf8mb4\ncollation-server=utf8mb4_unicode_ci\n\
default-storage-engine=INNODB\n\
sql_mode=NO_ENGINE_SUBSTITUTION,STRICT_TRANS_TABLES\n\
max_connections=151\n\
thread_cache_size={thread_cache}\n\
innodb_buffer_pool_size={buffer_pool}M\n\
innodb_log_file_size=256M\n\
innodb_log_buffer_size=64M\n\
innodb_flush_log_at_trx_commit=1\n\
innodb_lock_wait_timeout=50\n\
innodb_read_io_threads={io_threads}\n\
innodb_write_io_threads={io_threads}\n\
innodb_flush_method=normal\n\
innodb_doublewrite=1\n\
lower_case_table_names=1\n",
            basedir = basedir_forward,
            thread_cache = thread_cache,
            buffer_pool = buffer_pool_mb,
            io_threads = io_threads,
        );
        file.write_all(content.as_bytes())?;
        Ok(())
    }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        // extract_zip_flatten 已剥掉 zip 顶层目录，install_path 即 MySQL 程序目录根
        // bin/mysqld.exe 直接在 install_path/bin/ 下
        let working_dir = PathBuf::from(&ctx.install_path);
        // 绝对路径化：避免依赖子进程 CWD 解析相对路径（曾导致 .\data\mysql-init.err
        // 创建时被拒 "Permission denied"）。F2 的 --log-error 诊断落盘路径改为绝对路径。
        let data_dir = PathBuf::from(&ctx.install_path).join("data");

        let init_command = StartCommand {
            program: "bin/mysqld.exe".to_string(),
            args: vec![
                "--initialize-insecure".to_string(),
                "--basedir=".to_string() + &ctx.install_path,
                "--datadir=".to_string() + &data_dir.to_string_lossy(),
                "--log-error=".to_string() + &data_dir.join("mysql-init.err").to_string_lossy(),
            ],
            env_vars: std::collections::BTreeMap::new(),
            working_dir: working_dir.clone(),
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: None,
        };

        // 首次初始化密码注入（方案 B）：
        // - 仅在「未初始化」且用户提供了 init_password 时生效；
        // - 方式：把 `ALTER USER 'root'@'localhost' IDENTIFIED BY '...';` 写入安装根目录下的
        //   临时 SQL 文件（绝对路径，避免相对路径 Permission denied；且放 install_path 根目录
        //   而非 data/，防止被 first_run_init 的 wipe_data_dir_if_nonempty 误删），
        //   并在「首次正常启动」命令上挂 --init-file=<绝对路径>。
        //   注意：--init-file 在 --initialize-insecure（bootstrap 模式）下受限，账户管理语句
        //   （ALTER USER）不会执行；因此挂在首次「正常启动」命令上，启动后由 lifecycle 删除文件。
        // - 已初始化（initialized == true）时不注入：密码仅消费一次，且防御性清理可能残留的文件。
        let initialized = ctx
            .config
            .get("initialized")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut main_args = vec![
            "--defaults-file=my.ini".to_string(),
            "--console".to_string(),
        ];

        if initialized {
            // 防御性清理：已初始化不应再有初始化密码临时文件，删除可能残留的明文文件
            let stale = PathBuf::from(&ctx.install_path).join(".mysql-init-password.sql");
            let _ = std::fs::remove_file(&stale);
        } else if let Some(pw) = &ctx.init_password {
            if !pw.is_empty() {
                // 放 install_path 根目录（非 data/），避免被 wipe_data_dir_if_nonempty 误删
                let init_sql_path =
                    PathBuf::from(&ctx.install_path).join(".mysql-init-password.sql");
                // SQL 字符串转义：先转义反斜杠，再转义单引号（MySQL 字符串字面量规则）
                let mut escaped = pw.replace('\\', "\\\\");
                escaped = escaped.replace('\'', "\\'");
                let sql = format!(
                    "ALTER USER 'root'@'localhost' IDENTIFIED BY '{}';\n",
                    escaped
                );
                std::fs::write(&init_sql_path, sql)?;
                main_args.push(format!("--init-file={}", init_sql_path.to_string_lossy()));
            }
        }

        Ok(StartCommand {
            program: "bin/mysqld.exe".to_string(),
            args: main_args,
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
                    field_type: ConfigFieldType::Size {
                        units: vec!["M".to_string(), "G".to_string()],
                    },
                    default_value: serde_json::json!("128M"),
                    section: Some("[mysqld]".to_string()),
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
            // init_password 为一次性敏感字段：仅首次初始化消费，绝不写入 my.ini / installed.json
            ephemeral_keys: vec!["init_password".to_string()],
        })
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> {
        // 仅返回相对文件名，调用方（write_config_form / read_config_source）
        // 会自行拼接 install_path，避免双路径拼接 bug。
        Some(PathBuf::from("my.ini"))
    }

    fn working_dir(&self, ctx: &WorkingDirContext) -> PathBuf {
        PathBuf::from(&ctx.install_path)
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
            init_password: None,
        };
        let cmd = p.start_command(&ctx).unwrap();
        assert!(cmd.program.contains("mysqld.exe"));
        assert!(cmd.args.contains(&"--defaults-file=my.ini".to_string()));
        assert!(cmd.args.contains(&"--console".to_string()));
        assert_eq!(
            cmd.working_dir,
            std::path::PathBuf::from("apps/mysql/8.4.10")
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
            init_password: None,
        };
        let cmd = p.start_command(&ctx).unwrap();
        let fri = cmd.first_run_init.expect("MySQL 应有首次初始化");
        assert!(fri.init_command.args.contains(&"--initialize-insecure".to_string()));
        // B：--basedir / --datadir 改为基于 install_path 的绝对路径（不再用相对 ./data）
        // 使用平台感知的路径分隔符（Windows \、Unix /）
        let sep = std::path::MAIN_SEPARATOR_STR;
        let expected_datadir = format!("apps/mysql/8.4.10{}data", sep);
        let expected_logerror = format!("apps/mysql/8.4.10{}data{}mysql-init.err", sep, sep);
        assert!(fri.init_command.args.contains(&"--basedir=apps/mysql/8.4.10".to_string()));
        assert!(fri.init_command.args.contains(&format!("--datadir={}", expected_datadir)));
        // F2/B：--log-error 现在为绝对路径，且全 args 中仅一份
        assert!(fri.init_command.args.contains(&format!("--log-error={}", expected_logerror)));
        let log_error_count = fri
            .init_command
            .args
            .iter()
            .filter(|a| a.starts_with("--log-error="))
            .count();
        assert_eq!(log_error_count, 1, "init args 中应仅有一份 --log-error");
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
    fn mysql_config_schema_has_five_fields_and_init_password_is_ephemeral() {
        let p = MySqlProvider::new();
        let schema = p.config_schema().expect("MySQL 应有 schema");
        // 4 个持久化字段 + 1 个 ephemeral 的 init_password
        assert_eq!(schema.fields.len(), 5);
        assert_eq!(schema.fields[0].key, "port");
        assert_eq!(schema.fields[0].section.as_deref(), Some("[mysqld]"));
        // init_password 为最后一个字段，且被标记为 ephemeral
        assert_eq!(schema.fields[4].key, "init_password");
        assert_eq!(schema.fields[4].field_type, ConfigFieldType::Password);
        assert!(schema.ephemeral_keys.contains(&"init_password".to_string()));
    }

    #[test]
    fn mysql_start_command_injects_init_file_when_password_provided_and_not_initialized() {
        let p = MySqlProvider::new();
        let install_path = "apps/mysql/8.4.10";
        // 确保安装目录存在（start_command 会在其中写临时 SQL 文件）
        std::fs::create_dir_all(install_path).unwrap();
        let ctx = super::StartContext {
            installed_id: "uuid".to_string(),
            install_path: install_path.to_string(),
            version: "8.4.10".to_string(),
            config: serde_json::json!({}),
            custom_start_command: None,
            init_password: Some("S3cret!Pass".to_string()),
        };
        let cmd = p.start_command(&ctx).unwrap();
        // 主启动命令应挂 --init-file=<绝对路径>
        let init_file_arg = cmd
            .args
            .iter()
            .find(|a| a.starts_with("--init-file="))
            .expect("应注入 --init-file");
        assert!(
            init_file_arg.ends_with(".mysql-init-password.sql"),
            "init-file 路径应为安装根目录下的临时 SQL 文件，实际：{}",
            init_file_arg
        );
        assert!(
            !init_file_arg.contains("/data/") && !init_file_arg.contains("\\data\\"),
            "init-file 不应放在 data/ 下（避免被 wipe_data_dir 误删），实际：{}",
            init_file_arg
        );
    }

    #[test]
    fn mysql_start_command_skips_init_file_when_initialized() {
        let p = MySqlProvider::new();
        let ctx = super::StartContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/mysql/8.4.10".to_string(),
            version: "8.4.10".to_string(),
            config: serde_json::json!({"initialized": true}),
            custom_start_command: None,
            init_password: Some("S3cret!Pass".to_string()),
        };
        let cmd = p.start_command(&ctx).unwrap();
        assert!(
            !cmd.args.iter().any(|a| a.starts_with("--init-file=")),
            "已初始化后不应再注入 --init-file"
        );
    }

    #[test]
    fn mysql_start_command_skips_init_file_when_password_empty() {
        let p = MySqlProvider::new();
        let ctx = super::StartContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/mysql/8.4.10".to_string(),
            version: "8.4.10".to_string(),
            config: serde_json::json!({}),
            custom_start_command: None,
            init_password: Some("".to_string()),
        };
        let cmd = p.start_command(&ctx).unwrap();
        assert!(
            !cmd.args.iter().any(|a| a.starts_with("--init-file=")),
            "空密码不应注入 --init-file（维持空密码行为）"
        );
    }

    #[test]
    fn mysql_config_file_path_returns_my_ini_in_install_path() {
        let p = MySqlProvider::new();
        let ctx = super::ConfigContext {
            install_path: "apps/mysql/8.4.10".to_string(),
            version: "8.4.10".to_string(),
            config: serde_json::json!({}),
        };
        let path = p.config_file_path(&ctx).expect("应有路径");
        assert!(!path.to_string_lossy().contains("mysql-8.4.10-winx64"));
        assert!(path.to_string_lossy().ends_with("my.ini"));
    }

    #[test]
    fn mysql_working_dir_is_install_path() {
        let p = MySqlProvider::new();
        let ctx = super::WorkingDirContext {
            install_path: "apps/mysql/8.4.10".to_string(),
            version: "8.4.10".to_string(),
        };
        assert_eq!(
            p.working_dir(&ctx),
            std::path::PathBuf::from("apps/mysql/8.4.10")
        );
    }
}
