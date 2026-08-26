use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField, ConfigFieldType,
    ConfigSchema, HealthCheckSpec, LogSource, LogSourceKind, MirrorSource, SoftwareCategory,
};

use super::{
    ConfigContext, DataDirContext, HealthContext, InstallContext, LogContext, SoftwareProvider,
    StartCommand, StartContext, default_log_sources, resolve_data_dir,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

fn config_str(c: &serde_json::Value, key: &str, default: &str) -> String {
    c.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| default.to_string())
}

fn config_u64(c: &serde_json::Value, key: &str, default: u64) -> u64 {
    c.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

pub struct ElasticsearchProvider;

/// 从 Maven Central maven-metadata.xml 解析 Elasticsearch 版本（`<version>x.y.z</version>` 形式）。
/// 仅保留主版本 >= 8 的纯数字三段式发行版本（8.x/9.x 延续当前架构且下载 URL 命名稳定；
/// 跳过 8 之前的远古版本与含 -alpha/-beta/-rc/-snapshot 后缀的预发布）。
fn parse_maven_versions(xml: &str) -> Vec<String> {
    let mut versions: Vec<String> = Vec::new();
    for cap in regex::Regex::new(r"<version>([^<]+)</version>")
        .expect("valid regex")
        .captures_iter(xml)
    {
        let v = cap[1].to_string();
        if v.contains('-') {
            continue;
        }
        let major: u32 = v.split('.').next().and_then(|s| s.parse().ok()).unwrap_or(0);
        if major < 8 {
            continue;
        }
        if !versions.contains(&v) {
            versions.push(v);
        }
    }
    versions
}

impl ElasticsearchProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ElasticsearchProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for ElasticsearchProvider {
    fn key(&self) -> &str {
        "elasticsearch"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        let version = "8.19.0".to_string();
        // 平台差异在编译期选取对应官方包（Windows 为 zip，其余为 tar.gz）。
        #[cfg(windows)]
        let (url, format) = (
            "https://artifacts.elastic.co/downloads/elasticsearch/elasticsearch-8.19.0-windows-x86_64.zip".to_string(),
            ArchiveFormat::Zip,
        );
        #[cfg(target_os = "linux")]
        let (url, format) = (
            "https://artifacts.elastic.co/downloads/elasticsearch/elasticsearch-8.19.0-linux-x86_64.tar.gz".to_string(),
            ArchiveFormat::TarGz,
        );
        #[cfg(target_os = "macos")]
        let (url, format) = (
            "https://artifacts.elastic.co/downloads/elasticsearch/elasticsearch-8.19.0-darwin-x86_64.tar.gz".to_string(),
            ArchiveFormat::TarGz,
        );
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        let (url, format) = (
            "https://artifacts.elastic.co/downloads/elasticsearch/elasticsearch-8.19.0-linux-x86_64.tar.gz".to_string(),
            ArchiveFormat::TarGz,
        );

        let versions = vec![CatalogVersion {
            version: version.clone(),
            mirrors: vec![MirrorSource {
                name: "i18n:elasticsearchOfficial".to_string(),
                url,
                builtin: None,
            }],
            archive: ArchiveInfo {
                format,
                size: None,
                sha256: None,
            },
        }];

        CatalogEntry {
            key: "elasticsearch".to_string(),
            name: "Elasticsearch".to_string(),
            description: "分布式搜索与分析引擎".to_string(),
            description_i18n: Some("catalogDesc.elasticsearch".to_string()),
            category: SoftwareCategory::Search,
            icon: "mdi:magnify".to_string(),
            versions,
            default_version: version,
        }
    }

    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        // 爬 Maven Central metadata，解析 Elasticsearch 正式版本（跳过 alpha/beta/rc/snapshot）。
        let url = "https://repo1.maven.org/maven2/org/elasticsearch/elasticsearch/maven-metadata.xml";
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .ok()?;
        let resp = client
            .get(url)
            .header("User-Agent", "OPX")
            .send()
            .ok()?;
        if !resp.status().is_success() {
            eprintln!("[elasticsearch] Maven metadata 返回 {}", resp.status());
            return None;
        }
        let xml = resp.text().ok()?;
        let versions = parse_maven_versions(&xml);
        if versions.is_empty() {
            return None;
        }
        #[cfg(windows)]
        let (url_part, format) = (
            |v: &str| format!("elasticsearch-{v}-windows-x86_64.zip"),
            ArchiveFormat::Zip,
        );
        #[cfg(target_os = "linux")]
        let (url_part, format) = (
            |v: &str| format!("elasticsearch-{v}-linux-x86_64.tar.gz"),
            ArchiveFormat::TarGz,
        );
        #[cfg(target_os = "macos")]
        let (url_part, format) = (
            |v: &str| format!("elasticsearch-{v}-darwin-x86_64.tar.gz"),
            ArchiveFormat::TarGz,
        );
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        let (url_part, format) = (
            |v: &str| format!("elasticsearch-{v}-linux-x86_64.tar.gz"),
            ArchiveFormat::TarGz,
        );

        Some(
            versions
                .into_iter()
                .map(|v| CatalogVersion {
                    version: v.clone(),
                    mirrors: vec![MirrorSource {
                        name: "i18n:elasticsearchOfficial".to_string(),
                        url: format!(
                            "https://artifacts.elastic.co/downloads/elasticsearch/{}",
                            url_part(&v)
                        ),
                        builtin: None,
                    }],
                    archive: ArchiveInfo {
                        format: format.clone(),
                        size: None,
                        sha256: None,
                    },
                })
                .collect(),
        )
    }

    fn post_install(&self, ctx: &InstallContext) -> Result<()> {
        // 预创建默认数据目录并写入默认 elasticsearch.yml（启动时会按 config 覆写）。
        let config_dir = ctx.install_dir().join("config");
        std::fs::create_dir_all(&config_dir)?;
        let data_path = ctx.install_dir().join("data");
        std::fs::create_dir_all(&data_path)?;
        let yml = format!(
            "# Generated by opx - Elasticsearch (default)\n\
             network.host: 127.0.0.1\n\
             http.port: 9200\n\
             transport.port: 9300\n\
             discovery.type: single-node\n\
             xpack.security.enabled: false\n\
             path.data: {data_path}\n",
            data_path = data_path.to_string_lossy(),
        );
        std::fs::write(config_dir.join("elasticsearch.yml"), yml)?;
        Ok(())
    }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        // ES 8.x 需要 JDK 17+，且必须是完整 JDK（含 jdk.attach 模块，供
        // org.elasticsearch.entitlement 使用）。ES 发行版自带完整 JDK（<install>/jdk），
        // 优先用它；配置里所选 JRE（缺 jdk.attach）会被忽略，否则启动即报
        // "Module jdk.attach not found"（boot layer 阶段失败）。
        let jdk = {
            let bundled = PathBuf::from(&ctx.install_path).join("jdk");
            let java_bin = if cfg!(windows) { "bin/java.exe" } else { "bin/java" };
            if bundled.join(java_bin).exists() {
                bundled.to_string_lossy().to_string()
            } else if let Some(p) = ctx.jdk_install_path.as_deref() {
                p.to_string()
            } else {
                return Err(anyhow::anyhow!(
                    "未找到 ES 内置 JDK 或已安装的完整 JDK，请先安装 JDK 后再启动"
                ));
            }
        };

        let port = config_u64(&ctx.config, "port", 9200);
        let transport_port = config_u64(&ctx.config, "transport_port", 9300);
        let heap = config_str(&ctx.config, "heap", "1g");
        let network_host = config_str(&ctx.config, "network_host", "127.0.0.1");
        let data_path =
            resolve_data_dir(&ctx.config, "data_path", "data", &ctx.install_path);

        // 按 config 生成 config/elasticsearch.yml（覆盖 post_install 的默认值）。
        // .yml 不被 config_editor 支持，故由 Provider 自行生成。
        let config_dir = PathBuf::from(&ctx.install_path).join("config");
        std::fs::create_dir_all(&config_dir)?;
        let yml = format!(
            "# Generated by opx - Elasticsearch\n\
             network.host: {network_host}\n\
             http.port: {port}\n\
             transport.port: {transport_port}\n\
             discovery.type: single-node\n\
             xpack.security.enabled: false\n\
             path.data: {data_path}\n",
            network_host = network_host,
            port = port,
            transport_port = transport_port,
            data_path = data_path.to_string_lossy(),
        );
        std::fs::write(config_dir.join("elasticsearch.yml"), yml)?;

        let bin = if cfg!(windows) {
            "bin/elasticsearch.bat"
        } else {
            "bin/elasticsearch"
        };

        let mut env_vars = std::collections::BTreeMap::new();
        env_vars.insert("ES_JAVA_HOME".to_string(), jdk);
        env_vars.insert(
            "ES_JAVA_OPTS".to_string(),
            format!("-Xms{heap} -Xmx{heap}"),
        );
        // 本机无 AWS 凭证时 S3RepositoryPlugin 会打印一大段 region 解析失败的
        // stacktrace（INFO 级噪音）。预设 region 让默认 provider chain 首项成功，
        // 使启动日志保持干净；不配 S3 快照仓库时该值无实际作用。
        env_vars.insert("AWS_REGION".to_string(), "us-east-1".to_string());

        Ok(StartCommand {
            program: bin.to_string(),
            args: vec![],
            env_vars,
            working_dir: PathBuf::from(&ctx.install_path),
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: None,
        })
    }

    fn health_check(&self, ctx: &HealthContext) -> HealthCheckSpec {
        let port = ctx
            .config
            .get("port")
            .and_then(|v| v.as_u64())
            .map(|p| p as u16)
            .unwrap_or(if ctx.port > 0 { ctx.port } else { 9200 });
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
                    label_i18n: "configField.esPort".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(9200),
                    section: None,
                    description_i18n: Some("configField.esPortDesc".to_string()),
                },
                ConfigField {
                    key: "transport_port".to_string(),
                    label_i18n: "configField.esTransportPort".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(9300),
                    section: None,
                    description_i18n: Some("configField.esTransportPortDesc".to_string()),
                },
                ConfigField {
                    key: "heap".to_string(),
                    label_i18n: "configField.esHeap".to_string(),
                    field_type: ConfigFieldType::Size {
                        units: vec!["m".to_string(), "g".to_string()],
                    },
                    default_value: serde_json::json!("1g"),
                    section: None,
                    description_i18n: Some("configField.esHeapDesc".to_string()),
                },
                ConfigField {
                    key: "network_host".to_string(),
                    label_i18n: "configField.esNetworkHost".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("127.0.0.1"),
                    section: None,
                    description_i18n: Some("configField.esNetworkHostDesc".to_string()),
                },
                ConfigField {
                    key: "data_path".to_string(),
                    label_i18n: "configField.esDataPath".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("data"),
                    section: None,
                    description_i18n: Some("configField.esDataPathDesc".to_string()),
                },
                ConfigField {
                    key: "jdk".to_string(),
                    label_i18n: "configField.esJdk".to_string(),
                    // options/labels 由 get_config_schema 命令层动态填充（已装 JDK/JRE）
                    field_type: ConfigFieldType::Select {
                        options: vec![],
                        labels: vec![],
                    },
                    default_value: serde_json::json!(""),
                    section: None,
                    description_i18n: Some("configField.esJdkDesc".to_string()),
                },
            ],
            ephemeral_keys: vec![],
            field_rules: vec![],
        })
    }

    fn log_sources(&self, ctx: &LogContext) -> Vec<LogSource> {
        let mut sources = default_log_sources(ctx);
        let file = PathBuf::from(&ctx.install_path)
            .join("logs")
            .join("elasticsearch.log");
        sources.push(crate::services::software_manager::log_viewer::attach_archives(LogSource {
            path: file.to_string_lossy().to_string(),
            kind: LogSourceKind::ProviderFile,
            has_levels: true,
            level_pattern: None,
            label: None,
            archives: vec![],
        }));
        sources
    }

    fn data_dirs(&self, ctx: &DataDirContext) -> Vec<PathBuf> {
        vec![resolve_data_dir(
            &ctx.config,
            "data_path",
            "data",
            &ctx.install_path,
        )]
    }

    fn min_jdk_version(&self) -> Option<u32> {
        // Elasticsearch 8.x 需要 JDK 17+
        Some(17)
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> {
        // .yml 不被 config_editor 支持，配置由 start_command 自行生成
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> serde_json::Value {
        serde_json::json!({
            "port": 9200,
            "transport_port": 9300,
            "heap": "1g",
            "network_host": "127.0.0.1",
            "data_path": "data",
            "jdk": "",
        })
    }

    fn test_install_path() -> String {
        let p = std::env::temp_dir().join("opx_test_elasticsearch");
        // 清理历史测试可能留下的 jdk/（self-contained 测试不应被污染）
        let _ = std::fs::remove_dir_all(p.join("jdk"));
        let _ = std::fs::create_dir_all(&p);
        p.to_string_lossy().to_string()
    }

    #[test]
    fn key_and_category() {
        let p = ElasticsearchProvider::new();
        assert_eq!(p.key(), "elasticsearch");
        assert_eq!(p.catalog_entry().category, SoftwareCategory::Search);
    }

    #[test]
    fn config_file_path_is_none() {
        let p = ElasticsearchProvider::new();
        let ctx = ConfigContext {
            install_path: test_install_path(),
            version: "8.19.0".to_string(),
            config: test_config(),
        };
        assert!(p.config_file_path(&ctx).is_none());
    }

    #[test]
    fn min_jdk_version_is_17() {
        assert_eq!(ElasticsearchProvider::new().min_jdk_version(), Some(17));
    }

    #[test]
    fn start_command_generates_elasticsearch_yml() {
        let p = ElasticsearchProvider::new();
        let install_path = test_install_path();
        let ctx = StartContext {
            installed_id: "es-1".to_string(),
            install_path: install_path.clone(),
            version: "8.19.0".to_string(),
            config: test_config(),
            custom_start_command: None,
            init_password: None,
            jdk_install_path: Some("C:\\jdk-17".to_string()),
            mysql_install_path: None,
        };
        let cmd = p.start_command(&ctx).expect("start_command ok");
        #[cfg(windows)]
        assert_eq!(cmd.program, "bin/elasticsearch.bat");
        #[cfg(not(windows))]
        assert_eq!(cmd.program, "bin/elasticsearch");
        assert!(cmd.args.is_empty());
        assert_eq!(cmd.env_vars.get("ES_JAVA_HOME"), Some(&"C:\\jdk-17".to_string()));
        assert!(cmd.env_vars.get("ES_JAVA_OPTS").map(|s| s.contains("-Xmx1g")).unwrap_or(false));

        let yml = std::fs::read_to_string(
            PathBuf::from(&install_path)
                .join("config")
                .join("elasticsearch.yml"),
        )
        .expect("elasticsearch.yml written");
        assert!(yml.contains("discovery.type: single-node"));
        assert!(yml.contains("xpack.security.enabled: false"));
        assert!(yml.contains("http.port: 9200"));
        assert!(yml.contains("transport.port: 9300"));
    }

    #[test]
    fn health_check_is_tcp_9200() {
        let p = ElasticsearchProvider::new();
        let hc = p.health_check(&HealthContext {
            installed_id: "es-1".to_string(),
            install_path: test_install_path(),
            port: 0,
            config: test_config(),
        });
        match hc {
            HealthCheckSpec::Tcp { port, timeout_ms } => {
                assert_eq!(port, 9200);
                assert_eq!(timeout_ms, 1000);
            }
            _ => panic!("expected Tcp health check"),
        }
    }

    #[test]
    fn start_command_prefers_bundled_jdk() {
        let p = ElasticsearchProvider::new();
        // 用独立临时目录，避免在共享 test_install_path 下留下 jdk/ 污染其他测试
        let install_path = std::env::temp_dir()
            .join("opx_test_es_bundled")
            .to_string_lossy()
            .to_string();
        // 模拟 ES 发行版自带完整 JDK（<install>/jdk，含 jdk.attach 模块）
        let bundled_bin = PathBuf::from(&install_path).join("jdk").join("bin");
        std::fs::create_dir_all(&bundled_bin).unwrap();
        std::fs::write(bundled_bin.join("java.exe"), "").unwrap();

        let ctx = StartContext {
            installed_id: "es-1".to_string(),
            install_path: install_path.clone(),
            version: "8.19.0".to_string(),
            config: test_config(),
            custom_start_command: None,
            init_password: None,
            // 配置里选了 JRE 也会被忽略，ES_JAVA_HOME 优先指向自带完整 JDK
            jdk_install_path: Some("D:\\jre-25".to_string()),
            mysql_install_path: None,
        };
        let cmd = p.start_command(&ctx).expect("start_command ok");
        let expected = PathBuf::from(&install_path).join("jdk").to_string_lossy().to_string();
        assert_eq!(cmd.env_vars.get("ES_JAVA_HOME"), Some(&expected));
    }

    #[test]
    fn parse_maven_versions_filters_prereleases() {
        let xml = r#"
<metadata>
  <versioning>
    <lastUpdated>20260825000000</lastUpdated>
    <versions>
      <version>7.17.30</version>
      <version>8.19.0</version>
      <version>8.19.1</version>
      <version>9.0.0-beta1</version>
      <version>9.0.0</version>
      <version>9.1.0-rc1</version>
      <version>9.2.0</version>
      <version>9.3.0-SNAPSHOT</version>
      <version>9.4.2-alpha1</version>
    </versions>
  </versioning>
</metadata>
"#;
        let vs = parse_maven_versions(xml);
        assert!(vs.contains(&"8.19.0".to_string()));
        assert!(vs.contains(&"8.19.1".to_string()));
        assert!(vs.contains(&"9.0.0".to_string()));
        assert!(vs.contains(&"9.2.0".to_string()));
        // 跳过 8 之前的版本（URL 命名不稳定且过时）
        assert!(!vs.contains(&"7.17.30".to_string()));
        // 跳过所有预发布/快照
        for bad in ["9.0.0-beta1", "9.1.0-rc1", "9.3.0-SNAPSHOT", "9.4.2-alpha1"] {
            assert!(!vs.contains(&bad.to_string()), "should skip {bad}");
        }
    }

    #[test]
    fn start_command_errors_without_jdk() {
        let p = ElasticsearchProvider::new();
        let ctx = StartContext {
            installed_id: "es-1".to_string(),
            install_path: test_install_path(),
            version: "8.19.0".to_string(),
            config: test_config(),
            custom_start_command: None,
            init_password: None,
            jdk_install_path: None,
            mysql_install_path: None,
        };
        let err = p.start_command(&ctx).unwrap_err();
        assert!(err.to_string().contains("请先安装 JDK"));
    }
}
