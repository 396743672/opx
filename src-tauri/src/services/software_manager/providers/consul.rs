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

pub struct ConsulProvider;

impl ConsulProvider { pub fn new() -> Self { Self } }
impl Default for ConsulProvider { fn default() -> Self { Self::new() } }

impl SoftwareProvider for ConsulProvider {
    fn key(&self) -> &str { "consul" }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];
        #[cfg(windows)]
        {
            // 执行时核实：HashiCorp releases URL
            versions.push(CatalogVersion {
                version: "1.18.0".to_string(),
                mirrors: vec![MirrorSource {
                    name: "i18n:consulOfficial".to_string(),
                    url: "https://releases.hashicorp.com/consul/1.18.0/consul_1.18.0_windows_amd64.zip".to_string(),
                    builtin: None,
                }],
                archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
            });
        }
        CatalogEntry {
            key: "consul".to_string(),
            name: "Consul".to_string(),
            description: "服务注册与发现".to_string(),
            description_i18n: Some("catalogDesc.consul".to_string()),
            category: SoftwareCategory::Registry,
            icon: "mdi:hexagon-multiple".to_string(),
            versions,
            default_version: "1.18.0".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> { Ok(()) }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        let mode = config_str(&ctx.config, "mode", "dev");
        let bind = config_str(&ctx.config, "bind", "127.0.0.1");
        let http_port = config_u64(&ctx.config, "http_port", 8500);
        let working_dir = PathBuf::from(&ctx.install_path);

        let mut args = vec!["agent".to_string()];
        if mode == "server" {
            let data_dir = working_dir.join("data");
            let _ = std::fs::create_dir_all(&data_dir);
            args.push("-server".to_string());
            args.push("-bootstrap-expect=1".to_string());
            args.push(format!("-data-dir={}", data_dir.to_string_lossy()));
            args.push(format!("-bind={}", bind));
        } else {
            args.push("-dev".to_string());
            args.push(format!("-bind={}", bind));
        }
        // dev / server 两分支都需显式指定 HTTP API 监听端口，否则用户改 http_port 后
        // 进程仍监听默认 8500，健康检查按新端口探测会误报失败
        args.push(format!("-http-port={}", http_port));

        Ok(StartCommand {
            program: "consul.exe".to_string(),
            args,
            env_vars: std::collections::BTreeMap::new(),
            working_dir,
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: None,
        })
    }

    fn health_check(&self, ctx: &HealthContext) -> HealthCheckSpec {
        let port = config_u64(&ctx.config, "http_port", 8500) as u16;
        let port = if port > 0 { port } else if ctx.port > 0 { ctx.port } else { 8500 };
        HealthCheckSpec::Http {
            url: format!("http://127.0.0.1:{}/v1/status/leader", port),
            expected_status: 200,
            timeout_ms: 1000,
        }
    }

    fn config_schema(&self) -> Option<ConfigSchema> {
        Some(ConfigSchema {
            fields: vec![
                ConfigField {
                    key: "mode".to_string(),
                    label_i18n: "configField.consulMode".to_string(),
                    field_type: ConfigFieldType::Select { options: vec!["dev".to_string(), "server".to_string()] },
                    default_value: serde_json::json!("dev"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "bind".to_string(),
                    label_i18n: "configField.bind".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("127.0.0.1"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "http_port".to_string(),
                    label_i18n: "configField.httpPort".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(8500),
                    section: None,
                    description_i18n: None,
                },
            ],
            ephemeral_keys: vec![],
        })
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> { None }
}

fn config_str(c: &serde_json::Value, key: &str, default: &str) -> String {
    c.get(key).and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| default.to_string())
}
fn config_u64(c: &serde_json::Value, key: &str, default: u64) -> u64 {
    c.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> ConsulProvider { ConsulProvider::new() }

    #[test]
    fn key_is_consul() { assert_eq!(provider().key(), "consul"); }

    #[test]
    fn catalog_is_registry_category() {
        assert_eq!(provider().catalog_entry().category, SoftwareCategory::Registry);
        assert_eq!(provider().catalog_entry().icon, "mdi:hexagon-multiple");
    }

    #[test]
    fn start_command_dev_mode_by_default() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/c".into(), version: "1".into(),
            config: serde_json::json!({}),
            custom_start_command: None, init_password: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        assert_eq!(cmd.program, "consul.exe");
        assert!(cmd.args.iter().any(|a| a == "agent"));
        assert!(cmd.args.iter().any(|a| a == "-dev"));
        assert!(cmd.first_run_init.is_none());
    }

    #[test]
    fn start_command_server_mode_when_configured() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/c".into(), version: "1".into(),
            config: serde_json::json!({ "mode": "server", "bind": "127.0.0.1" }),
            custom_start_command: None, init_password: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        assert!(cmd.args.iter().any(|a| a == "-server"));
        assert!(cmd.args.iter().any(|a| a == "-bootstrap-expect=1"));
        assert!(!cmd.args.iter().any(|a| a == "-dev"));
    }

    #[test]
    fn health_check_http_8500() {
        let ctx = HealthContext { installed_id: "x".into(), install_path: "/c".into(), port: 0, config: serde_json::json!({}) };
        match provider().health_check(&ctx) {
            HealthCheckSpec::Http { url, expected_status, .. } => {
                assert_eq!(expected_status, 200);
                assert!(url.contains("8500"));
            }
            o => panic!("expected Http, got {:?}", o),
        }
    }

    #[test]
    fn start_command_adds_http_port_default_8500() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/c".into(), version: "1".into(),
            config: serde_json::json!({}),
            custom_start_command: None, init_password: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        assert!(cmd.args.iter().any(|a| a == "-http-port=8500"));
    }

    #[test]
    fn start_command_http_port_from_config_both_modes() {
        for (mode, config) in [
            ("dev", serde_json::json!({ "http_port": 8600 })),
            ("server", serde_json::json!({ "mode": "server", "http_port": 8600 })),
        ] {
            let ctx = StartContext {
                installed_id: "x".into(), install_path: "/c".into(), version: "1".into(),
                config,
                custom_start_command: None, init_password: None,
            };
            let cmd = provider().start_command(&ctx).unwrap();
            assert!(
                cmd.args.iter().any(|a| a == "-http-port=8600"),
                "{} 模式 args 应含 -http-port=8600, got {:?}",
                mode, cmd.args
            );
        }
    }

    #[test]
    fn config_schema_has_mode_select_field() {
        let schema = provider().config_schema().unwrap();
        let mode_field = schema.fields.iter().find(|f| f.key == "mode").unwrap();
        match &mode_field.field_type {
            ConfigFieldType::Select { options } => {
                assert!(options.contains(&"dev".to_string()));
                assert!(options.contains(&"server".to_string()));
            }
            o => panic!("mode 应为 Select, got {:?}", o),
        }
    }
}
