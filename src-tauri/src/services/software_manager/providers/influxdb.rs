use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField, ConfigFieldType,
    ConfigSchema, HealthCheckSpec, MirrorSource, SoftwareCategory,
};

use super::{
    ConfigContext, DataDirContext, HealthContext, InstallContext, SoftwareProvider, StartCommand,
    StartContext, resolve_data_dir,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

fn config_u64(c: &serde_json::Value, key: &str, default: u64) -> u64 {
    c.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

pub struct InfluxdbProvider;

impl InfluxdbProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for InfluxdbProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for InfluxdbProvider {
    fn key(&self) -> &str {
        "influxdb"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        let version = "2.9.1".to_string();
        // 平台差异在编译期选取对应官方包（Windows 为 zip，其余为 tar.gz）。
        #[cfg(windows)]
        let (url, format) = (
            "https://download.influxdata.com/influxdb/releases/influxdb2-2.9.1-windows_amd64.zip".to_string(),
            ArchiveFormat::Zip,
        );
        #[cfg(target_os = "linux")]
        let (url, format) = (
            "https://download.influxdata.com/influxdb/releases/influxdb2-2.9.1_linux_amd64.tar.gz".to_string(),
            ArchiveFormat::TarGz,
        );
        #[cfg(target_os = "macos")]
        let (url, format) = (
            "https://download.influxdata.com/influxdb/releases/influxdb2-2.9.1_darwin_amd64.tar.gz".to_string(),
            ArchiveFormat::TarGz,
        );
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        let (url, format) = (
            "https://download.influxdata.com/influxdb/releases/influxdb2-2.9.1_linux_amd64.tar.gz".to_string(),
            ArchiveFormat::TarGz,
        );

        let versions = vec![CatalogVersion {
            version: version.clone(),
            mirrors: vec![MirrorSource {
                name: "i18n:influxdbOfficial".to_string(),
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
            key: "influxdb".to_string(),
            name: "InfluxDB".to_string(),
            description: "时序数据库（InfluxDB v2）".to_string(),
            description_i18n: Some("catalogDesc.influxdb".to_string()),
            category: SoftwareCategory::TimeSeries,
            icon: "mdi:chart-line".to_string(),
            versions,
            default_version: version,
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
        // InfluxDB 以纯 flag 启动，无需写入配置文件；数据目录在 start_command 解析并创建。
        Ok(())
    }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        // InfluxDB 为 Go 原生二进制，免 JVM，直接运行 influxd。
        let port = config_u64(&ctx.config, "port", 8086);
        let bolt_path =
            resolve_data_dir(&ctx.config, "bolt_path", "data/influxd.bolt", &ctx.install_path);
        let engine_path =
            resolve_data_dir(&ctx.config, "engine_path", "data/engine", &ctx.install_path);

        // 确保数据目录父目录存在（bolt_path 是文件，父目录需先建）
        if let Some(parent) = bolt_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Some(parent) = engine_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let bin = if cfg!(windows) { "influxd.exe" } else { "influxd" };
        let args = vec![
            format!("--http-bind-address=:{}", port),
            format!("--bolt-path={}", bolt_path.to_string_lossy()),
            format!("--engine-path={}", engine_path.to_string_lossy()),
            "--log-level=info".to_string(),
        ];

        Ok(StartCommand {
            program: bin.to_string(),
            args,
            env_vars: std::collections::BTreeMap::new(),
            working_dir: PathBuf::from(&ctx.install_path),
            creation_flags: CREATE_NO_WINDOW,
            // T5（首次初始化 influx setup）采用设计 §8 推荐方案 (c)：P0 先保证 influxd 启动且 /ping 通过，
            // first_run_init 留待后续扩展 init_secrets 通道后再实现；管理员凭据此时由用户手动 CLI 完成。
            first_run_init: None,
        })
    }

    fn health_check(&self, ctx: &HealthContext) -> HealthCheckSpec {
        let port = ctx
            .config
            .get("port")
            .and_then(|v| v.as_u64())
            .map(|p| p as u16)
            .unwrap_or(if ctx.port > 0 { ctx.port } else { 8086 });
        HealthCheckSpec::Http {
            url: format!("http://127.0.0.1:{}/ping", port),
            expected_status: 204,
            timeout_ms: 1000,
        }
    }

    fn config_schema(&self) -> Option<ConfigSchema> {
        Some(ConfigSchema {
            fields: vec![
                ConfigField {
                    key: "port".to_string(),
                    label_i18n: "configField.influxdbPort".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(8086),
                    section: None,
                    description_i18n: Some("configField.influxdbPortDesc".to_string()),
                },
                ConfigField {
                    key: "bolt_path".to_string(),
                    label_i18n: "configField.influxdbBoltPath".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("data/influxd.bolt"),
                    section: None,
                    description_i18n: Some("configField.influxdbBoltPathDesc".to_string()),
                },
                ConfigField {
                    key: "engine_path".to_string(),
                    label_i18n: "configField.influxdbEnginePath".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("data/engine"),
                    section: None,
                    description_i18n: Some("configField.influxdbEnginePathDesc".to_string()),
                },
                ConfigField {
                    key: "auth_enabled".to_string(),
                    label_i18n: "configField.influxdbAuth".to_string(),
                    field_type: ConfigFieldType::Boolean,
                    default_value: serde_json::json!(true),
                    section: None,
                    description_i18n: Some("configField.influxdbAuthDesc".to_string()),
                },
                ConfigField {
                    key: "admin_user".to_string(),
                    label_i18n: "configField.influxdbAdminUser".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!(""),
                    section: None,
                    description_i18n: Some("configField.influxdbAdminUserDesc".to_string()),
                },
                ConfigField {
                    key: "admin_password".to_string(),
                    label_i18n: "configField.influxdbAdminPassword".to_string(),
                    field_type: ConfigFieldType::Password,
                    default_value: serde_json::json!(""),
                    section: None,
                    description_i18n: Some("configField.influxdbAdminPasswordDesc".to_string()),
                },
            ],
            // admin_user/admin_password 为一次性凭据，不落盘。
            ephemeral_keys: vec!["admin_user".to_string(), "admin_password".to_string()],
        })
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> {
        None
    }

    fn data_dirs(&self, ctx: &DataDirContext) -> Vec<PathBuf> {
        // bolt_path 是文件，备份其所在目录。
        let bolt = resolve_data_dir(
            &ctx.config,
            "bolt_path",
            "data/influxd.bolt",
            &ctx.install_path,
        );
        match bolt.parent() {
            Some(p) => vec![p.to_path_buf()],
            None => vec![bolt],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> serde_json::Value {
        serde_json::json!({
            "port": 8086,
            "bolt_path": "data/influxd.bolt",
            "engine_path": "data/engine",
            "auth_enabled": true,
            "admin_user": "admin",
            "admin_password": "secret",
        })
    }

    fn test_install_path() -> String {
        let p = std::env::temp_dir().join("opx_test_influxdb");
        let _ = std::fs::create_dir_all(&p);
        p.to_string_lossy().to_string()
    }

    #[test]
    fn key_and_category() {
        let p = InfluxdbProvider::new();
        assert_eq!(p.key(), "influxdb");
        assert_eq!(p.catalog_entry().category, SoftwareCategory::TimeSeries);
    }

    #[test]
    fn config_file_path_is_none() {
        let p = InfluxdbProvider::new();
        let ctx = ConfigContext {
            install_path: test_install_path(),
            version: "2.9.1".to_string(),
            config: test_config(),
        };
        assert!(p.config_file_path(&ctx).is_none());
    }

    #[test]
    fn min_jdk_version_is_none() {
        assert_eq!(InfluxdbProvider::new().min_jdk_version(), None);
    }

    #[test]
    fn start_command_has_http_bind_address() {
        let p = InfluxdbProvider::new();
        let ctx = StartContext {
            installed_id: "influxdb-1".to_string(),
            install_path: test_install_path(),
            version: "2.9.1".to_string(),
            config: test_config(),
            custom_start_command: None,
            init_password: None,
            jdk_install_path: None,
            mysql_install_path: None,
        };
        let cmd = p.start_command(&ctx).expect("start_command ok");
        #[cfg(windows)]
        assert_eq!(cmd.program, "influxd.exe");
        #[cfg(not(windows))]
        assert_eq!(cmd.program, "influxd");
        assert!(cmd
            .args
            .iter()
            .any(|a| a == "--http-bind-address=:8086"));
        assert!(cmd
            .args
            .iter()
            .any(|a| a.starts_with("--bolt-path=")));
        assert!(cmd
            .args
            .iter()
            .any(|a| a.starts_with("--engine-path=")));
        assert!(cmd.env_vars.is_empty());
        assert!(cmd.first_run_init.is_none());
    }

    #[test]
    fn health_check_is_http_ping_204() {
        let p = InfluxdbProvider::new();
        let hc = p.health_check(&HealthContext {
            installed_id: "influxdb-1".to_string(),
            install_path: test_install_path(),
            port: 0,
            config: test_config(),
        });
        match hc {
            HealthCheckSpec::Http {
                url,
                expected_status,
                timeout_ms,
            } => {
                assert_eq!(url, "http://127.0.0.1:8086/ping");
                assert_eq!(expected_status, 204);
                assert_eq!(timeout_ms, 1000);
            }
            _ => panic!("expected Http health check"),
        }
    }
}
