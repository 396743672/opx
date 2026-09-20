//! 应用内自动更新内核。
//!
//! 更新源按「系统设置里的代理」动态解析（不写死 endpoint）：
//! 1) 显式全局 CONNECT 代理（如本机 VPN `proxy_url`）→ 直连 latest.json + 注入 proxy，
//!    latest.json 与安装包二进制都走该代理，最稳。
//! 2) 配置了 GitHub 加速代理（默认 `github_proxy_url = https://ghfast.top`）→ 走镜像
//!    `latest-ghproxy.json`（发版脚本已把内部 url 改写为镜像地址，二进制也走镜像）。
//! 3) 两者都没配 → 探测直连（3s 超时），不通则回退镜像。
//!
//! 注意：Tauri JS 的 `check()` 不支持自定义 endpoint，所以端点切换与代理注入只能在 Rust 完成。

use anyhow::{Context, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Updater, UpdaterExt};

const OWNER: &str = "396743672";
const REPO: &str = "opx";

/// `check_app_update` 返回给前端的结构
#[derive(Clone, Serialize)]
pub struct UpdateCheckResult {
    pub available: bool,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub current_version: String,
}

/// 下载进度事件（`update-progress`），前端累加 `chunk` 计算百分比
#[derive(Clone, Serialize)]
pub struct UpdateProgress {
    /// started | progress | finished | done
    pub phase: String,
    /// progress 阶段为本次增量字节，其余为 0
    pub chunk: u64,
    /// started 阶段携带总大小
    pub total: Option<u64>,
}

/// 直连 GitHub 的原始 latest.json
fn direct_endpoint() -> String {
    format!("https://github.com/{OWNER}/{REPO}/releases/latest/download/latest.json")
}

/// 走 GitHub 加速镜像的 latest.json（内部 url 已被发版脚本改写为镜像地址）
fn mirror_endpoint(mirror: &str) -> String {
    format!(
        "{}/https://github.com/{}/{}/releases/latest/download/latest-ghproxy.json",
        mirror.trim_end_matches('/'),
        OWNER,
        REPO
    )
}

/// 依据设置构建 updater（含端点 / 代理解析）
pub async fn build_updater(app: &AppHandle) -> Result<Updater> {
    let settings = crate::commands::config::read_settings().unwrap_or_default();
    let proxy_url = settings.proxy_url.trim().to_string();
    // 加速前缀支持配置多个（换行/逗号分隔，下载侧会轮换）；updater 只能挂一个端点，取首个
    let github_proxy = crate::utils::download::parse_proxy_list(&settings.github_proxy_url)
        .into_iter()
        .next()
        .unwrap_or_default();

    let (endpoint_strs, proxy) = if !proxy_url.is_empty() {
        (vec![direct_endpoint()], Some(proxy_url))
    } else if !github_proxy.is_empty() {
        (vec![mirror_endpoint(&github_proxy)], None)
    } else if direct_reachable().await {
        (vec![direct_endpoint()], None)
    } else {
        (vec![mirror_endpoint("https://ghfast.top")], None)
    };

    let endpoints: Vec<reqwest::Url> = endpoint_strs
        .iter()
        .map(|s| reqwest::Url::parse(s).context(format!("更新端点 URL 非法: {s}")))
        .collect::<Result<Vec<_>>>()?;

    let mut builder = app
        .updater_builder()
        .endpoints(endpoints)
        .map_err(|e| anyhow::anyhow!("设置更新端点失败: {e}"))?;

    if let Some(p) = proxy {
        let purl = reqwest::Url::parse(&p).context("全局代理地址无效")?;
        builder = builder.proxy(purl);
    }

    builder.build().context("构建 updater 失败")
}

/// 探测直连 GitHub 是否可达（3s 超时，no_proxy 直连，避免被环境变量代理接管）
async fn direct_reachable() -> bool {
    let client = match reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    client.head(&direct_endpoint()).send().await.is_ok()
}

/// 检查更新，返回结构化结果
pub async fn check_for_update(app: &AppHandle) -> Result<UpdateCheckResult> {
    let current = app.package_info().version.to_string();
    let updater = build_updater(app).await?;
    match updater.check().await {
        Ok(Some(update)) => Ok(UpdateCheckResult {
            available: true,
            version: Some(update.version),
            notes: update.body,
            current_version: current,
        }),
        Ok(None) => Ok(UpdateCheckResult {
            available: false,
            version: None,
            notes: None,
            current_version: current,
        }),
        Err(e) => Err(e).context("检查更新失败"),
    }
}

/// 检查并下载安装更新，进度通过 `update-progress` 事件推给前端。
/// Windows 上 NSIS 安装器会在安装步骤自动退出进程。
pub async fn install_update(app: &AppHandle) -> Result<()> {
    let updater = build_updater(app).await?;
    let update = updater
        .check()
        .await
        .context("检查更新失败")?
        .context("当前已是最新，无需更新")?;
    let app_prog = app.clone();
    let app_fin = app.clone();
    update
        .download_and_install(
            move |chunk_length: usize, content_length: Option<u64>| {
                let _ = app_prog.emit(
                    "update-progress",
                    UpdateProgress {
                        phase: "progress".into(),
                        chunk: chunk_length as u64,
                        total: content_length,
                    },
                );
            },
            move || {
                let _ = app_fin.emit(
                    "update-progress",
                    UpdateProgress {
                        phase: "finished".into(),
                        chunk: 0,
                        total: None,
                    },
                );
            },
        )
        .await
        .context("下载或安装更新失败")?;
    let _ = app.emit(
        "update-progress",
        UpdateProgress {
            phase: "done".into(),
            chunk: 0,
            total: None,
        },
    );
    Ok(())
}
