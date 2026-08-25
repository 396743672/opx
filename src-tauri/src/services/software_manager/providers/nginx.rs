use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, LogSource, LogSourceKind, MirrorSource,
    SoftwareCategory,
};

use super::{
    ConfigContext, DataDirContext, HealthContext, InstallContext, LogContext, SoftwareProvider,
    StartCommand, StartContext,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

pub struct NginxProvider;

impl NginxProvider {
    pub fn new() -> Self {
        Self
    }

    /// Windows nginx 启动需要 temp/ 下这些临时目录（client_body_temp 等）；安装 zip 不含，
    /// 缺失时 nginx 报 `[emerg] CreateDirectory() ... failed` 直接退出。
    fn ensure_temp_dirs(install_path: &Path) -> Result<()> {
        for sub in [
            "client_body_temp",
            "proxy_temp",
            "fastcgi_temp",
            "uwsgi_temp",
            "scgi_temp",
        ] {
            std::fs::create_dir_all(install_path.join("temp").join(sub))?;
        }
        Ok(())
    }

    /// 确保主配置已注入 JSON 访问日志（幂等；读-改-写，无变化不写盘）。
    fn ensure_access_log_conf(install_path: &Path) -> Result<()> {
        use crate::services::website_manager::nginx_conf;
        let conf_path = install_path.join("conf").join("nginx.conf");
        let content = std::fs::read_to_string(&conf_path)?;
        let updated = nginx_conf::ensure_access_log(&content);
        if updated != content {
            std::fs::write(&conf_path, updated)?;
        }
        Ok(())
    }
}

impl Default for NginxProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for NginxProvider {
    fn key(&self) -> &str {
        "nginx"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];

        #[cfg(windows)]
        {
            versions.push(CatalogVersion {
                version: "1.31.2".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "i18n:huaweiMirror".to_string(),
                        url: "https://mirrors.huaweicloud.com/nginx/nginx-1.31.2.zip".to_string(),
                        builtin: None,
                    },
                    MirrorSource {
                        name: "i18n:official".to_string(),
                        url: "https://nginx.org/download/nginx-1.31.2.zip".to_string(),
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

        // 注：Nginx 本设计 Windows-only（与 MySQL/Redis 决策一致）。
        // Unix 上 Nginx 二进制名是 nginx（无 .exe），且配置路径不同，
        // 如需 Unix 支持须单独适配。本 provider 不在 Unix catalog 注册版本。

        CatalogEntry {
            key: "nginx".to_string(),
            name: "Nginx".to_string(),
            description: "高性能 HTTP 服务器与反向代理".to_string(),
            description_i18n: Some("catalogDesc.nginx".to_string()),
            category: SoftwareCategory::WebServer,
            icon: "mdi:web".to_string(),
            versions,
            default_version: "1.31.2".to_string(),
        }
    }

    fn post_install(&self, ctx: &InstallContext) -> Result<()> {
        // 创建 nginx 运行所需的 temp/ 临时目录，避免启动时 [emerg] CreateDirectory failed
        Self::ensure_temp_dirs(Path::new(&ctx.install_path))?;
        // 安装后向主配置注入（全部幂等，重复安装不重复注入）：
        // 1. include sites/*.conf; —— 站点管理生成的 server 块由此加载
        // 2. map $http_upgrade $connection_upgrade {...} —— WebSocket 反代所需变量
        // 3. client_max_body_size 200m + underscores_in_headers on + gzip on
        // 4. log_format opx_json + access_log —— JSON 访问日志
        use crate::services::website_manager::nginx_conf;
        let conf_path = PathBuf::from(&ctx.install_path)
            .join("conf")
            .join("nginx.conf");
        if let Ok(content) = std::fs::read_to_string(&conf_path) {
            let updated = nginx_conf::ensure_access_log(
                &nginx_conf::ensure_common_settings(
                    &nginx_conf::ensure_map_upgrade(
                        &nginx_conf::ensure_include(&content),
                    ),
                ),
            );
            if updated != content {
                std::fs::write(&conf_path, updated)?;
            }
        }
        Ok(())
    }

    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        // 爬 nginx.org/en/download.html，正则提取版本号
        let url = "https://nginx.org/en/download.html";
        let response = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .ok()?
            .get(url)
            .header("User-Agent", "OPX")
            .send()
            .ok()?;

        if !response.status().is_success() {
            eprintln!("[nginx] 下载页返回 {}", response.status());
            return None;
        }

        let html = response.text().ok()?;
        let mut versions = vec![];

        // 正则匹配 nginx-X.Y.Z.zip（mainline 版本 1.31.x）
        let re = regex::Regex::new(r"nginx-(\d+\.\d+\.\d+)\.zip").ok()?;
        let mut seen = std::collections::HashSet::new();

        for cap in re.captures_iter(&html) {
            let version = cap.get(1)?.as_str().to_string();
            if seen.contains(&version) {
                continue;
            }
            seen.insert(version.clone());

            // 只取主线版本（1.31.x）
            let minor: u32 = version
                .split('.')
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let major: u32 = version
                .split('.')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            if major == 1 && minor == 31 {
                versions.push(CatalogVersion {
                    version: version.clone(),
                    mirrors: vec![
                        MirrorSource {
                            name: "i18n:huaweiMirror".to_string(),
                            url: format!(
                                "https://mirrors.huaweicloud.com/nginx/nginx-{}.zip",
                                version
                            ),
                            builtin: None,
                        },
                        MirrorSource {
                            name: "i18n:official".to_string(),
                            url: format!("https://nginx.org/download/nginx-{}.zip", version),
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
        }

        if versions.is_empty() {
            None
        } else {
            Some(versions)
        }
    }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        // Windows nginx 需 temp/ 下多个临时目录（client_body_temp 等），缺失会 [emerg] 启动失败；
        // 安装时 post_install 已建，此处再次确保以覆盖已安装实例。
        Self::ensure_temp_dirs(Path::new(&ctx.install_path))?;
        // 确保 JSON 访问日志注入（幂等；覆盖升级/移植前的已装实例）
        Self::ensure_access_log_conf(Path::new(&ctx.install_path))?;
        Ok(StartCommand {
            program: "nginx.exe".to_string(),
            args: vec!["-g".to_string(), "daemon off;".to_string()],
            env_vars: std::collections::BTreeMap::new(),
            working_dir: PathBuf::from(&ctx.install_path),
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: None,
        })
    }

    fn health_check(&self, ctx: &HealthContext) -> HealthCheckSpec {
        let port = ctx
            .config
            .get("listen")
            .and_then(|v| v.as_u64())
            .map(|p| p as u16)
            .unwrap_or(if ctx.port > 0 { ctx.port } else { 80 });
        // Tcp 探测：nginx 端口通即健康。Http 200 过于严格——SSL 站点 80 会 301 跳转、
        // 无首页返回 404 时会被误判超时（引发僵尸进程累积）。
        HealthCheckSpec::Tcp {
            port,
            timeout_ms: 1000,
        }
    }

    fn config_schema(&self) -> Option<ConfigSchema> {
        Some(ConfigSchema {
            fields: vec![
                ConfigField {
                    key: "listen".to_string(),
                    label_i18n: "configField.listen".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(80),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "worker_processes".to_string(),
                    label_i18n: "configField.workerProcesses".to_string(),
                    field_type: ConfigFieldType::Select {
                        options: vec![
                            "auto".to_string(),
                            "1".to_string(),
                            "2".to_string(),
                            "4".to_string(),
                            "8".to_string(),
                            "16".to_string(),
                        ],
                        labels: vec![],
                    },
                    // auto = nginx 自动取 CPU 核数（最优），故默认 auto 即按当前系统 CPU 最优
                    default_value: serde_json::json!("auto"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "worker_connections".to_string(),
                    label_i18n: "configField.workerConnections".to_string(),
                    field_type: ConfigFieldType::Select {
                        options: vec![
                            "512".to_string(),
                            "1024".to_string(),
                            "2048".to_string(),
                            "4096".to_string(),
                            "8192".to_string(),
                        ],
                        labels: vec![],
                    },
                    default_value: serde_json::json!("1024"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "root".to_string(),
                    label_i18n: "configField.root".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("html"),
                    section: None,
                    description_i18n: None,
                },
            ],
            ephemeral_keys: vec![],
            field_rules: vec![],
        })
    }

    fn log_sources(&self, ctx: &LogContext) -> Vec<LogSource> {
        // nginx daemon off 正常无 stdout，控制台 tab 永远为空，故不引入 StdoutRedirect，
        // 只提供访问日志（JSON）与错误日志。
        let logs = Path::new(&ctx.install_path).join("logs");
        vec![
            LogSource {
                path: logs.join("access.log").to_string_lossy().to_string(),
                kind: LogSourceKind::ProviderFile,
                has_levels: false, // JSON 访问日志无级别
                level_pattern: None,
                label: Some("访问日志".into()),
            },
            LogSource {
                path: logs.join("error.log").to_string_lossy().to_string(),
                kind: LogSourceKind::ProviderFile,
                has_levels: true, // error.log 含 [error]/[notice] 等标记
                level_pattern: None,
                label: Some("错误日志".into()),
            },
        ]
    }

    fn config_file_path(&self, ctx: &ConfigContext) -> Option<PathBuf> {
        Some(PathBuf::from(&ctx.install_path).join("conf").join("nginx.conf"))
    }

    /// 升级时需迁移到新版的用户目录：站点配置（conf/sites）+ 站点数据（sites-data）
    fn data_dirs(&self, _ctx: &DataDirContext) -> Vec<PathBuf> {
        vec![
            PathBuf::from("sites-data"),
            PathBuf::from("conf/sites"),
        ]
    }
}
