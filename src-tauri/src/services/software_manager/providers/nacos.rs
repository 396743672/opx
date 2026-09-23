use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, FieldCondition, FieldRule, HealthCheckSpec, LogSource,
    MirrorSource, SoftwareCategory,
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

/// 解析集群节点列表：按行/逗号切分，trim 后滤空行；每项须为 `host:port`
/// （host 非空、port 1-65535）。返回规范化的 `host:port` 列表。
/// 空列表或任一项非法返回 Err。
fn parse_cluster_nodes(raw: &str) -> Result<Vec<String>, String> {
    let mut nodes = Vec::new();
    for token in raw.split(['\n', '\r', ',']) {
        let t = token.trim();
        if t.is_empty() {
            continue;
        }
        let (host, port) = t.rsplit_once(':').ok_or_else(|| format!("缺少端口: {t}"))?;
        let (host, port) = (host.trim(), port.trim());
        let port: u16 = port.parse().map_err(|_| format!("端口非法: {t}"))?;
        if host.is_empty() || port == 0 {
            return Err(format!("节点非法: {t}"));
        }
        nodes.push(format!("{host}:{port}"));
    }
    if nodes.is_empty() {
        return Err("节点列表为空".to_string());
    }
    Ok(nodes)
}

/// 生成 cluster.conf 内容：每行一个 `host:port`，结尾换行。
fn render_cluster_conf(nodes: &[String]) -> String {
    let mut s = nodes.join("\n");
    s.push('\n');
    s
}

pub struct NacosProvider;

