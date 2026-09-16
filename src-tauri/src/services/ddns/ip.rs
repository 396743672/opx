//! 公网出口 IP 检测：多源轮询，任一成功即用。

use anyhow::{anyhow, Result};

const SOURCES_V4: &[&str] = &["https://myip.ipip.net", "https://api.ipify.org"];
const SOURCES_V6: &[&str] = &["https://api6.ipify.org", "https://ipv6.icanhazip.com"];

/// 依次请求源列表；提取失败换下一源，全失败报错。
pub async fn detect_public_ip(v6: bool) -> Result<String> {
    let sources: &[&str] = if v6 { SOURCES_V6 } else { SOURCES_V4 };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let mut last_err = String::new();
    for url in sources {
        match client.get(*url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body = resp.text().await.unwrap_or_default();
                if let Some(ip) = extract_ip(&body) {
                    // v6 源不该吐 v4（反之亦然）
                    let is_v4 = ip.contains('.') && !ip.contains(':');
                    if is_v4 != v6 {
                        return Ok(ip);
                    }
                }
                last_err = format!("{} 响应无有效 IP", url);
            }
            Ok(resp) => last_err = format!("{} 返回 {}", url, resp.status()),
            Err(e) => last_err = format!("{} 请求失败: {}", url, e),
        }
    }
    Err(anyhow!("公网 IP 检测失败：{}", last_err))
}

/// 从任意文本提取第一个合法 IP（IPv4 优先）。纯函数。
/// 扫描 [0-9a-fA-F.:] 连续片段（长度 7..=45），逐个尝试 parse。
pub fn extract_ip(body: &str) -> Option<String> {
    let bytes = body.as_bytes();
    let is_ip_char = |c: u8| c.is_ascii_digit() || c.is_ascii_hexdigit() || c == b'.' || c == b':';
    let mut i = 0;
    let mut candidates_v6: Vec<String> = Vec::new();
    while i < bytes.len() {
        if is_ip_char(bytes[i]) {
            let start = i;
            while i < bytes.len() && is_ip_char(bytes[i]) {
                i += 1;
            }
            let frag = &body[start..i];
            let len = frag.len();
            if (7..=15).contains(&len) && frag.contains('.') {
                if frag.parse::<std::net::Ipv4Addr>().is_ok() {
                    return Some(frag.to_string());
                }
            } else if (7..=45).contains(&len)
                && frag.contains(':')
                && frag.parse::<std::net::Ipv6Addr>().is_ok()
            {
                candidates_v6.push(frag.to_string());
            }
        } else {
            i += 1;
        }
    }
    candidates_v6.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_bare_ip() {
        assert_eq!(extract_ip("1.2.3.4").as_deref(), Some("1.2.3.4"));
        assert_eq!(extract_ip("  2600::1\n").as_deref(), Some("2600::1"));
    }

    #[test]
    fn extracts_ip_from_ipip_chinese_text() {
        // myip.ipip.net 的真实响应形态
        assert_eq!(
            extract_ip("当前 IP：112.10.23.45  来自于：中国 浙江 杭州  电信").as_deref(),
            Some("112.10.23.45")
        );
    }

    #[test]
    fn rejects_garbage_and_version_mismatch() {
        assert_eq!(extract_ip("hello world"), None);
        // v4 场景不该吐出 v6（extract_ip 优先 IPv4）
        assert_eq!(extract_ip("no ip here 999.999.999.999"), None);
    }
}
