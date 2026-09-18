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
    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, anyhow::Result<String>>;
    fn get_value<'a>(
        &'a self,
        fqdn: &'a str,
        rtype: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<Option<String>>>;
    fn set_value<'a>(
        &'a self,
        fqdn: &'a str,
        rtype: &'a str,
        value: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<()>>;

    /// 把 fqdn 的 rtype("A"/"AAAA") 记录同步为 ip。返回动作文案：
    /// "A 记录新建 1.2.3.4" / "A 记录更新 old → new" / "未变"
    fn sync_record<'a>(
        &'a self,
        fqdn: &'a str,
        rtype: &'a str,
        ip: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<String>> {
        Box::pin(async move {
            match self.get_value(fqdn, rtype).await? {
                Some(old) if old == ip => Ok("未变".to_string()),
                Some(old) => {
                    self.set_value(fqdn, rtype, ip).await?;
                    Ok(format!("{} 记录更新 {} → {}", rtype, old, ip))
                }
                None => {
                    self.set_value(fqdn, rtype, ip).await?;
                    Ok(format!("{} 记录新建 {}", rtype, ip))
                }
            }
        })
    }
}

/// 唯一的 blanket impl：任何 DnsProvider 自动是 DdnsProvider。
/// （不要额外为具体类型写 `impl DdnsProvider for X`，会与这条冲突。）
///
/// 方法体**必须逐个显式转发**：空实现 `impl<T: DnsProvider> DdnsProvider for T {}`
/// 不会自动继承同名方法（编译器只看到「未实现」的 E0046），必须给出到达
/// `DnsProvider` 同名方法的完整路径。
impl<T: crate::services::acme::dns::DnsProvider + ?Sized> DdnsProvider for T {
    fn id(&self) -> &str {
        crate::services::acme::dns::DnsProvider::id(self)
    }
    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, anyhow::Result<String>> {
        crate::services::acme::dns::DnsProvider::find_zone(self, domain)
    }
    fn get_value<'a>(
        &'a self,
        fqdn: &'a str,
        rtype: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<Option<String>>> {
        crate::services::acme::dns::DnsProvider::get_value(self, fqdn, rtype)
    }
    fn set_value<'a>(
        &'a self,
        fqdn: &'a str,
        rtype: &'a str,
        value: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<()>> {
        crate::services::acme::dns::DnsProvider::set_value(self, fqdn, rtype, value)
    }
}

/// 按账号取实现；凭证缺失返回 None（调用方给出「请先配置凭证」错误）。
pub fn provider_for_account(
    a: &crate::models::dns_account::DnsAccount,
) -> Option<Box<dyn DdnsProvider>> {
    match a.provider.as_str() {
        "cloudflare" if !a.token.trim().is_empty() => Some(Box::new(
            cloudflare::Cloudflare::new(a.token.clone()),
        )),
        "aliyun"
            if !a.access_key_id.trim().is_empty() && !a.access_key_secret.trim().is_empty() =>
        {
            Some(Box::new(aliyun::Aliyun::new(
                a.access_key_id.clone(),
                a.access_key_secret.clone(),
            )))
        }
        "dnspod"
            if !a.access_key_id.trim().is_empty() && !a.access_key_secret.trim().is_empty() =>
        {
            Some(Box::new(dnspod::Dnspod::new(
                a.access_key_id.clone(),
                a.access_key_secret.clone(),
            )))
        }
        "huawei"
            if !a.access_key_id.trim().is_empty() && !a.access_key_secret.trim().is_empty() =>
        {
            Some(Box::new(huawei::Huawei::new(
                a.access_key_id.clone(),
                a.access_key_secret.clone(),
            )))
        }
        _ => None,
    }
}

