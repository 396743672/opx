//! 公网出口 IP 检测：多源轮询，任一成功即用。
//!
//! 源必须是**该家族专用**的：如 myip.ipip.net 返回「出口 IP」，双栈网络下优先给 IPv6，
//! 拿它当 v4 源会永久失败（实测：上海电信双栈 → 只吐 240e:...）。
//! 故 v4 用 ipv4.* 专用域，v6 用 ipv6.* / api6.*。

use anyhow::{anyhow, Result};
use std::net::{Ipv4Addr, Ipv6Addr};

/// v4 源：全部为「强制 IPv4」端点，避免双栈机器拿到 AAAA
const SOURCES_V4: &[&str] = &[
    "https://ipv4.icanhazip.com",
    "https://ipv4.ident.me",
    "https://ifconfig.me/ip",
];
const SOURCES_V6: &[&str] = &[
    "https://ipv6.icanhazip.com",
    "https://ipv6.ident.me",
    "https://api6.ipify.org",
];

/// 依次请求源列表；提取失败换下一源，全失败时汇总**每一个**源的原因。
pub async fn detect_public_ip(v6: bool) -> Result<String> {
    let sources: &[&str] = if v6 { SOURCES_V6 } else { SOURCES_V4 };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    // 收集每个源的原因：只留最后一个会掩盖「首个源有内容但家族不符」这类真问题
    let mut errs: Vec<String> = Vec::new();
    for url in sources {
        match client.get(*url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body = resp.text().await.unwrap_or_default();
                if let Some(ip) = extract_ip(&body) {
                    // 家族校验用 parse 而非字符嗅探：::ffff:1.2.3.4 这类形态含点也含冒号，
                    // 嗅探会把它误判为非 v4
                    let family_ok = if v6 {
                        ip.parse::<Ipv6Addr>().is_ok()
                    } else {
                        ip.parse::<Ipv4Addr>().is_ok()
                    };
                    if family_ok {
                        return Ok(ip);
                    }
                    errs.push(format!(
                        "{} 返回了非 {} 地址：{}",
                        url,
                        if v6 { "IPv6" } else { "IPv4" },
                        ip
                    ));
                    continue;
                }
                errs.push(format!("{} 响应无有效 IP", url));
            }
            Ok(resp) => errs.push(format!("{} 返回 {}", url, resp.status())),
            Err(e) => errs.push(format!("{} 请求失败: {}", url, e)),
        }
    }
    Err(anyhow!("公网 IP 检测失败：{}", errs.join("；")))
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
