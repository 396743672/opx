use anyhow::Result;
use std::fs::File;
use std::io::Write;
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
                mirrors: vec![
                    MirrorSource {
                        name: "i18n:official".to_string(),
                        url: "https://cdn.mysql.com/archives/mysql-8.4/mysql-8.4.10-winx64.zip".to_string(),
                        builtin: None,
                    },
                ],
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
            description_i18n: Some("catalogDesc.mysql".to_string()),
            category: SoftwareCategory::Database,
            icon: "mdi:database".to_string(),
            versions,
            default_version: "8.4.10".to_string(),
        }
    }

    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        // ponytail: 从 config/mysql-versions.json 读取 LTS 版本列表
        let path = crate::utils::paths::config_dir().join("mysql-versions.json");
        let defaults = r#"["8.0.41","8.4.11","9.7.2"]"#;
        let content = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            let _ = std::fs::write(&path, defaults);
            defaults.to_string()
        });
        let versions: Vec<CatalogVersion> = serde_json::from_str::<Vec<String>>(&content).ok()?
            .iter().map(|ver| {
                let major_minor = ver.split('.').take(2).collect::<Vec<_>>().join(".");
                let dl = format!("https://cdn.mysql.com/archives/mysql-{}/mysql-{}-winx64.zip", major_minor, ver);
                CatalogVersion {
                    version: ver.clone(),
                    mirrors: vec![MirrorSource { name: "i18n:official".to_string(), url: dl, builtin: None }],
                    archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
                }
            }).collect();
        if versions.is_empty() { None } else { Some(versions) }
    }

    fn post_install(&self, ctx: &InstallContext) -> Result<()> {
        // extract_zip_flatten 已剥掉 mysql-{version}-winx64 顶层目录，
        // install_dir 即 MySQL 程序目录根，my.ini 直接放 install_dir
        let my_ini_path = ctx.install_dir().join("my.ini");

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
        let data_dir = working_dir.join("data");

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
            "--basedir=".to_string() + &ctx.install_path,
            "--datadir=".to_string() + &working_dir.join("data").to_string_lossy(),
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
                // init-file 用绝对路径（清理代码在 OPX 进程侧执行，非 MySQL 子进程 CWD）
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
