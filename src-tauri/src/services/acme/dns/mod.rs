//! DNS 服务商抽象（DNS-01 用）。v1 仅 Cloudflare，新增服务商只需加一个实现文件并在
//! `provider_for` 注册。
//!
//! 注意：需以 `Box<dyn DnsProvider>` 使用，而 trait 对象不支持 `async fn`（RPITIT 非
//! dyn 兼容），故统一返回 boxed future。

pub mod cloudflare;

use std::future::Future;
use std::pin::Pin;

use anyhow::Result;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait DnsProvider: Send + Sync {
    fn id(&self) -> &str;
    /// 返回 domain 所属 zone（注册域名），用于拼接记录全名。
    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, Result<String>>;
    fn create_txt<'a>(&'a self, fqdn: &'a str, value: &'a str) -> BoxFuture<'a, Result<()>>;
    fn delete_txt<'a>(&'a self, fqdn: &'a str, value: &'a str) -> BoxFuture<'a, Result<()>>;
}

/// 按 id 取服务商实现。
pub fn provider_for(id: &str, token: &str) -> Option<Box<dyn DnsProvider>> {
    match id {
        "cloudflare" => Some(Box::new(cloudflare::Cloudflare::new(token.to_string()))),
        _ => None,
    }
}

/// zones 中与 domain 匹配（相等或以 `.` 结尾后缀）的最长 zone。
pub fn longest_zone_match(domain: &str, zones: &[String]) -> Option<String> {
    let d = domain.trim_end_matches('.').to_lowercase();
    zones
        .iter()
        .filter(|z| {
            let zl = z.trim_end_matches('.').to_lowercase();
            d == zl || d.ends_with(&format!(".{zl}"))
        })
        .max_by_key(|z| z.len())
        .cloned()
}

/// DNS-01 记录全名。
pub fn acme_challenge_fqdn(domain: &str) -> String {
    format!("_acme-challenge.{}", domain.trim_end_matches('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longest_zone_match_picks_most_specific_suffix() {
        let zones = vec!["example.com".to_string(), "b.example.com".to_string(), "other.org".to_string()];
        assert_eq!(longest_zone_match("a.b.example.com", &zones).as_deref(), Some("b.example.com"));
        assert_eq!(longest_zone_match("x.example.com", &zones).as_deref(), Some("example.com"));
        assert_eq!(longest_zone_match("nope.net", &zones), None);
    }

    #[test]
    fn acme_challenge_fqdn_prefixes_label() {
        assert_eq!(acme_challenge_fqdn("example.com"), "_acme-challenge.example.com");
        assert_eq!(acme_challenge_fqdn("a.b.example.com"), "_acme-challenge.a.b.example.com");
    }
}
