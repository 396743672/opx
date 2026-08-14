use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, MirrorSource, SoftwareCategory,
};

use super::{
    ConfigContext, FirstRunInit, HealthContext, InstallContext, SoftwareProvider, StartCommand,
    StartContext,
};

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
                    section: Some("nacosServer".to_string()),
                    description_i18n: Some("configField.nacosServerPortDesc".to_string()),
                },
                ConfigField {
                    key: "console_port".to_string(),
                    label_i18n: "configField.nacosConsolePort".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(8080),
                    section: Some("nacosServer".to_string()),
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
                    section: Some("nacosDeploy".to_string()),
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
                    section: Some("nacosDeploy".to_string()),
                    description_i18n: Some("configField.nacosStorageDesc".to_string()),
                },
                ConfigField {
                    key: "mysql_host".to_string(),
                    label_i18n: "configField.nacosMysqlHost".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("127.0.0.1"),
                    section: Some("nacosMysql".to_string()),
                    description_i18n: Some("configField.nacosMysqlHostDesc".to_string()),
                },
                ConfigField {
                    key: "mysql_port".to_string(),
                    label_i18n: "configField.nacosMysqlPort".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(3306),
                    section: Some("nacosMysql".to_string()),
                    description_i18n: Some("configField.nacosMysqlPortDesc".to_string()),
                },
                ConfigField {
                    key: "mysql_db".to_string(),
                    label_i18n: "configField.nacosMysqlDb".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("nacos"),
                    section: Some("nacosMysql".to_string()),
                    description_i18n: Some("configField.nacosMysqlDbDesc".to_string()),
                },
                ConfigField {
                    key: "mysql_user".to_string(),
                    label_i18n: "configField.nacosMysqlUser".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("root"),
                    section: Some("nacosMysql".to_string()),
                    description_i18n: Some("configField.nacosMysqlUserDesc".to_string()),
                },
                ConfigField {
                    key: "mysql_password".to_string(),
                    label_i18n: "configField.nacosMysqlPassword".to_string(),
                    field_type: ConfigFieldType::Password,
                    default_value: serde_json::json!(""),
                    section: Some("nacosMysql".to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::software_manager::providers::{HealthContext, StartContext};

    fn provider() -> NacosProvider { NacosProvider::new() }

    #[test]
    fn key_is_nacos() { assert_eq!(provider().key(), "nacos"); }

    #[test]
    fn catalog_is_registry_category() {
        assert_eq!(provider().catalog_entry().category, SoftwareCategory::Registry);
        assert_eq!(provider().catalog_entry().icon, "mdi:hexagon-multiple");
    }

    #[test]
    fn start_command_requires_jdk() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/n".into(), version: "2.5.3".into(),
            config: serde_json::json!({}), custom_start_command: None, init_password: None,
            jdk_install_path: None,
            mysql_install_path: None,
        };
        assert!(provider().start_command(&ctx).is_err(), "无 JDK 应报错");
    }

    #[test]
    fn start_command_uses_jdk_java_and_jar() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/nacos".into(), version: "2.5.3".into(),
            config: serde_json::json!({}), custom_start_command: None, init_password: None,
            jdk_install_path: Some("C:/jdk".into()),
            mysql_install_path: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        assert!(cmd.program.contains("java"), "program 应为 java, got {}", cmd.program);
        assert!(cmd.args.iter().any(|a| a == "-jar"), "应带 -jar");
        assert!(cmd.args.iter().any(|a| a.contains("nacos-server.jar")), "应指定 nacos-server.jar");
        assert!(cmd.args.iter().any(|a| a.contains("standalone")), "应带 standalone");
        assert!(cmd.args.iter().any(|a| a.contains("nacos.home=/nacos")), "应指定 nacos.home 为安装目录, args={:?}", cmd.args);
        assert!(cmd.args.iter().any(|a| a.contains("loader.path=") && a.contains("plugins")), "应指定 loader.path 指向 plugins, args={:?}", cmd.args);
        assert!(cmd.args.iter().any(|a| a.starts_with("--add-opens=")), "应带 --add-opens, args={:?}", cmd.args);
        assert!(cmd.args.iter().any(|a| a.contains("identity.key=serverIdentity")), "应设 server.identity.key, args={:?}", cmd.args);
        assert!(cmd.first_run_init.is_none());
    }

    #[test]
    fn health_check_tcp_8848_default() {
        let ctx = HealthContext { installed_id: "x".into(), install_path: "/n".into(), port: 0, config: serde_json::json!({}) };
        match provider().health_check(&ctx) {
            HealthCheckSpec::Tcp { port, .. } => assert_eq!(port, 8848),
            o => panic!("expected Tcp, got {:?}", o),
        }
    }

    #[test]
    fn start_command_uses_resolved_jdk_path_from_command_layer() {
        // 命令层已按表单选的 JDK 解析出路径填 jdk_install_path，provider 直接用。
        // config.jdk 存的是 installed_id（稳定标识），不再是路径。
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/nacos".into(), version: "2.5.3".into(),
            config: serde_json::json!({ "jdk": "some-installed-id" }),
            custom_start_command: None, init_password: None,
            jdk_install_path: Some("C:/chosen-jdk".into()),
            mysql_install_path: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        assert!(cmd.program.contains("chosen-jdk"), "应使用命令层解析的 JDK 路径, got {}", cmd.program);
    }

    #[test]
    fn start_command_applies_heap_and_function_mode_and_context() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/nacos".into(), version: "2.5.3".into(),
            config: serde_json::json!({
                "heap": "1g",
                "function_mode": "naming",
                "context_path": "/my-nacos",
            }),
            custom_start_command: None, init_password: None,
            jdk_install_path: Some("C:/jdk".into()),
            mysql_install_path: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        assert!(cmd.args.iter().any(|a| a == "-Xms1g"), "应带 -Xms1g");
        assert!(cmd.args.iter().any(|a| a == "-Xmx1g"), "应带 -Xmx1g");
        assert!(cmd.args.iter().any(|a| a.contains("functionMode=naming")), "应带 functionMode");
        assert!(cmd.args.iter().any(|a| a.contains("contextPath=/my-nacos")), "应带 contextPath");
    }

    #[test]
    fn start_command_defaults_omit_all_mode_and_standard_context() {
        // 默认 all 模式 + 默认 /nacos 上下文：不传额外参数
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/nacos".into(), version: "2.5.3".into(),
            config: serde_json::json!({}),
            custom_start_command: None, init_password: None,
            jdk_install_path: Some("C:/jdk".into()),
            mysql_install_path: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        assert!(cmd.args.iter().any(|a| a == "-Xms512m"), "默认堆内存 512m");
        assert!(!cmd.args.iter().any(|a| a.contains("functionMode")), "默认 all 不传 functionMode");
        assert!(!cmd.args.iter().any(|a| a.contains("contextPath")), "默认 /nacos 不传 contextPath");
    }

    #[test]
    fn config_schema_has_all_nacos_fields() {
        let schema = provider().config_schema().unwrap();
        let keys: Vec<&str> = schema.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"jdk"));
        assert!(keys.contains(&"heap"));
        assert!(keys.contains(&"function_mode"));
        assert!(keys.contains(&"context_path"));
        assert!(keys.contains(&"port"));
    }

    #[test]
    fn config_schema_has_port() {
        let schema = provider().config_schema().unwrap();
        assert!(schema.fields.iter().any(|f| f.key == "port"));
    }
}
