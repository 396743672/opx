//! DNS 服务商抽象（DNS-01 与 DDNS 共用）。
//!
//! 五个方法在两个 trait 上同名同签名，`DdnsProvider` 只多一个带默认实现的
//! `sync_record`，且对 `dyn DnsProvider` 走 blanket impl —— 故四个服务商
//! 实现文件只需 `impl DnsProvider`，不必各写一遍同步逻辑。
//!
//! 注意：需以 `Box<dyn DnsProvider>` 使用，而 trait 对象不支持 `async fn`（RPITIT 非
//! dyn 兼容），故统一返回 boxed future。

pub mod cloudflare;

use std::future::Future;
use std::pin::Pin;

use anyhow::Result;

use crate::models::dns_account::DnsAccount;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait DnsProvider: Send + Sync {
    fn id(&self) -> &str;
    /// 该账户下所有 zone 名（「测试」按钮与域名归属确认用）
    fn list_zones<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>>>;
    /// 返回 domain 所属 zone（注册域名），用于拼接记录全名。
    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, Result<String>>;
    /// 读 name+type 的当前值（无记录 → None）
    ///
    /// 实现须返回**可直接比较**的值：Cloudflare 的 TXT content 带引号而
    /// A/AAAA 是裸 IP，故 TXT 应剥引号后再返回——否则 DDNS 的读-比较-写
    /// 会因 `"abc"` != `abc` 每轮误写一次。
    fn get_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str)
        -> BoxFuture<'a, Result<Option<String>>>;
    /// 无记录则新建，有则更新
    ///
    /// **TTL 由各实现自定**（TXT 挑战求快、DDNS 的 A/AAAA 宜长），不在此签名里
    /// 暴露；代价是同一条记录被 DDNS 与 ACME 交替写时会互相覆盖 TTL。
    fn set_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str, value: &'a str)
        -> BoxFuture<'a, Result<()>>;
    /// 删除 name+type 的记录；不存在时视为成功（幂等）
    fn delete_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str) -> BoxFuture<'a, Result<()>>;
}

/// 按账号（服务商 + 凭证）取实现；凭证缺失或服务商未知返回 None。
pub fn provider_for_account(a: &DnsAccount) -> Option<Box<dyn DnsProvider>> {
    match a.provider.as_str() {
        "cloudflare" if !a.token.trim().is_empty() => {
            Some(Box::new(cloudflare::Cloudflare::new(a.token.clone())))
        }
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

    /// 凭证缺失或服务商未知时必须返回 None（调用方据此报「请先配置凭证」）
    #[test]
    fn provider_for_account_rejects_missing_credentials() {
        let base = crate::models::dns_account::DnsAccount {
            id: "a1".into(),
            name: "n".into(),
            provider: "cloudflare".into(),
            token: String::new(),
            access_key_id: String::new(),
            access_key_secret: String::new(),
            zones: vec![],
            tested_at: None,
        };
        // cloudflare 需要 token
        assert!(provider_for_account(&base).is_none());

        let mut with_token = base.clone();
        with_token.token = "t".into();
        assert!(provider_for_account(&with_token).is_some());

        // 未知服务商
        let mut unknown = with_token.clone();
        unknown.provider = "nope".into();
        assert!(provider_for_account(&unknown).is_none());

        // aliyun / dnspod / huawei 的双凭证校验由 ddns::provider_for_account 负责
        // （见 T3）；此处只断「acme 侧不认识这三家时不得误放行」。
        // 正例断言（双凭证齐全 → Some）留到 T4 把三家改成 impl DnsProvider 后补，
        // 那时它们才结构上可能产出 Box<dyn DnsProvider>。
        // T4 备注：四家已 impl DnsProvider（结构上可产出），但 acme 侧工厂仍只 match
        // "cloudflare"（分派四家是 T7 的活）——T7 扩工厂后回来补四家正例断言。
        for p in ["aliyun", "dnspod", "huawei"] {
            let mut half = base.clone();
            half.provider = p.into();
            half.access_key_id = "k".into(); // 只有 id，没有 secret
            assert!(
                provider_for_account(&half).is_none(),
                "{} 在 acme 侧未注册，不应放行",
                p
            );
            let mut full = half.clone();
            full.access_key_secret = "s".into();
            assert!(
                provider_for_account(&full).is_none(),
                "{} 在 acme 侧未注册，凭证齐全也不应放行",
                p
            );
        }
    }
}
