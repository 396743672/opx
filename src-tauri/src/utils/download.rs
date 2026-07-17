use anyhow::Result;
use futures::TryStreamExt;
use reqwest;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;

static DOWNLOAD_CONFIG: OnceLock<DownloadConfig> = OnceLock::new();

pub struct DownloadConfig {
    pub github_proxy_url: String,
    pub global_proxy_url: String,
}

/// 启动时由 settings 初始化
pub fn init_download_config(github_proxy: String, global_proxy: String) {
    let _ = DOWNLOAD_CONFIG.set(DownloadConfig {
        github_proxy_url: github_proxy,
        global_proxy_url: global_proxy,
    });
}

fn config() -> &'static DownloadConfig {
    DOWNLOAD_CONFIG.get_or_init(|| DownloadConfig {
        github_proxy_url: "https://ghfast.top".to_string(),
        global_proxy_url: String::new(),
    })
}

fn resolve_url(url: &str) -> String {
    let cfg = config();
    // 全局代理优先（VPN 类可直连 GitHub）；无全局时走 GitHub 代理
    if cfg.global_proxy_url.is_empty() && url.contains("github.com") && !cfg.github_proxy_url.is_empty() {
        let proxied = format!("{}/{}", cfg.github_proxy_url.trim_end_matches('/'), url);
        tracing::info!(original = %url, proxied = %proxied, "using github_proxy");
        return proxied;
    }
    url.to_string()
}

pub fn download(url: &str, destination: &Path) -> Result<()> {
    let url = resolve_url(url);
    let mut builder = reqwest::blocking::Client::builder()
        .user_agent("OPX");
    let cfg = config();
    if !cfg.global_proxy_url.is_empty() {
        tracing::info!(proxy = %cfg.global_proxy_url, "using global proxy");
        if let Ok(proxy) = reqwest::Proxy::all(&cfg.global_proxy_url) {
            builder = builder.proxy(proxy);
        }
    }
    let response = builder.build()?
        .get(&url).send()?.error_for_status()?;
    let content = response.bytes()?;
    std::fs::write(destination, content)?;
    Ok(())
}

pub async fn download_with_progress<F>(
    url: &str,
    destination: &Path,
    mut on_progress: F,
) -> Result<()>
where
    F: FnMut(u64, Option<u64>),
{
    let url = resolve_url(url);
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .user_agent("OPX");
    let cfg = config();
    if !cfg.global_proxy_url.is_empty() {
        tracing::info!(proxy = %cfg.global_proxy_url, "using global proxy");
        if let Ok(proxy) = reqwest::Proxy::all(&cfg.global_proxy_url) {
            builder = builder.proxy(proxy);
        }
    }
    let response = builder.build()?
        .get(&url).send().await?.error_for_status()?;

    let total_size = response.content_length();
    let mut file = File::create(destination)?;
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.try_next().await? {
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total_size);
    }
    Ok(())
}
