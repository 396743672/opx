//! DDNS 动态域名：公网 IP 检测 + 服务商 A/AAAA 同步。
//!
//! 注意：需以 `Box<dyn DdnsProvider>` 使用，而 trait 对象不支持 `async fn`（RPITIT 非
//! dyn 兼容），故统一返回 boxed future（同 `acme::dns`）。

pub mod aliyun;
pub mod cloudflare;
pub mod dnspod;
pub mod huawei;
pub mod ip;
pub mod scheduler;

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

/// 一轮同步的结果（报告文案用；变更条目已在 `sync_once` 内写审计）。
pub struct SyncResult {
    pub v4: String,
    pub v6: Option<String>,
    pub changes: Vec<String>,
}

/// 同步目标：去空白、转小写、丢空项并去重（顺序保持）。
fn normalize_domains(d: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for x in d {
        let x = x.trim().to_lowercase();
        if !x.is_empty() && !out.contains(&x) {
            out.push(x);
        }
    }
    out
}

/// 记录是否需要写：「未变」以外的一切动作都算变更。
fn is_change(action: &str) -> bool {
    action != "未变"
}

/// 一轮完整同步：检测公网 IP → 逐域名同步 A（+AAAA）。
/// 实际发生变更的条目写审计；返回 IP 与变更列表（报告用）。
/// 单个域名失败只记审计并继续下一个，不中断整轮。
pub async fn sync_once(s: &crate::models::settings::AppSettings) -> anyhow::Result<SyncResult> {
    if !s.ddns_enabled {
        anyhow::bail!("DDNS 未启用");
    }
    let domains = normalize_domains(&s.ddns_domains);
    if domains.is_empty() {
        anyhow::bail!("未配置域名");
    }
    let provider = provider_for(s)
        .ok_or_else(|| anyhow::anyhow!("DDNS 服务商凭证未配置（{}）", s.ddns_provider))?;
    let v4 = ip::detect_public_ip(false).await?;
    let v6 = if s.ddns_enable_ipv6 {
        match ip::detect_public_ip(true).await {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(error = %e, "IPv6 检测失败（跳过 AAAA 同步）");
                None
            }
        }
    } else {
        None
    };
    // 本轮要写的记录：A 必有，AAAA 仅在 v6 检测成功时
    let mut targets: Vec<(&str, String)> = vec![("A", v4.clone())];
    if let Some(v) = &v6 {
        targets.push(("AAAA", v.clone()));
    }
    let mut changes = Vec::new();
    for fqdn in &domains {
        for (rtype, ip) in &targets {
            match provider.sync_record(fqdn, rtype, ip).await {
                Ok(action) if is_change(&action) => {
                    crate::oplog!("ddns_update", fqdn, &action);
                    changes.push(format!("{}：{}", fqdn, action));
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(error = %e, fqdn = %fqdn, rtype = %rtype, "DDNS 记录同步失败");
                    crate::oplog_fail!("ddns_update", fqdn, "同步失败", &format!("{:#}", e));
                    changes.push(format!("{}：失败 {}", fqdn, e));
                }
            }
        }
    }
    Ok(SyncResult { v4, v6, changes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_domains_trims_lowercases_and_dedups() {
        let raw = vec![
            "  Example.COM ".to_string(),
            "example.com".to_string(),
            "".to_string(),
            "   ".to_string(),
            "sub.example.com".to_string(),
            "sub.EXAMPLE.com".to_string(),
        ];
        assert_eq!(
            normalize_domains(&raw),
            vec!["example.com".to_string(), "sub.example.com".to_string()]
        );
        assert!(normalize_domains(&[]).is_empty());
        // 全是空白的列表归一化后为空 → sync_once 会报「未配置域名」
        assert!(normalize_domains(&["  ".to_string()]).is_empty());
    }

    #[test]
    fn is_change_skips_unchanged_action_only() {
        assert!(!is_change("未变"));
        assert!(is_change("A 记录新建 1.2.3.4"));
        assert!(is_change("A 记录更新 1.2.3.4 → 5.6.7.8"));
    }
}
