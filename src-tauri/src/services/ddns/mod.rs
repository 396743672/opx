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
#[derive(Debug)]
pub struct SyncResult {
    pub v4: String,
    pub v6: Option<String>,
    pub changes: Vec<String>,
    /// 失败记录条数（域名 × 记录类型）。>0 时 changes 里也有对应文案，但
    /// 上游（调度器日志、设置页报告）需要能直接判断整体成败，不必解析字符串。
    pub failures: usize,
}

/// 同步互斥：两路入口（调度器 / 「立即同步」命令）并发时若同一条 fqdn 都被
/// 读到「无记录」，会各自 POST 出重复记录，需人工清理。
/// `try_lock` 快速失败：抢不到说明另一轮正在跑，直接报错，不排队也不阻塞。
/// ponytail: 进程内单锁，多实例部署才需要跨进程方案。
static SYNC_GUARD: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
/// 全程持 `SYNC_GUARD`（锁覆盖 API 调用，这正是加锁的目的）；已有同步在跑时
/// 立即返回错误，不排队 —— 设置页点击需要即时可见的反馈，而非静默等待。
pub async fn sync_once(s: &crate::models::settings::AppSettings) -> anyhow::Result<SyncResult> {
    let _guard = SYNC_GUARD
        .try_lock()
        .map_err(|_| anyhow::anyhow!("已有同步进行中，请稍后再试"))?;
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
    let mut failures = 0usize;
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
                    failures += 1;
                    changes.push(format!("{}：失败 {}", fqdn, e));
                }
            }
        }
    }
    Ok(SyncResult {
        v4,
        v6,
        changes,
        failures,
    })
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

    /// 并发第二路必须立刻被拒（而非排队/静默）。
    /// 用默认设置（ddns_enabled=false）短路：同步体在拿到锁后马上 bail，
    /// 因此无需网络即可稳定区分「抢锁失败」与「业务失败」。
    #[tokio::test]
    async fn sync_once_rejects_second_concurrent_entry() {
        let s = crate::models::settings::AppSettings::default();
        let held = SYNC_GUARD.try_lock().expect("首轮应能拿到锁");
        let err = sync_once(&s).await.expect_err("已有同步时第二路必须报错");
        assert!(
            err.to_string().contains("已有同步进行中"),
            "错误文案应指向并发冲突，实际: {}",
            err
        );
        drop(held); // 释放后恢复可用
        assert!(SYNC_GUARD.try_lock().is_ok());
    }
}
