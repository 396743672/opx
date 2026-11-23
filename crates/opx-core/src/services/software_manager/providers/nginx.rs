use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, LogSource, LogSourceKind, MirrorSource,
    SoftwareCategory,
};

use super::{
    ConfigContext, DataDirContext, GracefulStopCommand, HealthContext, InstallContext, LogContext,
    SoftwareProvider, StartCommand, StartContext, StopContext, exe_name,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

/// nginx 源码构建的前置依赖预检：PCRE / PCRE2 头文件缺失时返回可照抄的修复提示。
///
/// 为什么单查头文件就够：`./configure` 找不到 PCRE 时报的是
/// `the HTTP rewrite module requires the PCRE library`，而 nginx 三种解法里
/// 「装系统库」是唯一适合托管工具自动判断的；缺 zlib/OpenSSL 的情况实测不会
/// 出现在最小安装上（两者是常见基础依赖），故不做过度预检。
fn missing_pcre_hint() -> Option<String> {
    const HEADERS: &[&str] = &[
        "/usr/include/pcre2.h",
        "/usr/local/include/pcre2.h",
        "/usr/include/pcre.h",
        "/usr/local/include/pcre.h",
    ];
    if HEADERS.iter().any(|p| std::path::Path::new(p).exists()) {
        return None;
    }
    Some(
        "Nginx 源码构建缺少 PCRE 开发库（HTTP rewrite 模块必需）。\n\
         请先安装后重试：\n  \
         Debian/Ubuntu: apt install -y libpcre2-dev build-essential\n  \
         RHEL/CentOS:   dnf install -y pcre2-devel gcc make\n  \
         Alpine:        apk add pcre2-dev build-base"
            .to_string(),
    )
}

/// Nginx 官方**源码** tar.gz URL（P2-1：*nix 无预编译二进制，安装后 `./configure && make` 构建）。
fn nginx_source_url(version: &str) -> String {
    format!("https://nginx.org/download/nginx-{version}.tar.gz")
}

/// 远程版本按 OS 构造（回归：Linux 不能用 Windows .zip）。
///
/// nginx.org 同时发布 Windows `.zip`（预编译）与 *nix `.tar.gz`（源码，装后编译）。
/// 此前 `fetch_remote_versions` 正则只匹配 `.zip` 且硬编码 Zip 格式，导致 Linux
/// 刷新版本后拿到 Windows 包 → 解压出 `nginx.exe` → `sbin/nginx` 找不到。
/// Windows 保留华为云 + 官网双镜像；*nix 仅官网源码 tar.gz（华为云不镜像源码包）。
fn nginx_remote_version(version: &str, os: &str) -> CatalogVersion {
    let (mirrors, format) = if os == "windows" {
        (
            vec![
                MirrorSource {
                    name: "i18n:huaweiMirror".to_string(),
                    url: format!("https://mirrors.huaweicloud.com/nginx/nginx-{version}.zip"),
                    builtin: None,
                },
                MirrorSource {
                    name: "i18n:official".to_string(),
                    url: format!("https://nginx.org/download/nginx-{version}.zip"),
                    builtin: None,
                },
            ],
            ArchiveFormat::Zip,
        )
    } else {
        (
            vec![MirrorSource {
                name: "i18n:official".to_string(),
                url: nginx_source_url(version),
                builtin: None,
            }],
            ArchiveFormat::TarGz,
        )
    };
    CatalogVersion {
        version: version.to_string(),
        mirrors,
        archive: ArchiveInfo {
            format,
            size: None,
            sha256: None,
        },
    }
}

/// Nginx 可执行文件绝对路径（P2-1）：Windows 在安装根（`nginx.exe`）；
/// *nix 源码 `make install` 后位于 `<install>/sbin/nginx`。
fn nginx_bin_path(install_path: &str) -> String {
    if crate::utils::platform::current_os() == "windows" {
        PathBuf::from(install_path)
            .join(exe_name("nginx"))
            .to_string_lossy()
            .to_string()
    } else {
        PathBuf::from(install_path)
            .join("sbin")
            .join("nginx")
            .to_string_lossy()
            .to_string()
    }
}

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
        // P2-1：运行时按 OS 选包（Windows 官方 Windows 构建 zip / *nix 官方源码 tar.gz，装后编译）。
        let os = crate::utils::platform::current_os();
        const VER: &str = "1.31.2";
        let (mirrors, format) = if os == "windows" {
            (
                vec![
                    MirrorSource {
                        name: "i18n:huaweiMirror".to_string(),
                        url: format!("https://mirrors.huaweicloud.com/nginx/nginx-{VER}.zip"),
                        builtin: None,
                    },
                    MirrorSource {
                        name: "i18n:official".to_string(),
                        url: format!("https://nginx.org/download/nginx-{VER}.zip"),
                        builtin: None,
                    },
                ],
                ArchiveFormat::Zip,
            )
        } else {
            (
                vec![MirrorSource {
                    name: "i18n:official".to_string(),
                    url: nginx_source_url(VER),
                    builtin: None,
                }],
                ArchiveFormat::TarGz,
            )
        };
        let versions = vec![CatalogVersion {
            version: VER.to_string(),
            mirrors,
            archive: ArchiveInfo { format, size: None, sha256: None },
        }];

        CatalogEntry {
            key: "nginx".to_string(),
            name: "Nginx".to_string(),
            description: "高性能 HTTP 服务器与反向代理".to_string(),
            description_i18n: Some("catalogDesc.nginx".to_string()),
            category: SoftwareCategory::WebServer,
            icon: "mdi:web".to_string(),
            versions,
            default_version: VER.to_string(),
        }
    }

    fn post_install(&self, ctx: &InstallContext) -> Result<()> {
        // P2-1：非 Windows 走官方源码包 → 配置 + 编译 + 安装到 <install>
        // （产出 <install>/sbin/nginx 与 <install>/conf/nginx.conf）。依赖目标机 gcc/make 及 PCRE/zlib/OpenSSL 开发库。
        if crate::utils::platform::current_os() != "windows" {
            // 前置依赖预检：nginx 的 HTTP rewrite 模块强依赖 PCRE(2) 开发库，
            // 最小化 Debian/Ubuntu/RHEL 安装默认不带它，configure 会直接失败。
            // 不预检只能拿到「退出码 1」，用户无从下手（实测 Debian 13 即如此）。
            if let Some(hint) = missing_pcre_hint() {
                anyhow::bail!(hint);
            }
            let prefix = format!("--prefix={}", ctx.install_path);
            let steps: &[(&str, &[&str])] = &[
                ("./configure", &[prefix.as_str()]),
                ("make", &[]),
            ];
            super::run_build_steps(ctx.install_dir(), "Nginx", steps)?;
            // 不跑 `make install`：源码是直接解到安装根的（`install_dir == install_path`），
            // prefix 指向同一目录，install 阶段 `cp conf/koi-win <prefix>/conf/koi-win`
            // 会以「同一文件」报错并中断整个安装（实测 Debian 13 + nginx 1.31.9）。
            // 手动放置与 install 等价的产物即可：objs/nginx → sbin/nginx + logs/ 目录；
            // conf/ 与 html/ 本来就在安装根，无需再拷。
            let root = Path::new(&ctx.install_path);
            let built = root.join("objs").join("nginx");
            if !built.is_file() {
                anyhow::bail!(
                    "Nginx 构建产物缺失：{}（`make` 未产出可执行文件）",
                    built.display()
                );
            }
            std::fs::create_dir_all(root.join("sbin"))?;
            let target = root.join("sbin").join("nginx");
            std::fs::copy(&built, &target)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))?;
            }
            std::fs::create_dir_all(root.join("logs"))?;
        }
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
            // 关掉环境变量代理探测（ALL_PROXY 等）：reqwest 默认会读，
            // 宿主若设了不支持 CONNECT 的 HTTP 代理，公网直连被劫持后必失败
            .no_proxy()
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
        let os = crate::utils::platform::current_os();

        // 正则同时匹配 .zip（Windows 包）与 .tar.gz（*nix 源码包）；
        // 版本构造按 OS 分叉，避免 Linux 拿到 Windows 包。
        let re = regex::Regex::new(r"nginx-(\d+\.\d+\.\d+)\.(?:zip|tar\.gz)").ok()?;
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
                versions.push(nginx_remote_version(&version, &os));
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
            program: nginx_bin_path(&ctx.install_path),
            args: vec!["-g".to_string(), "daemon off;".to_string()],
            env_vars: std::collections::BTreeMap::new(),
            working_dir: PathBuf::from(&ctx.install_path),
            remove_envs: Vec::new(),
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
        let access = crate::services::software_manager::log_viewer::attach_archives(LogSource {
            path: logs.join("access.log").to_string_lossy().to_string(),
            kind: LogSourceKind::ProviderFile,
            has_levels: false, // JSON 访问日志无级别
            level_pattern: None,
            label: Some("访问日志".into()),
            archives: vec![],
        });
        let error = crate::services::software_manager::log_viewer::attach_archives(LogSource {
            path: logs.join("error.log").to_string_lossy().to_string(),
            kind: LogSourceKind::ProviderFile,
            has_levels: true, // error.log 含 [error]/[notice] 等标记
            level_pattern: None,
            label: Some("错误日志".into()),
            archives: vec![],
        });
        vec![access, error]
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

    /// P1-3：语义化优雅停止。`nginx -s quit` 让 master 等待 worker 处理完在途请求后退出，
    /// 比 `taskkill /F` 干净（避免连接被硬断）。
    fn graceful_stop_command(&self, ctx: &StopContext) -> Option<GracefulStopCommand> {
        Some(GracefulStopCommand {
            program: nginx_bin_path(&ctx.install_path),
            args: vec!["-s".to_string(), "quit".to_string()],
            working_dir: PathBuf::from(&ctx.install_path),
            env_vars: std::collections::BTreeMap::new(),
            timeout_secs: 10,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_version_uses_tar_gz_on_linux() {
        // 回归：Linux 远程版本若拿 Windows .zip，会解出 nginx.exe，sbin/nginx 找不到
        let v = nginx_remote_version("1.31.3", "linux");
        assert_eq!(v.version, "1.31.3");
        assert_eq!(v.archive.format, ArchiveFormat::TarGz);
        assert!(v.mirrors.iter().all(|m| m.url.ends_with(".tar.gz")));
        assert!(v.mirrors.iter().all(|m| !m.url.contains("huaweicloud")));
    }

    #[test]
    fn remote_version_uses_zip_on_windows() {
        let v = nginx_remote_version("1.31.3", "windows");
        assert_eq!(v.archive.format, ArchiveFormat::Zip);
        assert!(v.mirrors.iter().all(|m| m.url.ends_with(".zip")));
        assert!(v.mirrors.iter().any(|m| m.url.contains("huaweicloud")));
    }

    #[test]
    fn remote_version_macos_uses_source_tar_gz() {
        let v = nginx_remote_version("1.31.3", "macos");
        assert_eq!(v.archive.format, ArchiveFormat::TarGz);
        assert!(v.mirrors.iter().all(|m| m.url.ends_with(".tar.gz")));
    }
}
