use anyhow::Result;
use futures::TryStreamExt;
use reqwest;
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// ponytail: 时区 +8 且 URL 指向 github.com → ghproxy.net 加速
fn github_accelerate(url: &str) -> String {
    if !url.contains("github.com") {
        return url.to_string();
    }
    // 检查系统时区是否为中国（UTC+8）
    let is_cn = chrono::Local::now().offset().local_minus_utc() == 8 * 3600;
    // 也检查 Asia/Shanghai
    let is_shanghai = std::env::var("TZ").map(|tz| tz.contains("Shanghai")).unwrap_or(false);
    if is_cn || is_shanghai {
        format!("https://ghproxy.net/{}", url)
    } else {
        url.to_string()
    }
}

pub fn download(url: &str, destination: &Path) -> Result<()> {
    let url = github_accelerate(url);
    // 设置 User-Agent：部分镜像对无 UA 请求返回 403
    let response = reqwest::blocking::Client::builder()
        .user_agent("OPX")
        .build()?
        .get(url)
        .send()?
        .error_for_status()?;
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
    let url = github_accelerate(url);
    // 设置 User-Agent：reqwest 默认不发该 header，部分镜像（如清华 TUNA）会对无 UA 请求返回 403
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .user_agent("OPX")
        .build()?
        .get(url)
        .send()
        .await?
        .error_for_status()?;

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
