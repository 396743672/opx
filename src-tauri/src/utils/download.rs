use anyhow::Result;
use futures::TryStreamExt;
use reqwest;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

/// 未配置加速前缀时的内置默认
pub const DEFAULT_GITHUB_PROXY: &str = "https://ghfast.top";

/// 单个加速前缀的尝试次数（含首次）。
/// 实测失败多为「首连抖动」（如 ghfast.top 首连 22.7s 超时、紧接着重试即成功），
/// 故同源重试比换源更对症。
const MAX_ATTEMPTS_PER_MIRROR: u32 = 3;

/// 仅连接阶段的超时
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// 读停顿超时：两次数据块之间超过该时长即视为连接卡死（成功读到数据后重新计时）。
/// ⚠️ 不要改用 `ClientBuilder::timeout`：官方文档明确它是
/// 「from when the request starts connecting until the response body has finished」的**总 deadline**，
/// 几百 MB 的文件在慢网下会被它整体掐断；`read_timeout` 才是「检测 stalled connection」的正确工具。
const READ_TIMEOUT: Duration = Duration::from_secs(60);

static DOWNLOAD_CONFIG: Mutex<Option<DownloadConfig>> = Mutex::new(None);

#[derive(Clone)]
pub struct DownloadConfig {
    /// GitHub 加速前缀，按配置顺序尝试（首个为主，失败自动轮换到下一个）
    pub github_proxies: Vec<String>,
    pub global_proxy_url: String,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            github_proxies: vec![DEFAULT_GITHUB_PROXY.to_string()],
            global_proxy_url: String::new(),
        }
    }
}

/// 解析用户配置的加速前缀：支持换行 / 逗号 / 分号分隔，可填多个做轮换。
/// 返回可能为空（= 用户未配置），由调用方决定是否回退内置默认。
pub fn parse_proxy_list(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in raw.split(['\n', '\r', ',', ';']) {
        let p = part.trim().trim_end_matches('/');
        if !p.is_empty() && !out.iter().any(|x| x == p) {
            out.push(p.to_string());
        }
    }
    out
}

/// 启动时 / 保存设置时调用，每次刷新
pub fn init_download_config(github_proxy: String, global_proxy: String) {
    let mut list = parse_proxy_list(&github_proxy);
    if list.is_empty() {
        list.push(DEFAULT_GITHUB_PROXY.to_string());
    }
    let mut g = DOWNLOAD_CONFIG.lock().unwrap();
    *g = Some(DownloadConfig {
        github_proxies: list,
        global_proxy_url: global_proxy,
    });
}

fn get_config() -> DownloadConfig {
    let g = DOWNLOAD_CONFIG.lock().unwrap();
    g.clone().unwrap_or_default()
}

