use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, BuiltinInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, MirrorSource, SoftwareCategory,
};
use crate::services::software_manager::providers::builtin_manifest;

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
                mirrors: {
                    let mut m = vec![];
                    let sha = builtin_manifest()
                        .get_builtin("nginx", "1.31.2")
                        .map(|e| e.sha256.clone())
                        .unwrap_or_default();
                    let size = builtin_manifest()
                        .get_builtin("nginx", "1.31.2")
                        .map(|e| e.size)
                        .unwrap_or(0);
                    m.push(MirrorSource {
                        name: "i18n:builtinVersion".to_string(),
                        url: "builtin://software/nginx/1.31.2.zip".to_string(),
                        builtin: Some(BuiltinInfo {
                            version: "1.31.2".to_string(),
                            sha256: sha,
                            size: size,
                        }),
                    });
                    m.push(MirrorSource {
                        name: "i18n:huaweiMirror".to_string(),
                        url: "https://mirrors.huaweicloud.com/nginx/nginx-1.31.2.zip".to_string(),
                        builtin: None,
                    });
                    m.push(MirrorSource {
                        name: "i18n:official".to_string(),
                        url: "https://nginx.org/download/nginx-1.31.2.zip".to_string(),
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

        // 注：Nginx 本设计 Windows-only（与 MySQL/Redis 决策一致）。
        // Unix 上 Nginx 二进制名是 nginx（无 .exe），且配置路径不同，
        // 如需 Unix 支持须单独适配。本 provider 不在 Unix catalog 注册版本。

        CatalogEntry {
            key: "nginx".to_string(),
            name: "Nginx".to_string(),
            description: "高性能 HTTP 服务器与反向代理".to_string(),
            category: SoftwareCategory::WebServer,
            icon: "mdi:web".to_string(),
            versions,
            default_version: "1.31.2".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
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
                    field_type: ConfigFieldType::Number,
                    default_value: serde_json::json!(4),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nginx_1312_has_builtin_as_first_mirror() {
        let entry = NginxProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "1.31.2")
            .expect("应有 1.31.2 版本");
        assert!(v.mirrors[0].builtin.is_some());
        assert_eq!(v.mirrors[0].builtin.as_ref().unwrap().version, "1.31.2");
    }

    #[test]
    fn nginx_fetch_remote_versions_method_exists() {
        let provider = NginxProvider::new();
        let _ = provider.fetch_remote_versions();
    }

    #[test]
    fn nginx_start_command_uses_daemon_off() {
        let p = NginxProvider::new();
        let ctx = super::StartContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/nginx/1.31.2".to_string(),
            version: "1.31.2".to_string(),
            config: serde_json::json!({}),
            custom_start_command: None,
            init_password: None,
        };
        let cmd = p.start_command(&ctx).unwrap();
        assert_eq!(cmd.program, "nginx.exe");
        assert!(cmd.args.contains(&"-g".to_string()));
        assert!(cmd.args.contains(&"daemon off;".to_string()));
        assert_eq!(cmd.working_dir, std::path::PathBuf::from("apps/nginx/1.31.2"));
        assert!(cmd.first_run_init.is_none());
        assert_eq!(cmd.creation_flags, CREATE_NO_WINDOW);
    }

    #[test]
    fn nginx_health_check_uses_http() {
        let p = NginxProvider::new();
        let ctx = super::HealthContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/nginx/1.31.2".to_string(),
            port: 80,
            config: serde_json::json!({}),
        };
        match p.health_check(&ctx) {
            crate::models::software::HealthCheckSpec::Http { url, expected_status, .. } => {
                assert_eq!(url, "http://127.0.0.1:80/");
                assert_eq!(expected_status, 200);
            }
            _ => panic!("应为 Http"),
        }
    }

    #[test]
    fn nginx_health_check_uses_config_port_over_ctx_port() {
        let p = NginxProvider::new();
        let ctx = super::HealthContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/nginx/1.31.2".to_string(),
            port: 80,
            config: serde_json::json!({"listen": 8080}),
        };
        match p.health_check(&ctx) {
            crate::models::software::HealthCheckSpec::Http { url, .. } => {
                assert_eq!(url, "http://127.0.0.1:8080/", "应使用 config.listen 端口");
            }
            _ => panic!("应为 Http"),
        }
    }

    #[test]
    fn nginx_health_check_uses_ctx_port_when_config_missing() {
        let p = NginxProvider::new();
        let ctx = super::HealthContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/nginx/1.31.2".to_string(),
            port: 8081,
            config: serde_json::json!({}),
        };
        match p.health_check(&ctx) {
            crate::models::software::HealthCheckSpec::Http { url, .. } => {
                assert_eq!(url, "http://127.0.0.1:8081/", "ctx.port 应优先于默认 80");
            }
            _ => panic!("应为 Http"),
        }
    }

    #[test]
    fn nginx_health_check_falls_back_to_80_when_port_zero() {
        let p = NginxProvider::new();
        let ctx = super::HealthContext {
            installed_id: "uuid".to_string(),
            install_path: "apps/nginx/1.31.2".to_string(),
            port: 0,
            config: serde_json::json!({}),
        };
        match p.health_check(&ctx) {
            crate::models::software::HealthCheckSpec::Http { url, .. } => {
                assert_eq!(url, "http://127.0.0.1:80/", "应回退到 80 端口");
            }
            _ => panic!("应为 Http"),
        }
    }

    #[test]
    fn nginx_config_schema_has_three_fields() {
        let p = NginxProvider::new();
        let schema = p.config_schema().expect("Nginx 应有 schema");
        assert_eq!(schema.fields.len(), 3);
        let keys: Vec<_> = schema.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"listen"));
        assert!(keys.contains(&"worker_processes"));
        assert!(keys.contains(&"root"));
        for f in &schema.fields {
            assert!(f.section.is_none(), "Nginx 字段 {} 不应有 section", f.key);
        }
    }

    #[test]
    fn nginx_config_file_path_returns_conf_nginx_conf() {
        let p = NginxProvider::new();
        let ctx = super::ConfigContext {
            install_path: "apps/nginx/1.31.2".to_string(),
            version: "1.31.2".to_string(),
            config: serde_json::json!({}),
        };
        let path = p.config_file_path(&ctx).expect("应有路径");
        let s = path.to_string_lossy();
        assert!(s.contains("conf"));
        assert!(s.ends_with("nginx.conf"));
    }
}
