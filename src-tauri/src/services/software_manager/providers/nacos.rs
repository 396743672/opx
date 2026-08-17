use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, LogSource, MirrorSource, SoftwareCategory,
};

use super::{
    ConfigContext, FirstRunInit, HealthContext, InstallContext, LogContext, SoftwareProvider,
    StartCommand, StartContext, default_log_sources,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

fn config_str(c: &serde_json::Value, key: &str, default: &str) -> String {
    c.get(key).and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| default.to_string())
}
fn config_u64(c: &serde_json::Value, key: &str, default: u64) -> u64 {
    c.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

pub struct NacosProvider;

impl NacosProvider { pub fn new() -> Self { Self } }
    // C 扩展：stdout 为结构化级别日志，启用级别筛选下拉
    fn log_sources(&self, ctx: &LogContext) -> Vec<LogSource> {
        let mut sources = default_log_sources(ctx);
        for s in &mut sources {
            s.has_levels = true;
        }
        sources
    }
}

impl Default for NacosProvider { fn default() -> Self { Self::new() } }

impl SoftwareProvider for NacosProvider {
    fn key(&self) -> &str { "nacos" }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];
        #[cfg(windows)]
        {
            // 执行时核实：alibaba/nacos GitHub Releases 下载 URL 形如
            // https://github.com/alibaba/nacos/releases/download/<ver>/nacos-server-<ver>.zip
            // 2.x 需 JDK 8+，3.x 需 JDK 17+，两者均内置在版本列表中按需选装。
            for (ver, _jdk_hint) in [
                ("2.5.3", "JDK8+"),
                ("3.2.3", "JDK17+"),
            ] {
                versions.push(CatalogVersion {
                    version: ver.to_string(),
                    mirrors: vec![MirrorSource {
                        name: "i18n:nacosOfficial".to_string(),
                        url: format!("https://github.com/alibaba/nacos/releases/download/{}/nacos-server-{}.zip", ver, ver),
                        builtin: None,
                    }],
                    archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
                });
            }
        }
        CatalogEntry {
            key: "nacos".to_string(),
            name: "Nacos".to_string(),
            description: "服务注册与配置中心".to_string(),
            description_i18n: Some("catalogDesc.nacos".to_string()),
            category: SoftwareCategory::Registry,
            icon: "mdi:hexagon-multiple".to_string(),
            versions,
            default_version: "2.5.3".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> { Ok(()) }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        // Nacos 是 Java 应用：需要已安装 JDK，java -jar 前台启动。
        // startup.cmd/startup.sh 会 daemon 化（后台运行）破坏 PID 管理，故直连 java。
        // jdk_install_path 由命令层解析（优先表单选择的 JDK，回退自动找），此处直接用。
        let jdk = ctx.jdk_install_path.as_deref()
            .ok_or_else(|| anyhow::anyhow!("请先安装 JDK/JRE 并在配置中选择后再启动 Nacos"))?;
        let java = std::path::Path::new(&jdk)
            .join("bin")
            .join(if cfg!(windows) { "java.exe" } else { "java" });
        let jar = std::path::Path::new(&ctx.install_path)
            .join("target").join("nacos-server.jar");
        let working_dir = PathBuf::from(&ctx.install_path);

        // 部署模式：standalone（默认）/ cluster（预留，暂未实现完整集群）
        let mode = config_str(&ctx.config, "mode", "standalone");
        if mode != "standalone" {
            return Err(anyhow::anyhow!("集群模式暂未支持，请使用单机模式（standalone）"));
        }

        let mut args = vec![
            // nacos.home 指定安装根目录（conf/、data/、logs/ 都在其下）。
            // 不设则默认用户主目录 ~/nacos，导致找不到 conf 启动失败。
            format!("-Dnacos.home={}", working_dir.to_string_lossy()),
            // loader.path 加载 plugins/ 下的数据源插件（Derby/MySQL 驱动等）。
            // 不设则 Derby 驱动类找不到，standalone 模式启动失败。
            format!("-Dloader.path={}", working_dir.join("plugins").to_string_lossy()),
            "-Dnacos.standalone=true".to_string(),
            "-jar".to_string(),
            jar.to_string_lossy().to_string(),
        ];

        // 服务端口：3.x 主 API 端口（默认 8848）
        let server_port = config_u64(&ctx.config, "port", 8848);
        if server_port != 8848 {
            args.insert(0, format!("-Dnacos.server.main.port={}", server_port));
        }
        // 控制台端口：3.x 独立（默认 8080）；2.x 与主端口共用，此参数被忽略（无害）
        let console_port = config_u64(&ctx.config, "console_port", 8080);
        if console_port != 8080 {
            args.insert(0, format!("-Dnacos.console.port={}", console_port));
        }

        // JDK 9+ 强封装：Nacos 的 JRaft 用反射访问 JDK 内部字段，必须 --add-opens
        // （与 startup.cmd 的 NACOS_JVM_OPTS 一致），否则 JDK 16+ 启动报 InaccessibleObjectException。
        let add_opens = [
            "java.base/java.lang=ALL-UNNAMED",
            "java.base/java.lang.reflect=ALL-UNNAMED",
            "java.base/java.util=ALL-UNNAMED",
        ];
        for o in add_opens {
            args.insert(0, format!("--add-opens={}", o));
        }

        // Nacos 2.2.1+ 校验 server identity：即使 auth.enabled=false，部分模块
        // 初始化也要求 identity 有值，否则报 Empty identity 启动失败。
        // 提供开发默认值（官方文档示例值），本地开发工具场景足够。
        args.insert(0, "-Dnacos.core.auth.server.identity.key=serverIdentity".to_string());
        args.insert(0, "-Dnacos.core.auth.server.identity.value=security".to_string());
        args.insert(0, "-Dnacos.core.auth.plugin.nacos.token.secret.key=VGhpc0lzTXlDdXN0b21TZWNyZXRLZXkwMTIzNDU2Nzg=".to_string());

        // 数据库模式：embedded（Derby 默认）/ mysql
        let storage = config_str(&ctx.config, "storage", "embedded");
        let mut first_run_init = None;
        if storage == "mysql" {
            let host = config_str(&ctx.config, "mysql_host", "127.0.0.1");
            let mysql_port = config_u64(&ctx.config, "mysql_port", 3306);
            let db = config_str(&ctx.config, "mysql_db", "nacos");
            let user = config_str(&ctx.config, "mysql_user", "root");
            let password = config_str(&ctx.config, "mysql_password", "");

            // 配置 Nacos 使用 MySQL 数据源
            let jdbc_url = format!(
                "jdbc:mysql://{}:{}/{}?characterEncoding=utf8&connectTimeout=1000&socketTimeout=3000&autoReconnect=true",
                host, mysql_port, db
            );
            args.insert(0, "-Dspring.datasource.platform=mysql".to_string());
            args.insert(0, "-Ddb.num=1".to_string());
            args.insert(0, format!("-Ddb.url.0={}", jdbc_url));
            args.insert(0, format!("-Ddb.user={}", user));
            args.insert(0, format!("-Ddb.password={}", password));

            // 首次启动自动建库建表：需已装 MySQL 的 mysql.exe
            let mysql_install = ctx.mysql_install_path.as_deref()
                .ok_or_else(|| anyhow::anyhow!("请先安装 MySQL（数据库模式需要）"))?;
            let mysql = std::path::Path::new(mysql_install)
                .join("bin").join(if cfg!(windows) { "mysql.exe" } else { "mysql" });

            // 启动前探活：检测 MySQL 是否在运行。连接失败返回明确提示，
            // 避免用户看到 Nacos 启动失败或建库报错的晦涩信息。
            // 用 -e "SELECT 1" 只验证连接，不做任何写操作。
            {
                let mut probe = std::process::Command::new(&mysql);
                probe
                    .args(["-h", &host, "-P", &mysql_port.to_string(), "-u", &user])
                    .args(["--connect-timeout=3", "-e", "SELECT 1"])
                    .env("MYSQL_PWD", &password);
                #[cfg(windows)]
                probe.creation_flags(CREATE_NO_WINDOW);
                let ok = probe
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if !ok {
                    // i18n: 前缀由前端 ErrorDialog 检测并调 t() 转译
                    return Err(anyhow::anyhow!("i18n:nacosMysqlNotRunning"));
                }
            }

            let schema_path = working_dir.join("conf").join("mysql-schema.sql");
            if !schema_path.exists() {
                return Err(anyhow::anyhow!("Nacos 缺少 mysql-schema.sql，请重新安装"));
            }
            // mysql -e "CREATE DATABASE ...; USE ...; SOURCE ..." 一步建库建表。
            // 密码走 MYSQL_PWD 环境变量，避免命令行明文泄漏。
            let sql = format!(
                "CREATE DATABASE IF NOT EXISTS `{}` DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci; USE `{}`; SOURCE {};",
                db, db,
                schema_path.to_string_lossy().replace('\\', "/")
            );
            let mut env = std::collections::BTreeMap::new();
            env.insert("MYSQL_PWD".to_string(), password);
            let init_cmd = StartCommand {
                program: mysql.to_string_lossy().to_string(),
                args: vec![
                    format!("-h{}", host),
                    format!("-P{}", mysql_port),
                    format!("-u{}", user),
                    "--default-character-set=utf8mb4".to_string(),
                    format!("-e{}", sql),
                ],
                env_vars: env,
                working_dir: working_dir.clone(),
                creation_flags: CREATE_NO_WINDOW,
                first_run_init: None,
            };
            first_run_init = Some(Box::new(FirstRunInit {
                init_command: init_cmd,
                temp_secret_output: None,
            }));
        }

        // JVM 堆内存：Size 字段存 "512m"/"1g"，拼 -Xms/-Xmx（同值）
        let heap = config_str(&ctx.config, "heap", "512m");
        if !heap.is_empty() {
            args.insert(0, format!("-Xmx{}", heap));
            args.insert(0, format!("-Xms{}", heap));
        }
        // 功能模块：config/naming/microservice/ai/all（默认 all 即不传）
        let function_mode = config_str(&ctx.config, "function_mode", "all");
        if !function_mode.is_empty() && function_mode != "all" {
            args.insert(0, format!("-Dnacos.functionMode={}", function_mode));
        }
        // 上下文路径（默认 /nacos）
        let context_path = config_str(&ctx.config, "context_path", "/nacos");
        if !context_path.is_empty() && context_path != "/nacos" {
            args.insert(0, format!("-Dnacos.server.contextPath={}", context_path));
        }

        Ok(StartCommand {
            program: java.to_string_lossy().to_string(),
            args,
            env_vars: std::collections::BTreeMap::new(),
            working_dir,
            creation_flags: CREATE_NO_WINDOW,
            first_run_init,
        })
    }

    fn health_check(&self, ctx: &HealthContext) -> HealthCheckSpec {
        let port = ctx.config.get("port").and_then(|v| v.as_u64()).map(|p| p as u16)
            .unwrap_or(if ctx.port > 0 { ctx.port } else { 8848 });
        // ponytail: TCP 探测端口。2.x/3.x 健康检查端点路径不同
        // （/nacos/v1/console/health/readiness vs /nacos/v3/admin/core/state/readiness），
        // TCP 探测规避版本差异。
        HealthCheckSpec::Tcp { port, timeout_ms: 1000 }
    }

    fn config_schema(&self) -> Option<ConfigSchema> {
        Some(ConfigSchema {
            fields: vec![
                ConfigField {
                    key: "port".to_string(),
                    label_i18n: "configField.port".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(8848),
                    section: None,
                    description_i18n: Some("configField.nacosServerPortDesc".to_string()),
                },
                ConfigField {
                    key: "console_port".to_string(),
                    label_i18n: "configField.nacosConsolePort".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(8080),
                    section: None,
                    description_i18n: Some("configField.nacosConsolePortDesc".to_string()),
                },
                ConfigField {
                    key: "mode".to_string(),
                    label_i18n: "configField.nacosMode".to_string(),
                    field_type: ConfigFieldType::Select {
                        options: vec!["standalone".to_string(), "cluster".to_string()],
                        labels: vec![],
                    },
                    default_value: serde_json::json!("standalone"),
                    section: None,
                    description_i18n: Some("configField.nacosModeDesc".to_string()),
                },
                ConfigField {
                    key: "storage".to_string(),
                    label_i18n: "configField.nacosStorage".to_string(),
                    field_type: ConfigFieldType::Select {
                        options: vec!["embedded".to_string(), "mysql".to_string()],
                        labels: vec![],
                    },
                    default_value: serde_json::json!("embedded"),
                    section: None,
                    description_i18n: Some("configField.nacosStorageDesc".to_string()),
                },
                ConfigField {
                    key: "mysql_host".to_string(),
                    label_i18n: "configField.nacosMysqlHost".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("127.0.0.1"),
                    section: None,
                    description_i18n: Some("configField.nacosMysqlHostDesc".to_string()),
                },
                ConfigField {
                    key: "mysql_port".to_string(),
                    label_i18n: "configField.nacosMysqlPort".to_string(),
                    // 用 Number 而非 Port：mysql_port 是连接外部 MySQL 的端口，
                    // 不是 Nacos 自己的监听端口，不应触发 collect_configured_ports 的占用检查。
                    field_type: ConfigFieldType::Number,
                    default_value: serde_json::json!(3306),
                    section: None,
                    description_i18n: Some("configField.nacosMysqlPortDesc".to_string()),
                },
                ConfigField {
                    key: "mysql_db".to_string(),
                    label_i18n: "configField.nacosMysqlDb".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("nacos"),
                    section: None,
                    description_i18n: Some("configField.nacosMysqlDbDesc".to_string()),
                },
                ConfigField {
                    key: "mysql_user".to_string(),
                    label_i18n: "configField.nacosMysqlUser".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("root"),
                    section: None,
                    description_i18n: Some("configField.nacosMysqlUserDesc".to_string()),
                },
                ConfigField {
                    key: "mysql_password".to_string(),
                    label_i18n: "configField.nacosMysqlPassword".to_string(),
                    field_type: ConfigFieldType::Password,
                    default_value: serde_json::json!(""),
                    section: None,
                    description_i18n: Some("configField.nacosMysqlPasswordDesc".to_string()),
                },
                ConfigField {
                    key: "jdk".to_string(),
                    label_i18n: "configField.nacosJdk".to_string(),
                    // options/labels 由 get_config_schema 命令层动态填充（已装 JDK/JRE）
                    field_type: ConfigFieldType::Select { options: vec![], labels: vec![] },
                    default_value: serde_json::json!(""),
                    section: None,
                    description_i18n: Some("configField.nacosJdkDesc".to_string()),
                },
                ConfigField {
                    key: "heap".to_string(),
                    label_i18n: "configField.nacosHeap".to_string(),
                    field_type: ConfigFieldType::Size { units: vec!["m".to_string(), "g".to_string()] },
                    default_value: serde_json::json!("512m"),
                    section: None,
                    description_i18n: Some("configField.nacosHeapDesc".to_string()),
                },
                ConfigField {
                    key: "function_mode".to_string(),
                    label_i18n: "configField.nacosFunctionMode".to_string(),
                    field_type: ConfigFieldType::Select {
                        options: vec![
                            "all".to_string(),
                            "config".to_string(),
                            "naming".to_string(),
                            "microservice".to_string(),
                            "ai".to_string(),
                        ],
                        labels: vec![],
                    },
                    default_value: serde_json::json!("all"),
                    section: None,
                    description_i18n: Some("configField.nacosFunctionModeDesc".to_string()),
                },
                ConfigField {
                    key: "context_path".to_string(),
                    label_i18n: "configField.nacosContextPath".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("/nacos"),
                    section: None,
                    description_i18n: Some("configField.nacosContextPathDesc".to_string()),
                },
            ],
            ephemeral_keys: vec![],
        })
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> { None }
}

