use anyhow::Result;
use futures::TryStreamExt;
use reqwest;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub fn download(url: &str, destination: &Path) -> Result<()> {
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
