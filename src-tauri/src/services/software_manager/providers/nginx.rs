use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, MirrorSource, SoftwareCategory,
};

use super::{
    ConfigContext, HealthContext, InstallContext, SoftwareProvider, StartCommand, StartContext,
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
        // 安装后向主配置注入（全部幂等，重复安装不重复注入）：
        // 1. include sites/*.conf; —— 站点管理生成的 server 块由此加载
        // 2. map $http_upgrade $connection_upgrade {...} —— WebSocket 反代所需变量
        // 3. client_max_body_size 200m + underscores_in_headers on + gzip on
        use crate::services::website_manager::nginx_conf;
        let conf_path = PathBuf::from(&ctx.install_path)
            .join("conf")
            .join("nginx.conf");
        if let Ok(content) = std::fs::read_to_string(&conf_path) {
            let updated = nginx_conf::ensure_common_settings(
                &nginx_conf::ensure_map_upgrade(
                    &nginx_conf::ensure_include(&content),
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
        HealthCheckSpec::Http {
            url: format!("http://127.0.0.1:{}/", port),
            expected_status: 200,
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
        })
    }

    fn config_file_path(&self, ctx: &ConfigContext) -> Option<PathBuf> {
        Some(PathBuf::from(&ctx.install_path).join("conf").join("nginx.conf"))
    }
}