/// 从 GitHub Releases JSON 解析 Nacos 正式版本 tag。
/// 跳过预发布（prerelease=true）、BETA、带日期后缀（3.2.1-2026.04.03）、
/// bugfix（3.1.0-bugfix）等非纯数字 X.Y.Z 版本。
fn parse_nacos_releases(releases: &[serde_json::Value]) -> Vec<String> {
    let mut versions: Vec<String> = Vec::new();
    for r in releases {
        if r.get("prerelease").and_then(|p| p.as_bool()).unwrap_or(false) {
            continue;
        }
        let tag = match r.get("tag_name").and_then(|t| t.as_str()) {
            Some(t) => t.to_string(),
            None => continue,
        };
        let is_pure_version = regex::Regex::new(r"^\d+\.\d+\.\d+$")
            .expect("valid regex")
            .is_match(&tag);
        if !is_pure_version {
            continue;
        }
        if !versions.contains(&tag) {
            versions.push(tag);
        }
    }
    versions
}

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

    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        // Nacos GitHub Releases，tag 即版本号（如 2.5.3 / 3.2.3），过滤预发布/日期后缀。
        let url = "https://api.github.com/repos/alibaba/nacos/releases?per_page=30";
        let client = reqwest::blocking::Client::builder()
            // 关掉环境变量代理探测（ALL_PROXY 等）：reqwest 默认会读，
            // 宿主若设了不支持 CONNECT 的 HTTP 代理，公网直连被劫持后必失败
            .no_proxy()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .ok()?;
        let resp = client
            .get(url)
            .header("User-Agent", "OPX")
            .header("Accept", "application/vnd.github+json")
            .send()
            .ok()?;
        if !resp.status().is_success() {
            eprintln!("[nacos] GitHub API 返回 {}", resp.status());
            return None;
        }
        let releases: Vec<serde_json::Value> = resp.json().ok()?;
        let versions = parse_nacos_releases(&releases);
        if versions.is_empty() {
            return None;
        }
        Some(
            versions
                .into_iter()
                .map(|v| CatalogVersion {
                    version: v.clone(),
                    mirrors: vec![MirrorSource {
                        name: "i18n:nacosOfficial".to_string(),
                        url: format!("https://github.com/alibaba/nacos/releases/download/{v}/nacos-server-{v}.zip"),
                        builtin: None,
                    }],
                    archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
                })
                .collect(),
        )
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

        // 部署模式：standalone（默认，单机）/ cluster（本实例作为集群一个节点加入外部集群）。
        let mode = config_str(&ctx.config, "mode", "standalone");
        let is_cluster = mode == "cluster";

        let mut args = vec![
            // nacos.home 指定安装根目录（conf/、data/、logs/ 都在其下）。
            // 不设则默认用户主目录 ~/nacos，导致找不到 conf 启动失败。
            format!("-Dnacos.home={}", working_dir.to_string_lossy()),
            // loader.path 加载 plugins/ 下的数据源插件（Derby/MySQL 驱动等）。
            // 不设则 Derby 驱动类找不到，standalone 模式启动失败。
            format!("-Dloader.path={}", working_dir.join("plugins").to_string_lossy()),
        ];
        if is_cluster {
            // 集群模式：Nacos 官方要求外部 MySQL；写入 conf/cluster.conf。
            // 不传 -Dnacos.standalone=true 即为集群模式。
            if config_str(&ctx.config, "storage", "embedded") != "mysql" {
                return Err(anyhow::anyhow!("i18n:nacosClusterNeedsMysql"));
            }
            let nodes = parse_cluster_nodes(&config_str(&ctx.config, "cluster_nodes", ""))
                .map_err(|_| anyhow::anyhow!("i18n:nacosClusterNodesInvalid"))?;
            let conf_dir = working_dir.join("conf");
            std::fs::create_dir_all(&conf_dir)?;
            std::fs::write(conf_dir.join("cluster.conf"), render_cluster_conf(&nodes))?;
        } else {
            args.push("-Dnacos.standalone=true".to_string());
        }
        args.push("-jar".to_string());
        args.push(jar.to_string_lossy().to_string());

        // 主服务端口（默认 8848）。必须无条件用 JVM 系统属性显式传入：系统属性优先级高于
        // 环境变量，否则宿主注入的 SERVER_PORT / SERVER__PORT 会被 Spring Boot 的宽松绑定
        // 解析成 server.port，覆盖 conf 配置（Nacos 会绑到宿主端口上，启动即失败）。
        // 2.x 只认 server.port，3.x 另认 nacos.server.main.port，两者都传以兼容。
        let server_port = config_u64(&ctx.config, "port", 8848);
        args.insert(0, format!("-Dnacos.server.main.port={}", server_port));
        args.insert(0, format!("-Dserver.port={}", server_port));
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
                remove_envs: Vec::new(),
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
            // Spring Boot 宽松绑定会把它们解析为 server.port，覆盖 Nacos 的端口
            // 配置（专属参数 -Dnacos.*.port 与 conf 均会被打穿），必须从继承环境中移除。
            remove_envs: vec!["SERVER_PORT".to_string(), "SERVER__PORT".to_string()],
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
        // mysql_* 连接字段仅在 storage=mysql 时显示（前端按 formData.storage 实时切换）
        let show_if_mysql = |key: &str| FieldRule {
            field_key: key.to_string(),
            visible_when: Some(FieldCondition {
                key: "storage".to_string(),
                equals: serde_json::json!("mysql"),
            }),
            required: false,
        };
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
                    key: "cluster_nodes".to_string(),
                    label_i18n: "configField.nacosClusterNodes".to_string(),
                    // 仅 cluster 模式可见且必填（见 field_rules）。一行一个 ip:port，含本节点。
                    field_type: ConfigFieldType::Textarea,
                    default_value: serde_json::json!(""),
                    section: None,
                    description_i18n: Some("configField.nacosClusterNodesDesc".to_string()),
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
                    field_type: ConfigFieldType::Select {
                        options: vec![],
                        labels: vec![],
                    },
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
            field_rules: vec![
                FieldRule {
                    field_key: "cluster_nodes".to_string(),
                    visible_when: Some(FieldCondition {
                        key: "mode".to_string(),
                        equals: serde_json::json!("cluster"),
                    }),
                    required: true,
                },
                show_if_mysql("mysql_host"),
                show_if_mysql("mysql_port"),
                show_if_mysql("mysql_db"),
                show_if_mysql("mysql_user"),
                show_if_mysql("mysql_password"),
            ],
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

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool) -> serde_json::Value {
        serde_json::json!({ "tag_name": tag, "prerelease": prerelease })
    }

    #[test]
    fn parse_nacos_releases_keeps_only_pure_versions() {
        let releases = vec![
            release("3.3.0-BETA", true),
            release("2.5.3", false),
            release("3.2.3", false),
            release("3.2.1-2026.04.03", false),
            release("3.1.0-bugfix", false),
            release("3.1.2", false),
        ];
        let vs = parse_nacos_releases(&releases);
        assert!(vs.contains(&"2.5.3".to_string()));
        assert!(vs.contains(&"3.2.3".to_string()));
        assert!(vs.contains(&"3.1.2".to_string()));
        for bad in ["3.3.0-BETA", "3.2.1-2026.04.03", "3.1.0-bugfix"] {
            assert!(!vs.contains(&bad.to_string()), "should skip {bad}");
        }
    }

    #[test]
    fn parse_cluster_nodes_accepts_lines_commas_and_whitespace() {
        let raw = "192.168.1.1:8848\n  10.0.0.2:8848 , [::1]:8848\r\n\n  nacos.example.com:8848  \n";
        let nodes = parse_cluster_nodes(raw).unwrap();
        assert_eq!(
            nodes,
            vec![
                "192.168.1.1:8848",
                "10.0.0.2:8848",
                "[::1]:8848",
                "nacos.example.com:8848",
            ]
        );
    }

    #[test]
    fn parse_cluster_nodes_rejects_bad_input() {
        // 空/纯空白
        assert!(parse_cluster_nodes("").is_err());
        assert!(parse_cluster_nodes("  \n , \n").is_err());
        // 缺端口
        assert!(parse_cluster_nodes("192.168.1.1").is_err());
        // 端口非法：0 / 越界 / 非数字
        assert!(parse_cluster_nodes("192.168.1.1:0").is_err());
        assert!(parse_cluster_nodes("192.168.1.1:65536").is_err());
        assert!(parse_cluster_nodes("192.168.1.1:abc").is_err());
        // host 为空
        assert!(parse_cluster_nodes(":8848").is_err());
    }

    #[test]
    fn render_cluster_conf_one_per_line_with_trailing_newline() {
        let nodes = vec!["192.168.1.1:8848".to_string(), "10.0.0.2:8848".to_string()];
        assert_eq!(render_cluster_conf(&nodes), "192.168.1.1:8848\n10.0.0.2:8848\n");
    }

    #[test]
    fn config_schema_gates_mysql_and_cluster_fields() {
        let schema = NacosProvider.config_schema().expect("nacos has schema");
        let rule = |key: &str| {
            schema
                .field_rules
                .iter()
                .find(|r| r.field_key == key)
                .unwrap_or_else(|| panic!("missing rule for {key}"))
        };
        // mysql_* 连接字段：仅 storage=mysql 时可见
        for k in ["mysql_host", "mysql_port", "mysql_db", "mysql_user", "mysql_password"] {
            let cond = rule(k).visible_when.as_ref().expect("has condition");
            assert_eq!(cond.key, "storage");
            assert_eq!(cond.equals, serde_json::json!("mysql"));
        }
        // cluster_nodes：仅 mode=cluster 时可见且必填
        let cluster = rule("cluster_nodes");
        assert!(cluster.required);
        let cond = cluster.visible_when.as_ref().expect("has condition");
        assert_eq!(cond.key, "mode");
        assert_eq!(cond.equals, serde_json::json!("cluster"));
    }

    #[test]
    fn start_command_always_pins_server_port() {
        // 回归：主端口必须无条件用 JVM 系统属性显式传入（系统属性优先级高于环境变量）。
        // 曾因「仅非默认端口才传」且用的是 2.x 不认的 nacos.server.main.port，
        // 导致宿主注入的 SERVER__PORT 经 Spring Boot 宽松绑定覆盖 server.port，Nacos 启动失败。
        let make = |port: u64| StartContext {
            installed_id: "t".to_string(),
            install_path: "C:\\nacos\\2.5.4".to_string(),
            version: "2.5.4".to_string(),
            config: serde_json::json!({ "port": port }),
            custom_start_command: None,
            init_password: None,
            jdk_install_path: Some("C:\\jdk-17".to_string()),
            mysql_install_path: None,
        };
        for port in [8848u64, 9999] {
            let cmd = NacosProvider
                .start_command(&make(port))
                .expect("start_command 应成功");
            for flag in [
                format!("-Dserver.port={port}"),
                format!("-Dnacos.server.main.port={port}"),
            ] {
                assert!(
                    cmd.args.iter().any(|a| *a == flag),
                    "端口 {port} 缺参数 {flag}；实际 args={:?}",
                    cmd.args
                );
            }
        }
    }
}

