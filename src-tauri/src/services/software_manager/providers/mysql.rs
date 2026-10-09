use anyhow::Result;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use opx_core::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, FieldRule, HealthCheckSpec, LogSource, MirrorSource,
    SoftwareCategory,
};

use super::{
    ConfigContext, FirstRunInit, GracefulStopCommand, HealthContext, InstallContext, LogContext,
    SoftwareProvider, StartCommand, StartContext, StopContext, WorkingDirContext,
    default_log_sources, exe_name,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

/// MySQL 归档包名与格式（P2-1，per-OS，均经官方 CDN HEAD 实测核实）：
/// - Windows `mysql-{v}-winx64.zip`
/// - Linux   `mysql-{v}-linux-glibc2.28-x86_64.tar.xz`（官方 Linux 仅 `.tar.xz`）
/// - macOS   `mysql-{v}-macos15-x86_64.tar.gz`
fn mysql_asset(version: &str, os: &str) -> (String, ArchiveFormat) {
    match os {
        "linux" => (
            format!("mysql-{version}-linux-glibc2.28-x86_64.tar.xz"),
            ArchiveFormat::TarXz,
        ),
        "macos" => (
            format!("mysql-{version}-macos15-x86_64.tar.gz"),
            ArchiveFormat::TarGz,
        ),
        _ => (format!("mysql-{version}-winx64.zip"), ArchiveFormat::Zip),
    }
}

/// 官方 CDN 下载 URL（`cdn.mysql.com/Downloads/MySQL-{major.minor}/{asset}`）。
fn mysql_official_url(version: &str, os: &str) -> String {
    let major_minor = version.split('.').take(2).collect::<Vec<_>>().join(".");
    format!(
        "https://cdn.mysql.com/Downloads/MySQL-{major_minor}/{}",
        mysql_asset(version, os).0
    )
}

/// MySQL 配置文件名：Windows 用 `my.ini`，Unix 用 `my.cnf`。
fn mysql_config_file_name() -> &'static str {
    if opx_core::utils::platform::current_os() == "windows" {
        "my.ini"
    } else {
        "my.cnf"
    }
}

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
        // P2-1：运行时按 OS 选官方包，替代编译期 #[cfg(windows)] 锁（非 Windows 不再空目录）。
        // 注：解压会剥掉顶层目录（mysql-{ver}-{os} 子目录）→ subdir 无需分平台；
        // 二进制名经 exe_name 统一（P2-2）；配置文件名经 mysql_config_file_name 分平台。
        let os = opx_core::utils::platform::current_os();
        const VER: &str = "8.4.11";
        let (asset, format) = mysql_asset(VER, os);
        let mut mirrors = vec![];
        if os == "windows" {
            // 自持源优先（仅 Windows 版已固化到自有 Release）：官方 CDN 国内慢且易断。
            mirrors.push(MirrorSource {
                name: "i18n:selfHosted".to_string(),
                url: format!("{}/{asset}", super::RESOURCE_BASE),
                builtin: None,
            });
        }
        mirrors.push(MirrorSource {
            name: "i18n:official".to_string(),
            url: mysql_official_url(VER, os),
            builtin: None,
        });
        let versions = vec![CatalogVersion {
            version: VER.to_string(),
            mirrors,
            // size/sha256 为 Windows 版实测值；其他平台留空（下载后完整性校验兜底）
            archive: if os == "windows" {
                ArchiveInfo {
                    format,
                    size: Some(281_191_914),
                    sha256: Some(
                        "a492371d687d2bab088b0062581144a0044b8964baefdf4faa579292b423d25c".to_string(),
                    ),
                }
            } else {
                ArchiveInfo { format, size: None, sha256: None }
            },
        }];

        CatalogEntry {
            key: "mysql".to_string(),
            name: "MySQL".to_string(),
            description: "关系型数据库".to_string(),
            description_i18n: Some("catalogDesc.mysql".to_string()),
            category: SoftwareCategory::Database,
            icon: "mdi:database".to_string(),
            versions,
            default_version: VER.to_string(),
        }
    }

    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        // ponytail: 从 config/mysql-versions.json 读取 LTS 版本列表
        let path = opx_core::utils::paths::config_dir().join("mysql-versions.json");
        let defaults = r#"["8.0.41","8.4.11","9.7.2"]"#;
        let content = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            let _ = std::fs::write(&path, defaults);
            defaults.to_string()
        });
        // ponytail: 从配置文件读版本列表
        let versions: Vec<CatalogVersion> = serde_json::from_str::<Vec<String>>(&content).ok().unwrap_or_default()
            .iter().map(|ver| {
                let os = opx_core::utils::platform::current_os();
                let (_, format) = mysql_asset(ver, os);
                CatalogVersion {
                    version: ver.clone(),
                    mirrors: vec![MirrorSource { name: "i18n:official".to_string(), url: mysql_official_url(ver, os), builtin: None }],
                    archive: ArchiveInfo { format, size: None, sha256: None },
                }
            }).collect();
        Some(versions)
    }

    fn post_install(&self, ctx: &InstallContext) -> Result<()> {
        // extract_* 已剥掉 mysql-{ver}-{os} 顶层目录，install_dir 即 MySQL 程序目录根，
        // 配置文件（win my.ini / *nix my.cnf）直接放 install_dir
        let my_ini_path = ctx.install_dir().join(mysql_config_file_name());

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
            program: format!("bin/{}", exe_name("mysqld")),
            args: vec![
                "--initialize-insecure".to_string(),
                "--basedir=".to_string() + &ctx.install_path,
                "--datadir=".to_string() + &data_dir.to_string_lossy(),
                "--log-error=".to_string() + &data_dir.join("mysql-init.err").to_string_lossy(),
            ],
            env_vars: std::collections::BTreeMap::new(),
            working_dir: working_dir.clone(),
            remove_envs: Vec::new(),
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
            format!("--defaults-file={}", mysql_config_file_name()),
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
            program: format!("bin/{}", exe_name("mysqld")),
            args: main_args,
            env_vars: std::collections::BTreeMap::new(),
            working_dir,
            remove_envs: Vec::new(),
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: Some(Box::new(FirstRunInit {
                init_command,
            })),
        })
    }

    /// P1-3 补完：MySQL 此前无优雅停止（root 密码 ephemeral → `mysqladmin shutdown` 无法认证），
    /// 停止即被 `taskkill /F` 强杀，有 InnoDB 未刷盘/数据损坏风险。
    ///
    /// 现凭据已随 P2-6 统一落盘（收紧权限），故可从 config 读回密码，经 **`MYSQL_PWD` 环境变量**
    /// 传给 `mysqladmin shutdown`（不落在命令行，避免被同机其他用户从进程列表读到），
    /// 让其干净关闭（刷盘、正常下线）。
    ///
    /// 未初始化或密码缺失（老实例，密码未落盘）→ 返回 `None`，走既有强杀回退，**行为不变**。
    fn graceful_stop_command(&self, ctx: &StopContext) -> Option<GracefulStopCommand> {
        let password = ctx
            .config
            .get("init_password")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if password.is_empty() {
            return None;
        }
        let port = ctx
            .config
            .get("port")
            .and_then(|v| v.as_u64())
            .map(|p| p as u16)
            .unwrap_or(if ctx.port > 0 { ctx.port } else { 3306 });
        let mut env_vars = std::collections::BTreeMap::new();
        env_vars.insert("MYSQL_PWD".to_string(), password.to_string());
        Some(GracefulStopCommand {
            program: PathBuf::from(&ctx.install_path)
                .join("bin")
                .join(exe_name("mysqladmin"))
                .to_string_lossy()
                .to_string(),
            args: vec![
                "-h".to_string(),
                "127.0.0.1".to_string(),
                "-P".to_string(),
                port.to_string(),
                "-u".to_string(),
                "root".to_string(),
                "shutdown".to_string(),
            ],
            working_dir: PathBuf::from(&ctx.install_path),
            env_vars,
            timeout_secs: 30,
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
                        labels: vec![],
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
            // init_password 改为**持久化**（经 `write_file_restricted` 收紧权限，同 P2-6 的 secret 约定）：
            // 优雅停止需 `mysqladmin shutdown` 认证，故不再 ephemeral；
            // 该字段在「已初始化」实例上由前端按 last_started_at 锁定禁用（不改动）。
            ephemeral_keys: vec![],
            // 初始化凭据：首启前必填（避免无密码 root 弱口令），首启后由前端按 last_started_at 锁定禁用
            field_rules: vec![FieldRule {
                field_key: "init_password".to_string(),
                visible_when: None,
                required: true,
            }],
        })
    }

    fn log_sources(&self, ctx: &LogContext) -> Vec<LogSource> {
        // C 扩展：stdout 为结构化级别日志，启用级别筛选下拉
        let mut sources = default_log_sources(ctx);
        for s in &mut sources {
            s.has_levels = true;
        }
        sources
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> {
        // 仅返回相对文件名，调用方（write_config_form / read_config_source）
        // 会自行拼接 install_path，避免双路径拼接 bug。
        Some(PathBuf::from(mysql_config_file_name()))
    }

    fn working_dir(&self, ctx: &WorkingDirContext) -> PathBuf {
        PathBuf::from(&ctx.install_path)
    }
}