/// 把设置页的 `ddns_*` 字段折成 `DnsAccount`，供 `sync_once` 用。
///
/// **这是刻意的适配层**：DDNS 的凭证字段与证书账号解耦（用户明确要求「证书可以
/// 是其他家的」），DDNS 不引入账号概念，只是复用同一个 provider 工厂。
pub fn ddns_account_of(s: &crate::models::settings::AppSettings) -> crate::models::dns_account::DnsAccount {
    crate::models::dns_account::DnsAccount {
        id: String::new(),
        name: "ddns".into(),
        provider: s.ddns_provider.clone(),
        token: s.ddns_cloudflare_token.clone(),
        access_key_id: match s.ddns_provider.as_str() {
            "aliyun" => s.ddns_aliyun_access_key_id.clone(),
            "dnspod" => s.ddns_dnspod_secret_id.clone(),
            "huawei" => s.ddns_huawei_access_key.clone(),
            _ => String::new(),
        },
        access_key_secret: match s.ddns_provider.as_str() {
            "aliyun" => s.ddns_aliyun_access_key_secret.clone(),
            "dnspod" => s.ddns_dnspod_secret_key.clone(),
            "huawei" => s.ddns_huawei_secret_key.clone(),
            _ => String::new(),
        },
        zones: Vec::new(),
        tested_at: None,
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
pub async fn sync_once(
    s: &crate::models::settings::AppSettings,
    account: &crate::models::dns_account::DnsAccount,
) -> anyhow::Result<SyncResult> {
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
    // 凭证由调用方折成 DnsAccount 传入（DDNS 的 ddns_* 字段与证书账号刻意解耦）。
    let provider = provider_for_account(account)
        .ok_or_else(|| anyhow::anyhow!("DDNS 服务商凭证未配置（{}）", account.provider))?;
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
        let account = ddns_account_of(&s);
        let held = SYNC_GUARD.try_lock().expect("首轮应能拿到锁");
        let err = sync_once(&s, &account)
            .await
            .expect_err("已有同步时第二路必须报错");
        assert!(
            err.to_string().contains("已有同步进行中"),
            "错误文案应指向并发冲突，实际: {}",
            err
        );
        drop(held); // 释放后恢复可用
        assert!(SYNC_GUARD.try_lock().is_ok());
    }

    /// sync_record 的默认实现必须走「读-比较-写」三分支：
    /// 无记录 → 新建文案；值不同 → 更新文案；值相同 → "未变"。
    /// 用一个只记账的假 provider 验证，不碰网络。
    struct Fake {
        cur: Option<String>,
        /// `Mutex` 而非 `RefCell`：`DnsProvider: Sync`，`RefCell` 过不了 Send/Sync 约束。
        writes: std::sync::Mutex<Vec<String>>,
    }

    impl crate::services::acme::dns::DnsProvider for Fake {
        fn id(&self) -> &str {
            "fake"
        }
        fn list_zones<'a>(
            &'a self,
        ) -> crate::services::acme::dns::BoxFuture<'a, anyhow::Result<Vec<String>>> {
            Box::pin(async { Ok(vec!["example.com".to_string()]) })
        }
        fn find_zone<'a>(
            &'a self,
            _domain: &'a str,
        ) -> crate::services::acme::dns::BoxFuture<'a, anyhow::Result<String>> {
            Box::pin(async { Ok("example.com".to_string()) })
        }
        fn get_value<'a>(
            &'a self,
            _fqdn: &'a str,
            _rtype: &'a str,
        ) -> crate::services::acme::dns::BoxFuture<'a, anyhow::Result<Option<String>>> {
            Box::pin(async move { Ok(self.cur.clone()) })
        }
        fn set_value<'a>(
            &'a self,
            _fqdn: &'a str,
            _rtype: &'a str,
            value: &'a str,
        ) -> crate::services::acme::dns::BoxFuture<'a, anyhow::Result<()>> {
            Box::pin(async move {
                self.writes.lock().unwrap().push(value.to_string());
                Ok(())
            })
        }
        fn delete_value<'a>(
            &'a self,
            _fqdn: &'a str,
            _rtype: &'a str,
        ) -> crate::services::acme::dns::BoxFuture<'a, anyhow::Result<()>> {
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn sync_record_default_impl_three_branches() {
        // 无记录 → 新建
        let p = Fake { cur: None, writes: Default::default() };
        let action = DdnsProvider::sync_record(&p, "a.example.com", "A", "1.2.3.4")
            .await
            .unwrap();
        assert_eq!(action, "A 记录新建 1.2.3.4");
        assert_eq!(p.writes.lock().unwrap().as_slice(), ["1.2.3.4"]);

        // 值不同 → 更新
        let p = Fake { cur: Some("9.9.9.9".into()), writes: Default::default() };
        let action = DdnsProvider::sync_record(&p, "a.example.com", "A", "1.2.3.4")
            .await
            .unwrap();
        assert_eq!(action, "A 记录更新 9.9.9.9 → 1.2.3.4");
        assert_eq!(p.writes.lock().unwrap().as_slice(), ["1.2.3.4"]);

        // 值相同 → 未变，且绝不写
        let p = Fake { cur: Some("1.2.3.4".into()), writes: Default::default() };
        let action = DdnsProvider::sync_record(&p, "a.example.com", "A", "1.2.3.4")
            .await
            .unwrap();
        assert_eq!(action, "未变");
        assert!(p.writes.lock().unwrap().is_empty(), "值未变时不应发起写入");
    }
}
