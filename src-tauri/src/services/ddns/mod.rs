//! DDNS 动态域名：公网 IP 检测 + 服务商 A/AAAA 同步。
//!
//! 注意：需以 `Box<dyn DdnsProvider>` 使用，而 trait 对象不支持 `async fn`（RPITIT 非
//! dyn 兼容），故统一返回 boxed future（同 `acme::dns`）。

pub mod aliyun;
pub mod cloudflare;
pub mod dnspod;
pub mod huawei;
pub mod ip;
// Task 7: pub mod scheduler;

use std::future::Future;
use std::pin::Pin;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait DdnsProvider: Send + Sync {
    fn id(&self) -> &str;
    /// 把 fqdn 的 rtype("A"/"AAAA") 记录同步为 ip。返回动作文案：
    /// "A 记录新建 1.2.3.4" / "A 记录更新 old → new" / "未变"
    fn sync_record<'a>(
        &'a self,
        fqdn: &'a str,
        rtype: &'a str,
        ip: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<String>>;
}

/// 按 settings.ddns_provider + 凭证取实现；凭证缺失返回 None
/// （调用方给出「请先配置凭证」错误）。
pub fn provider_for(s: &crate::models::settings::AppSettings) -> Option<Box<dyn DdnsProvider>> {
    match s.ddns_provider.as_str() {
        "cloudflare" if !s.ddns_cloudflare_token.trim().is_empty() => Some(Box::new(
            cloudflare::Cloudflare::new(s.ddns_cloudflare_token.clone()),
        )),
        "aliyun"
            if !s.ddns_aliyun_access_key_id.trim().is_empty()
                && !s.ddns_aliyun_access_key_secret.trim().is_empty() =>
        {
            Some(Box::new(aliyun::Aliyun::new(
                s.ddns_aliyun_access_key_id.clone(),
                s.ddns_aliyun_access_key_secret.clone(),
            )))
        }
        "dnspod"
            if !s.ddns_dnspod_secret_id.trim().is_empty()
                && !s.ddns_dnspod_secret_key.trim().is_empty() =>
        {
            Some(Box::new(dnspod::Dnspod::new(
                s.ddns_dnspod_secret_id.clone(),
                s.ddns_dnspod_secret_key.clone(),
            )))
        }
        "huawei"
            if !s.ddns_huawei_access_key.trim().is_empty()
                && !s.ddns_huawei_secret_key.trim().is_empty() =>
        {
            Some(Box::new(huawei::Huawei::new(
                s.ddns_huawei_access_key.clone(),
                s.ddns_huawei_secret_key.clone(),
            )))
        }
        _ => None,
    }
}
