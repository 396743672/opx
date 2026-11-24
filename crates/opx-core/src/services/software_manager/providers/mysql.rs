use anyhow::Result;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::models::software::{
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

/// 定位 MySQL 程序 bin 目录（回归：`os error 2`）。
///
/// 背景：Windows 的 zip 经 `extract_zip_flatten` 剥掉顶层目录，`bin/mysqld.exe`
/// 直接在安装根；但 Linux/macOS 的 tar.xz/tar.gz **保留顶层目录**
/// （`mysql-8.4.11-linux-glibc2.28-x86_64/bin/mysqld`），若仍按安装根拼接
/// `bin/mysqld` 就会触发 `启动失败：没有那个文件或目录 (os error 2)`。
///
/// 策略（按优先级）：
/// 1. 安装根下 `bin/` 存在 → 直接返回（Windows / 已平铺的存量安装）；
/// 2. 预期顶层目录 `<asset 去扩展名>/bin/` 存在 → 返回；
/// 3. 扫描安装根下唯一含 `bin/` 的子目录兜底（兼容自定义包名）。
fn mysql_bin_dir(install_path: &std::path::Path, version: &str, os: &str) -> PathBuf {
    let direct = install_path.join("bin");
    if direct.is_dir() {
        return direct;
    }
    let (asset, _) = mysql_asset(version, os);
    let top = asset
        .trim_end_matches(".tar.xz")
        .trim_end_matches(".tar.gz")
        .trim_end_matches(".zip");
    let nested = install_path.join(top).join("bin");
    if nested.is_dir() {
        return nested;
    }
    if let Ok(entries) = std::fs::read_dir(install_path) {
        for entry in entries.flatten() {
            let candidate = entry.path().join("bin");
            if candidate.is_dir() {
                return candidate;
            }
        }
    }
    // 都不存在：返回预期路径，让启动报错依然指向明确位置（而非回退到错误的根 bin）
    nested
}

/// `--basedir` 应指向程序根（bin 的父目录）：errmsg.sys / lib/private 等资源都在该目录下，
/// 指到 install_path（含 data/ 与 my.cnf 的外层）会导致启动报 "Can't find error-message file"。
fn mysql_basedir(install_path: &std::path::Path, version: &str, os: &str) -> PathBuf {
    mysql_bin_dir(install_path, version, os)
        .parent()
        .filter(|p| p != &install_path)
        .unwrap_or(install_path)
        .to_path_buf()
}

/// 返回相对 `install_path` 的程序路径（供 `StartCommand.program` 使用，配合 working_dir）。
fn mysql_program(install_path: &std::path::Path, version: &str, os: &str, base: &str) -> String {
    let bin = mysql_bin_dir(install_path, version, os);
    let rel = bin
        .strip_prefix(install_path)
        .unwrap_or(std::path::Path::new("bin"));
    rel.join(exe_name(base))
        .to_string_lossy()
        .replace('\\', "/")
}

/// MySQL 配置文件名：Windows 用 `my.ini`，Unix 用 `my.cnf`。
/// MySQL 8.0+ 的 sql_mode。
///
/// NO_ZERO_DATE / NO_ZERO_IN_DATE / ERROR_FOR_DIVISION_BY_ZERO 自 8.0 起标记
/// 废弃，官方要求它们与 strict mode 同时启用或同时不启用，否则启动即打印
/// MY-010915 警告；9.x 仍接受这三者（同样警告），计划在后续版本并入 strict。
/// 因此这里显式带上三者 + STRICT_TRANS_TABLES，8.x/9.x 都不报警告。
const MYSQL_SQL_MODE: &str =
    "STRICT_TRANS_TABLES,NO_ENGINE_SUBSTITUTION,NO_ZERO_DATE,NO_ZERO_IN_DATE,ERROR_FOR_DIVISION_BY_ZERO";

fn mysql_config_file_name() -> &'static str {
    if crate::utils::platform::current_os() == "windows" {
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
        let os = crate::utils::platform::current_os();
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
        let path = crate::utils::paths::config_dir().join("mysql-versions.json");
        let defaults = r#"["8.0.41","8.4.11","9.7.2"]"#;
        let content = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            let _ = std::fs::write(&path, defaults);
            defaults.to_string()
        });
        // ponytail: 从配置文件读版本列表
        let versions: Vec<CatalogVersion> = serde_json::from_str::<Vec<String>>(&content).ok().unwrap_or_default()
            .iter().map(|ver| {
                let os = crate::utils::platform::current_os();
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
        let (flush_method, lower_case) =
            mysql_innodb_platform_settings(&crate::utils::platform::current_os());

        let mut file = File::create(&my_ini_path)?;
        // 监听地址取「系统配置 → 对外访问地址」（默认本机首选网卡 IP）。
        // 早前这里硬编码 127.0.0.1：安装完只能本机连，外部一律 10061，
        // 而界面上又没有这个字段 —— 用户只能手改 my.cnf。
        let bind = crate::services::software_manager::netutils::default_service_bind();
        let content = format!(
            "[mysql]\ndefault-character-set=utf8mb4\n\n[mysqld]\n\
port=3306\n\
bind-address={bind}\n\
character-set-server=utf8mb4\ncollation-server=utf8mb4_unicode_ci\n\
default-storage-engine=INNODB\n\
sql_mode={sql_mode}\n\
max_connections=151\n\
thread_cache_size={thread_cache}\n\
innodb_buffer_pool_size={buffer_pool}M\n\
innodb_log_buffer_size=64M\n\
innodb_flush_log_at_trx_commit=1\n\
innodb_lock_wait_timeout=50\n\
innodb_read_io_threads={io_threads}\n\
innodb_write_io_threads={io_threads}\n\
innodb_flush_method={flush_method}\n\
innodb_doublewrite=1\n\
lower_case_table_names={lower_case}\n",
            bind = bind,
            sql_mode = MYSQL_SQL_MODE,
            thread_cache = thread_cache,
            buffer_pool = buffer_pool_mb,
            io_threads = io_threads,
            flush_method = flush_method,
            lower_case = lower_case,
        );
        file.write_all(content.as_bytes())?;
        Ok(())
    }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        // 程序 bin 目录按 OS 定位：Windows zip 已 flatten（install_path/bin），
        // Linux/macOS tar 保留顶层目录（install_path/mysql-{ver}-{os}/bin）。
        let os = crate::utils::platform::current_os();
        let working_dir = PathBuf::from(&ctx.install_path);
        let data_dir = working_dir.join("data");
        let mysqld_program = mysql_program(&working_dir, &ctx.version, &os, "mysqld");
        let basedir = mysql_basedir(&working_dir, &ctx.version, &os);
        let run_as = mysql_run_as_user_arg();

        let mut init_args = vec![
            "--initialize-insecure".to_string(),
            "--basedir=".to_string() + &basedir.to_string_lossy(),
            "--datadir=".to_string() + &data_dir.to_string_lossy(),
            "--log-error=".to_string() + &data_dir.join("mysql-init.err").to_string_lossy(),
        ];
        if let Some(user_arg) = &run_as {
            init_args.push(user_arg.clone());
        }

        let init_command = StartCommand {
            program: mysqld_program.clone(),
            args: init_args,
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
            "--basedir=".to_string() + &basedir.to_string_lossy(),
            "--datadir=".to_string() + &working_dir.join("data").to_string_lossy(),
            "--console".to_string(),
        ];
        if let Some(user_arg) = &run_as {
            main_args.push(user_arg.clone());
        }

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
            program: mysqld_program,
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
        // 与 start_command 同：Linux/macOS 的 mysqladmin 在保留的顶层目录 bin/ 下
        let os = crate::utils::platform::current_os();
        let bin = mysql_bin_dir(Path::new(&ctx.install_path), &ctx.version, &os);
        // 连接地址跟随实际 bind：只绑 192.168.200.128 时连 127.0.0.1 会被拒，
        // 优雅停止会退化成强杀（有 InnoDB 未刷盘风险）。
        let host = crate::services::software_manager::netutils::bind_from_config(&ctx.config)
            .map(|b| crate::services::software_manager::netutils::probe_host_for_bind(&b))
            .unwrap_or_else(|| "127.0.0.1".to_string());
        Some(GracefulStopCommand {
            program: bin.join(exe_name("mysqladmin")).to_string_lossy().to_string(),
            args: vec![
                "-h".to_string(),
                host,
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

    /// 与 `post_install` 写进 my.cnf 的 `bind-address` 保持一致。
    fn install_defaults(&self) -> serde_json::Value {
        serde_json::json!({
            "bind-address": crate::services::software_manager::netutils::default_service_bind()
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
                // 监听地址必须可配：默认只绑本机时外部一律连不上（错误码 10061），
                // 而这一项以前只写在生成的 my.cnf 里、界面上完全看不到。
                // 默认值取系统配置的对外访问地址（不是写死的 127.0.0.1）。
                ConfigField {
                    key: "bind-address".to_string(),
                    label_i18n: "configField.bindAddress".to_string(),
                    field_type: ConfigFieldType::BindAddress,
                    default_value: serde_json::json!(
                        crate::services::software_manager::netutils::default_service_bind()
                    ),
                    section: Some("[mysqld]".to_string()),
                    description_i18n: Some("configField.bindAddressDesc".to_string()),
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

/// Unix 下 mysqld 拒绝以 root 直接运行（`Fatal error: Please read "Security" section ...
/// run mysqld as root`），必须显式 `--user=<当前用户>`；root 环境显式指定即被接受。
/// 返回 None 表示无需添加（Windows）或无法获取用户名（保持原行为）。
fn mysql_run_as_user_arg() -> Option<String> {
    if crate::utils::platform::current_os() == "windows" {
        return None;
    }
    std::process::Command::new("whoami")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|u| format!("--user={u}"))
}

/// InnoDB 平台相关设置（回归：`innodb_flush_method=normal` 是 Windows 专用值，
/// Linux 启动报 `Error while setting value 'normal' to 'innodb-flush-method'`；
/// `lower_case_table_names=1` 也是 Windows 默认，Unix 应为 0）。
fn mysql_innodb_platform_settings(os: &str) -> (&'static str, &'static str) {
    if os == "windows" {
        ("normal", "1")
    } else {
        ("fsync", "0")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir()
            .join(format!("opx_mysql_bin_{}_{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    /// 8.0+ 官方要求：三个废弃 mode 与 strict 必须同时启用/同时不启用，
    /// 只写 STRICT_TRANS_TABLES 会在启动时报 MY-010915（实测确认）。
    #[test]
    fn sql_mode_includes_deprecated_modes_alongside_strict() {
        assert!(MYSQL_SQL_MODE.contains("STRICT_TRANS_TABLES"));
        for mode in ["NO_ZERO_DATE", "NO_ZERO_IN_DATE", "ERROR_FOR_DIVISION_BY_ZERO"] {
            assert!(MYSQL_SQL_MODE.contains(mode), "{mode} 缺失会触发 MY-010915 警告");
        }
    }

    #[test]
    fn bin_dir_uses_direct_bin_when_flattened() {
        // Windows zip / 手动平铺的存量安装：bin 直接在安装根
        let root = temp_root("flat");
        fs::create_dir_all(root.join("bin")).unwrap();
        let dir = mysql_bin_dir(&root, "8.4.11", "windows");
        assert_eq!(dir, root.join("bin"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bin_dir_finds_nested_top_level_dir_on_linux() {
        // 回归：官方 tar.xz 保留顶层目录，mysqld 不在安装根 bin/
        let root = temp_root("nested");
        let nested = root.join("mysql-8.4.11-linux-glibc2.28-x86_64").join("bin");
        fs::create_dir_all(&nested).unwrap();
        let dir = mysql_bin_dir(&root, "8.4.11", "linux");
        assert_eq!(dir, nested);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bin_dir_falls_back_to_scanning_single_subdir() {
        // 顶层目录名与预期不符时（如自定义包），扫描含 bin/ 的子目录兜底
        let root = temp_root("scan");
        let nested = root.join("custom-layout").join("bin");
        fs::create_dir_all(&nested).unwrap();
        let dir = mysql_bin_dir(&root, "8.4.11", "linux");
        assert_eq!(dir, nested);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn innodb_platform_settings_are_os_specific() {
        assert_eq!(mysql_innodb_platform_settings("windows"), ("normal", "1"));
        // 回归：Linux 不接受 normal，lower_case 也须为 0
        assert_eq!(mysql_innodb_platform_settings("linux"), ("fsync", "0"));
        assert_eq!(mysql_innodb_platform_settings("macos"), ("fsync", "0"));
    }

    #[test]
    fn basedir_is_program_root_not_install_root() {
        // 回归：errmsg.sys / lib/private 在顶层子目录，basedir 指错会导致主进程启动即退出
        let root = temp_root("basedir");
        let nested = root.join("mysql-8.4.11-linux-glibc2.28-x86_64").join("bin");
        fs::create_dir_all(&nested).unwrap();
        let basedir = mysql_basedir(&root, "8.4.11", "linux");
        assert_eq!(basedir, root.join("mysql-8.4.11-linux-glibc2.28-x86_64"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn program_is_relative_to_install_root() {
        let root = temp_root("program");
        let nested = root.join("mysql-8.4.11-linux-glibc2.28-x86_64").join("bin");
        fs::create_dir_all(&nested).unwrap();
        let prog = mysql_program(&root, "8.4.11", "linux", "mysqld");
        assert_eq!(
            prog,
            format!("mysql-8.4.11-linux-glibc2.28-x86_64/bin/{}", exe_name("mysqld"))
        );
        let _ = fs::remove_dir_all(&root);
    }
}
