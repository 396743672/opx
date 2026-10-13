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
///
/// ponytail: 另三家的具体类型在 `services::ddns`（T4 把四家统一到 `DnsProvider`）。
/// `Box<dyn DdnsProvider>` 无法转成 `Box<dyn DnsProvider>`，故这里按同一套凭证
/// 规则把三家再挂一次；规则若变要同步改 `services::ddns::provider_for_account`。
/// 升级路径：把 Cloudflare 也换成 ddns 那一份、两个工厂合一（先确认那边有剥
/// TXT 引号的逻辑）。
pub fn provider_for_account(a: &DnsAccount) -> Option<Box<dyn DnsProvider>> {
    match a.provider.as_str() {
        "cloudflare" if !a.token.trim().is_empty() => {
            Some(Box::new(cloudflare::Cloudflare::new(a.token.clone())))
        }
        "aliyun" if has_key_pair(a) => Some(Box::new(crate::services::ddns::aliyun::Aliyun::new(
            a.access_key_id.clone(),
            a.access_key_secret.clone(),
        ))),
        "dnspod" if has_key_pair(a) => Some(Box::new(
            crate::services::ddns::dnspod::Dnspod::new(
                a.access_key_id.clone(),
                a.access_key_secret.clone(),
            ),
        )),
        "huawei" if has_key_pair(a) => Some(Box::new(crate::services::ddns::huawei::Huawei::new(
            a.access_key_id.clone(),
            a.access_key_secret.clone(),
        ))),
        _ => None,
    }
}

fn has_key_pair(a: &DnsAccount) -> bool {
    !a.access_key_id.trim().is_empty() && !a.access_key_secret.trim().is_empty()
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

        // aliyun / dnspod / huawei：凭证不全不放行，齐全放行
        // （具体类型挂在 ddns 那三家上，见 provider_for_account 的注释）
        for p in ["aliyun", "dnspod", "huawei"] {
            let mut half = base.clone();
            half.provider = p.into();
            half.access_key_id = "k".into(); // 只有 id，没有 secret
            assert!(
                provider_for_account(&half).is_none(),
                "{} 缺 secret 不应放行",
                p
            );
            let mut full = half.clone();
            full.access_key_secret = "s".into();
            assert!(
                provider_for_account(&full).is_some(),
                "{} 双凭证齐全应放行",
                p
            );
        }
    }
}
