use anyhow::Result;
use futures::TryStreamExt;
use reqwest;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

/// 未配置加速前缀时的内置默认
pub const DEFAULT_GITHUB_PROXY: &str = "https://ghfast.top";

/// 单个加速前缀的尝试次数（含首次）。
/// 实测失败多为「首连抖动」或「大文件传输中途断流」（如 GitHub/CloudFront 经代理下载），
/// 配合断点续传，同源重试比换源更对症——续传会在断点处继续，而非从头重下。
const MAX_ATTEMPTS_PER_MIRROR: u32 = 5;

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
    let mut g = DOWNLOAD_CONFIG.lock().unwrap_or_else(|e| e.into_inner());
    *g = Some(DownloadConfig {
        github_proxies: list,
        global_proxy_url: global_proxy,
    });
}

fn get_config() -> DownloadConfig {
    let g = DOWNLOAD_CONFIG.lock().unwrap_or_else(|e| e.into_inner());
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
        // 换用下一个镜像时，丢弃旧分片从头下载（避免用 A 镜像的半截拼 B 镜像）。
        // 同一镜像内多次重试则保留分片，由 fetch_to_file 走断点续传。
        if idx > 0 {
            let _ = std::fs::remove_file(destination);
        }
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

/// 单次下载尝试：支持**断点续传**（HTTP Range）。
///
/// - 目标文件已存在且非空 → 带 `Range: bytes=<n>-` 续传；服务器回 `206` 则追加写入，
///   回 `200`（不支持 Range）则从头覆盖。
/// - 断流后上层会按镜像重试，本函数保留分片继续，进度条不会回零。
/// - 完整下载后做大小校验：最终字节数必须匹配（`content_length` 或 `Content-Range` 推导的总长），
///   否则判为截断，由上层重试/续传，全部失败则由 installer 删除缓存。
async fn fetch_to_file<F>(
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
    on_progress: &mut F,
) -> Result<()>
where
    F: FnMut(u64, Option<u64>),
{
    // 已有分片大小——支持的镜像会据此续传；不支持的会回 200 让我们从头覆盖。
    let mut resume_from: u64 = std::fs::metadata(destination).map(|m| m.len()).unwrap_or(0);

    let mut request = client.get(url);
    if resume_from > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={}-", resume_from));
    }
    let response = request
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("下载失败 (url={}): {}", url, e))?
        .error_for_status()
        .map_err(|e| anyhow::anyhow!("服务器返回错误 (url={}): {}", url, e))?;

    let status = response.status();
    // 416 = 请求区间越界，通常意味本地分片已达到/超过服务器文件大小 → 视为已完成，
    // 交由上层归档头校验 + SHA 校验判真伪（不一致会被删除并重下），不在此报错。
    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE && resume_from > 0 {
        return Ok(());
    }
    // 已带 Range 但服务器不支持（回 200 全量）→ 从头覆盖；其余非 2xx → 报错。
    if !status.is_success() {
        return Err(anyhow::anyhow!("服务器返回错误 (url={}): {}", url, status));
    }
    if resume_from > 0 && status != reqwest::StatusCode::PARTIAL_CONTENT {
        resume_from = 0;
    }

    // 206 时 content_length 是「本次剩余」，总长 = 已存在 + 剩余（S3/CloudFront 均返回）。
    let total_size = response.content_length().map(|len| len + resume_from);

    let mut file = if resume_from > 0 {
        OpenOptions::new().create(true).append(true).open(destination)?
    } else {
        File::create(destination)?
    };

    let mut downloaded: u64 = resume_from;
    on_progress(downloaded, total_size);
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream
        .try_next()
        .await
        .map_err(|e| anyhow::anyhow!("下载被中断（连接重置/代理丢流）：{}", e))?
    {
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total_size);
    }
    file.flush()?;

    // 完整性校验（核心修复）：若拿到总长，最终字节数必须一致。
    // 否则「截断下载」会被误判为成功——大文件（如 212MB 的 nacos zip）经代理/GitHub
    // 传输中途断流时，stream 提前结束返回 Ok(())，opx 误以为下载成功，
    // 后续解压报 `invalid Zip archive: Could not find central directory end`。
    if let Some(expected) = total_size {
        if downloaded != expected {
            return Err(anyhow::anyhow!(
                "下载被截断：期望 {} 字节，实际收到 {} 字节（连接可能在传输中中断，建议检查代理/网络）",
                expected,
                downloaded
            ));
        }
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

    /// 极简 HTTP/1.1 测试服务器处理器：
    /// - 无 Range 的首请求 → 回 `200` 但只发前半段即断开（模拟「传输中途连接重置」）
    /// - 带 `Range: bytes=N-` 的续传请求 → 回 `206` 补齐剩余字节后正常关闭
    async fn serve_half_then_resume(mut sock: tokio::net::TcpStream, content: Vec<u8>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut buf = [0u8; 4096];
        let mut headers = Vec::new();
        loop {
            let n = match sock.read(&mut buf).await {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };
            headers.extend_from_slice(&buf[..n]);
            if headers.windows(4).any(|w| w == b"\r\n\r\n") || headers.len() > 16 * 1024 {
                break;
            }
        }
        let header_text = String::from_utf8_lossy(&headers);
        let start = header_text
            .lines()
            .find_map(|line| {
                let l = line.trim();
                if l.to_ascii_lowercase().starts_with("range:") {
                    let spec = l["range:".len()..].trim();
                    if let Some(rest) = spec.strip_prefix("bytes=") {
                        return rest
                            .split('-')
                            .next()
                            .unwrap_or("0")
                            .trim()
                            .parse::<usize>()
                            .ok();
                    }
                }
                None
            })
            .unwrap_or(0);

        let total = content.len();
        let (status, len_hdr, body) = if start == 0 {
            (
                "HTTP/1.1 200 OK\r\n",
                format!("Content-Length: {}\r\n", total),
                &content[..total / 2],
            )
        } else {
            (
                "HTTP/1.1 206 Partial Content\r\n",
                format!("Content-Range: bytes {}-{}/{}\r\n", start, total - 1, total),
                &content[start..],
            )
        };
        let head = format!("{}{}Connection: close\r\n\r\n", status, len_hdr);
        let _ = sock.write_all(head.as_bytes()).await;
        let _ = sock.write_all(body).await;
        let _ = sock.flush().await;
    }

    #[tokio::test]
    async fn download_resumes_after_midstream_reset() {
        let total: usize = 100_000;
        let content: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let srv_content = content.clone();
        let srv = tokio::spawn(async move {
            loop {
                let (sock, _) = match listener.accept().await {
                    Ok(v) => v,
                    Err(_) => break,
                };
                let c = srv_content.clone();
                tokio::spawn(serve_half_then_resume(sock, c));
            }
        });

        let dest = std::env::temp_dir().join(format!("opx_dl_resume_{}.bin", std::process::id()));
        let _ = std::fs::remove_file(&dest);
        // 清空全局代理，确保直连本地服务器（不受系统代理影响）
        init_download_config(String::new(), String::new());

        let url = format!("http://{}/test.bin", addr);
        let result = download_with_progress(&url, &dest, |_, _| {}).await;

        let got = std::fs::read(&dest);
        let _ = std::fs::remove_file(&dest);
        srv.abort();

        result.expect("传输中途断流后应通过断点续传成功完成下载");
        assert_eq!(
            got.expect("目标文件应已写出"),
            content,
            "续传后文件内容应与原始一致"
        );
    }
}