/// 返回该 URL 的候选请求地址：
/// - 已配全局代理，或 URL 不含 github.com → 原样请求（不叠加加速前缀，避免代理链）
/// - 否则 → 每个加速前缀各生成一个候选，按配置顺序依次尝试
fn resolve_candidates(url: &str, cfg: &DownloadConfig) -> Vec<String> {
    if !cfg.global_proxy_url.is_empty() || !url.contains("github.com") {
        return vec![url.to_string()];
    }
    let list: Vec<String> = cfg
        .github_proxies
        .iter()
        .map(|p| p.trim().trim_end_matches('/'))
        .filter(|p| !p.is_empty())
        .map(|p| format!("{}/{}", p, url))
        .collect();
    if list.is_empty() {
        return vec![url.to_string()];
    }
    tracing::info!(original = %url, candidates = ?list, "using github_proxy");
    list
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
    let candidates = resolve_candidates(url, &cfg);

    let mut builder = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .user_agent("OPX");
    if !cfg.global_proxy_url.is_empty() {
        tracing::info!(proxy = %cfg.global_proxy_url, "using global proxy");
        if let Ok(proxy) = reqwest::Proxy::all(&cfg.global_proxy_url) {
            builder = builder.proxy(proxy);
        }
    }
    let client = builder
        .build()
        .map_err(|e| anyhow::anyhow!("创建 HTTP 客户端失败: {}", e))?;

    let mut last_err: Option<anyhow::Error> = None;
    for (idx, candidate) in candidates.iter().enumerate() {
        for attempt in 1..=MAX_ATTEMPTS_PER_MIRROR {
            match fetch_to_file(&client, candidate, destination, &mut on_progress).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    tracing::warn!(
                        url = %candidate,
                        attempt,
                        max_attempts = MAX_ATTEMPTS_PER_MIRROR,
                        error = %e,
                        "下载失败"
                    );
                    last_err = Some(e);
                    if attempt < MAX_ATTEMPTS_PER_MIRROR {
                        // 退避 1s → 3s，避开瞬时抖动
                        let backoff = Duration::from_secs(if attempt == 1 { 1 } else { 3 });
                        tokio::time::sleep(backoff).await;
                    }
                }
            }
        }
        if idx + 1 < candidates.len() {
            tracing::warn!(
                next = %candidates[idx + 1],
                "该加速前缀重试仍失败，换用下一个"
            );
        }
    }

    Err(last_err.unwrap_or_else(|| {
        anyhow::anyhow!("下载失败 (代理={}, url={})", cfg.global_proxy_url, url)
    }))
}

/// 单次下载尝试：盖写目标文件（失败重试时由 `File::create` 截断残留）。
async fn fetch_to_file<F>(
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
    on_progress: &mut F,
) -> Result<()>
where
    F: FnMut(u64, Option<u64>),
{
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("下载失败 (url={}): {}", url, e))?
        .error_for_status()
        .map_err(|e| anyhow::anyhow!("服务器返回错误 (url={}): {}", url, e))?;

    let total_size = response.content_length();
    let mut file = File::create(destination)?;
    let mut downloaded: u64 = 0;
    on_progress(0, total_size);
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.try_next().await? {
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total_size);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_proxy_list_supports_multiple_separators_and_dedup() {
        let list = parse_proxy_list("https://a.com\nhttps://b.com/ , https://a.com;  ");
        assert_eq!(list, vec!["https://a.com", "https://b.com"]);
    }

    #[test]
    fn parse_proxy_list_empty_when_unset() {
        assert!(parse_proxy_list("  \n , ; ").is_empty());
    }

    #[test]
    fn resolve_candidates_rotates_prefixes_in_order() {
        let cfg = DownloadConfig {
            github_proxies: vec!["https://p1".into(), "https://p2".into()],
            global_proxy_url: String::new(),
        };
        let got = resolve_candidates("https://github.com/o/r/releases/download/v/f.zip", &cfg);
        assert_eq!(
            got,
            vec![
                "https://p1/https://github.com/o/r/releases/download/v/f.zip",
                "https://p2/https://github.com/o/r/releases/download/v/f.zip",
            ]
        );
    }

    #[test]
    fn resolve_candidates_leaves_url_alone_with_global_proxy_or_non_github() {
        // 配了全局代理：不再叠加加速前缀（避免代理链）
        let with_proxy = DownloadConfig {
            github_proxies: vec!["https://p1".into()],
            global_proxy_url: "http://127.0.0.1:7890".into(),
        };
        assert_eq!(
            resolve_candidates("https://github.com/a.zip", &with_proxy),
            vec!["https://github.com/a.zip"]
        );
        // 非 github 源：原样请求（如 nginx.org / cdn.mysql.com）
        let plain = DownloadConfig {
            github_proxies: vec!["https://p1".into()],
            global_proxy_url: String::new(),
        };
        assert_eq!(
            resolve_candidates("https://nginx.org/download/n.zip", &plain),
            vec!["https://nginx.org/download/n.zip"]
        );
    }
}
