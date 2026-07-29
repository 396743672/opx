use anyhow::Result;
use futures::TryStreamExt;
use reqwest;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

static DOWNLOAD_CONFIG: Mutex<Option<DownloadConfig>> = Mutex::new(None);

#[derive(Clone)]
pub struct DownloadConfig {
    pub github_proxy_url: String,
    pub global_proxy_url: String,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            github_proxy_url: "https://ghfast.top".to_string(),
            global_proxy_url: String::new(),
        }
    }
}

/// 启动时 / 保存设置时调用，每次刷新
pub fn init_download_config(github_proxy: String, global_proxy: String) {
    let mut g = DOWNLOAD_CONFIG.lock().unwrap();
    *g = Some(DownloadConfig {
        github_proxy_url: if github_proxy.is_empty() { "https://ghfast.top".to_string() } else { github_proxy },
        global_proxy_url: global_proxy,
    });
}

fn get_config() -> DownloadConfig {
    let g = DOWNLOAD_CONFIG.lock().unwrap();
    g.clone().unwrap_or_default()
}

fn resolve_url(url: &str, cfg: &DownloadConfig) -> String {
    if cfg.global_proxy_url.is_empty() && url.contains("github.com") && !cfg.github_proxy_url.is_empty() {
        let proxied = format!("{}/{}", cfg.github_proxy_url.trim_end_matches('/'), url);
        tracing::info!(original = %url, proxied = %proxied, "using github_proxy");
        return proxied;
    }
    url.to_string()
}

pub async fn download_with_progress<F>(
    url: &str,
    destination: &Path,
    mut on_progress: F,
) -> Result<()>
where
    F: FnMut(u64, Option<u64>),
{
    let cfg = get_config();
    let url = resolve_url(url, &cfg);
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .user_agent("OPX");
    if !cfg.global_proxy_url.is_empty() {
        tracing::info!(proxy = %cfg.global_proxy_url, "using global proxy");
        if let Ok(proxy) = reqwest::Proxy::all(&cfg.global_proxy_url) {
            builder = builder.proxy(proxy);
        }
    }
    let response = builder.build()
        .map_err(|e| anyhow::anyhow!("创建 HTTP 客户端失败: {}", e))?
        .get(&url).send().await
        .map_err(|e| anyhow::anyhow!("下载失败 (代理={}, url={}): {}", cfg.global_proxy_url, url, e))?
        .error_for_status()
        .map_err(|e| anyhow::anyhow!("服务器返回错误 (url={}): {}", url, e))?;

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
