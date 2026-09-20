use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, MirrorSource, SoftwareCategory,
};

use super::{
    ConfigContext, DataDirContext, HealthContext, InstallContext, SoftwareProvider, StartCommand,
    StartContext, resolve_data_dir,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

fn config_str(ctx_config: &serde_json::Value, key: &str, default: &str) -> String {
    ctx_config
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| default.to_string())
}

fn config_u64(ctx_config: &serde_json::Value, key: &str, default: u64) -> u64 {
    ctx_config.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

pub struct MinioProvider;

impl MinioProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MinioProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for MinioProvider {
    fn key(&self) -> &str {
        "minio"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];

        #[cfg(windows)]
        {
            // RELEASE.2025-04-22（GitHub 发布页）
            versions.push(CatalogVersion {
                version: "RELEASE.2025-04-22".to_string(),
                mirrors: vec![
                    // 自持源优先：MinIO 已于 2025-10 停发社区版预编译二进制（仓库 2026-04 归档），
                    // 上游资产随时可能下架，故固化到自有 Release。
                    // ⚠️ 资产名必须与上游逐字一致：ArchiveFormat::Executable 的缓存文件名取 URL
                    // basename（installer.rs），改名会导致已缓存文件失效、用户重新下载 121MB。
                    MirrorSource {
                        name: "i18n:selfHosted".to_string(),
                        url: format!(
                            "{}/minio.windows-amd64.RELEASE.2025-04-22T22-12-26Z.exe",
                            super::RESOURCE_BASE
                        ),
                        builtin: None,
                    },
                    MirrorSource {
                        name: "i18n:minioOfficial".to_string(),
                        url: "https://github.com/minio/minio/releases/download/RELEASE.2025-04-22T22-12-26Z/minio.windows-amd64.RELEASE.2025-04-22T22-12-26Z.exe".to_string(),
                        builtin: None,
                    },
                ],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Executable,
                    // 实测值（`minio --version` 自报 RELEASE.2025-04-22T22-12-26Z）；两源同源
                    size: Some(121_008_640),
                    sha256: Some(
                        "2ceb3b3d68bdf1c4def9702cb02c5c8adb235197d1c8f2eaad24136833ab9a57".to_string(),
                    ),
                },
            });
            // RELEASE.2025-09-07（上游最后一个带预编译二进制的版本）
            // 原 "latest" 指向 dl.min.io，实测已 HTTP 410 Gone（上游停发后下架），改指 GitHub Release 资产；
            // URL 含 github.com，运行时会被 download::resolve_url 自动加上 ghfast.top 前缀。
            // 注：RELEASE.2025-10-15 起上游只发源码（该 tag 资产数为 0），2025-09-07 是可用的最新版。
            versions.push(CatalogVersion {
                version: "RELEASE.2025-09-07".to_string(),
                mirrors: vec![MirrorSource {
                    name: "i18n:minioOfficial".to_string(),
                    url: "https://github.com/minio/minio/releases/download/RELEASE.2025-09-07T16-13-09Z/minio.windows-amd64.RELEASE.2025-09-07T16-13-09Z.exe".to_string(),
                    builtin: None,
                }],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Executable,
                    // 官方 .sha256sum 与 GitHub 独立 digest 双向互证（113,115,136 B）
                    size: Some(113_115_136),
                    sha256: Some(
                        "af709e6ba68488404e85acdd22a3030d0f5e56a108d4b27d744f18ceb50861b4".to_string(),
                    ),
                },
            });
        }

        // 注：MinIO 本设计 Windows-only（与 MySQL/Redis/Nginx 决策一致）。
        // Unix 上 MinIO 二进制名是 minio（无 .exe），如需 Unix 支持须单独适配。

        CatalogEntry {
            key: "minio".to_string(),
            name: "MinIO".to_string(),
            description: "S3 兼容对象存储".to_string(),
            description_i18n: Some("catalogDesc.minio".to_string()),
            category: SoftwareCategory::Storage,
            icon: "mdi:cloud".to_string(),
            versions,
            default_version: "RELEASE.2025-04-22".to_string(),
        }
    }

    fn post_install(&self, ctx: &InstallContext) -> Result<()> {
        let data_dir = ctx.install_dir().join("data");
        std::fs::create_dir_all(&data_dir)?;
        // ponytail: 下载的 exe 文件名带版本后缀，重命名为 minio.exe
        if let Ok(entries) = std::fs::read_dir(ctx.install_dir()) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "exe") && path.file_stem().map_or(true, |n| n != "minio") {
                    let target = ctx.install_dir().join("minio.exe");
                    let _ = std::fs::rename(&path, &target);
                    break;
                }
            }
        }
        Ok(())
    }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        let api_port = config_u64(&ctx.config, "api_port", 9000);
        let console_port = config_u64(&ctx.config, "console_port", 9001);
        let data_dir = config_str(&ctx.config, "data_dir", "./data");
        let access_key = config_str(&ctx.config, "access_key", "minioadmin");
        let secret_key = config_str(&ctx.config, "secret_key", "minioadmin");

        let mut env_vars = std::collections::BTreeMap::new();
        env_vars.insert("MINIO_ROOT_USER".to_string(), access_key);
        env_vars.insert("MINIO_ROOT_PASSWORD".to_string(), secret_key);

        // data_dir 解析为绝对路径（相对于 install_path），避免 minio 在错误目录创建
        let abs_data_dir = if std::path::Path::new(&data_dir).is_absolute() {
            data_dir.to_string()
        } else {
            let clean = data_dir
                .strip_prefix("./")
                .or_else(|| data_dir.strip_prefix(".\\"))
                .unwrap_or(&data_dir);
            std::path::PathBuf::from(&ctx.install_path)
                .join(clean)
                .to_string_lossy()
                .replace('\\', "/")
        };
        let _ = std::fs::create_dir_all(&abs_data_dir);

        // 内置版 RELEASE.2025-04-22 不支持 --console-address（该参数 2021-10 后引入），
        // 旧版用 MINIO_BROWSER_ADDRESS 环境变量；网络版是最新版，支持 --console-address
        let is_old_version = ctx.version == "RELEASE.2025-04-22";
        let mut args = vec![
            "server".to_string(),
            abs_data_dir,
            "--address".to_string(),
            format!(":{}", api_port),
        ];
        if is_old_version {
            env_vars.insert(
                "MINIO_BROWSER_ADDRESS".to_string(),
                format!(":{}", console_port),
            );
        } else {
            args.push("--console-address".to_string());
            args.push(format!(":{}", console_port));
        }

        Ok(StartCommand {
            program: "minio.exe".to_string(),
            args,
            env_vars,
            working_dir: PathBuf::from(&ctx.install_path),
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: None,
        })
    }

    fn health_check(&self, ctx: &HealthContext) -> HealthCheckSpec {
        let port = ctx
            .config
            .get("api_port")
            .and_then(|v| v.as_u64())
            .map(|p| p as u16)
            .unwrap_or(if ctx.port > 0 { ctx.port } else { 9000 });
        // 用 TCP 检查端口监听（比 HTTP 健康检查更可靠，不依赖具体端点路径）
        HealthCheckSpec::Tcp {
            port,
            timeout_ms: 1000,
        }
    }

    fn config_schema(&self) -> Option<ConfigSchema> {
        Some(ConfigSchema {
            fields: vec![
                ConfigField {
                    key: "api_port".to_string(),
                    label_i18n: "configField.apiPort".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(9000),
                    section: None,
                    description_i18n: Some("configField.apiPortDesc".to_string()),
                },
                ConfigField {
                    key: "console_port".to_string(),
                    label_i18n: "configField.consolePort".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(9001),
                    section: None,
                    description_i18n: Some("configField.consolePortDesc".to_string()),
                },
                ConfigField {
                    key: "data_dir".to_string(),
                    label_i18n: "configField.dataDir".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("./data"),
                    section: None,
                    description_i18n: Some("configField.dataDirDesc".to_string()),
                },
                ConfigField {
                    key: "access_key".to_string(),
                    label_i18n: "configField.accessKey".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("minioadmin"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "secret_key".to_string(),
                    label_i18n: "configField.secretKey".to_string(),
                    field_type: ConfigFieldType::Password,
                    default_value: serde_json::json!("minioadmin"),
                    section: None,
                    description_i18n: None,
                },
            ],
            ephemeral_keys: vec![],
            field_rules: vec![],
        })
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> {
        None
    }

    fn data_dirs(&self, ctx: &DataDirContext) -> Vec<PathBuf> {
        // MinIO 数据目录来自配置 data_dir（默认 ./data），需按 install_path 解析绝对路径
        vec![resolve_data_dir(&ctx.config, "data_dir", "./data", &ctx.install_path)]
    }
}
