//! ACME 证书自动续期：每小时检查一次，距到期 <30 天则续期。

use std::sync::Arc;
use std::time::Duration;

use tauri::AppHandle;

use crate::services::software_manager::SoftwareManager;
use crate::services::website_manager::WebsiteManager;

const CHECK_INTERVAL_SECS: u64 = 3600;
const RENEW_BEFORE_DAYS: i64 = 30;

/// 是否需要续期：缺失/不可解析 → true；已过期或距到期 < days → true。
pub fn needs_renewal(expires: Option<&str>, now: chrono::DateTime<chrono::Local>, days: i64) -> bool {
    let Some(s) = expires else { return true };
    let Ok(exp) = chrono::DateTime::parse_from_rfc3339(s) else { return true };
    exp.with_timezone(&chrono::Local) - now < chrono::Duration::days(days)
}

pub async fn run_scheduler(
    _app: AppHandle,
    wm: Arc<WebsiteManager>,
    sm: Arc<SoftwareManager>,
    dns_accounts: Arc<crate::services::dns_account::DnsAccountManager>,
) {
    let mut tick = tokio::time::interval(Duration::from_secs(CHECK_INTERVAL_SECS));
    tick.tick().await; // 消耗初始化 tick
    loop {
        tick.tick().await;
        let settings = match crate::commands::config::read_settings() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "读取设置失败，跳过本轮 ACME 续期");
                continue;
            }
        };
        let accounts = dns_accounts.list();

        // 每轮解析一次 nginx 与证书目录，避免逐站点重算
        let Ok(nginx) = crate::commands::website::resolve_nginx(&sm) else {
            tracing::warn!("未找到可用的 nginx 实例，跳过本轮 ACME 续期");
            continue;
        };
        let cert_dir = std::path::PathBuf::from(&nginx.install_path).join("sites-data").join("certs");

        for site in wm.list().into_iter().filter(|s| s.ssl.acme) {
            if !needs_renewal(site.ssl.cert_expires_at.as_deref(), chrono::Local::now(), RENEW_BEFORE_DAYS) {
                continue;
            }
            // 必须显式绑定账号：未绑/账号已删都跳过并留痕，
            // 绝不退回某个「默认」账号——那会签出用户没预期的证书。
            let Some(account) = site
                .ssl
                .dns_account_id
                .as_deref()
                .and_then(|id| accounts.iter().find(|a| a.id == id))
            else {
                tracing::warn!(
                    site = %site.id,
                    bound = ?site.ssl.dns_account_id,
                    "站点未绑定有效的 DNS 账号，跳过续期"
                );
                continue;
            };
            let acme = crate::services::acme::AcmeSettings {
                account: account.clone(),
                use_staging: settings.acme_use_staging,
            };
            let Some(raw) = site.server_name.clone() else { continue };
            let domain = match crate::commands::website::sanitize_domain(&raw) {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!(site = %site.id, error = %e, "域名不合法，跳过续期");
                    continue;
                }
            };
            match crate::services::acme::issue_certificate(&domain, &acme, &cert_dir, |_, _| {}).await {
                Ok(_) => {
                    if let Some(mut s) = wm.get(&site.id) {
                        // 与自签/手动签发一致存相对路径（绝对路径在 nginx 重装后会失效）
                        let base = format!("sites-data/certs/{}", domain);
                        s.ssl.cert_path = Some(format!("{base}.crt"));
                        s.ssl.key_path = Some(format!("{base}.key"));
                        s.ssl.cert_expires_at =
                            Some((chrono::Local::now() + chrono::Duration::days(90)).to_rfc3339());
                        if let Err(e) = wm.upsert_mem(s) {
                            tracing::warn!(site = %site.id, error = %e, "更新站点 SSL 配置失败");
                        }
                    }
                    if let Err(e) = wm.persist() {
                        tracing::warn!(site = %site.id, error = %e, "持久化站点配置失败");
                    }
                    // 审计要与实际一致：配置重载失败就不记为续期成功
                    match crate::commands::website::regenerate(&sm, &wm, true) {
                        Ok(_) => {
                            crate::oplog!("acme_renew", &format!("{} ({})", site.name, domain));
                        }
                        Err(e) => {
                            tracing::warn!(site = %site.id, error = %e, "续期后 nginx 配置重建/reload 失败");
                            crate::oplog!("acme_renew_failed", &format!("{} ({})", site.name, domain));
                        }
                    }
                }
                // {:#} 展开 error chain，便于定位真实原因（如 Cloudflare 权限/API 报错）
                Err(e) => tracing::warn!(site = %site.id, error = %format!("{:#}", e), "ACME 自动续期失败"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> chrono::DateTime<chrono::Local> {
        chrono::Local.with_ymd_and_hms(2026, 9, 11, 0, 0, 0).unwrap()
    }

    #[test]
    fn needs_renewal_rules() {
        // 缺失/不可解析 → true（无从确认，重签自愈）
        assert!(needs_renewal(None, now(), 30));
        assert!(needs_renewal(Some("not-a-date"), now(), 30));
        // 已过期 → true
        let past = (now() - chrono::Duration::days(1)).to_rfc3339();
        assert!(needs_renewal(Some(&past), now(), 30));
        // 距到期 <30 天 → true
        let soon = (now() + chrono::Duration::days(10)).to_rfc3339();
        assert!(needs_renewal(Some(&soon), now(), 30));
        // 充足 → false
        let far = (now() + chrono::Duration::days(80)).to_rfc3339();
        assert!(!needs_renewal(Some(&far), now(), 30));
    }

    /// 站点该用哪个账号：绑了就用绑的；没绑就是配置缺失，必须跳过而不是
    /// 拿某个默认账号去签（会给用户搞出意外的证书）。
    #[test]
    fn account_resolution_requires_explicit_binding() {
        let accounts = [crate::models::dns_account::DnsAccount {
            id: "acc-1".into(),
            name: "n".into(),
            provider: "cloudflare".into(),
            token: "t".into(),
            access_key_id: String::new(),
            access_key_secret: String::new(),
            zones: vec![],
            tested_at: None,
        }];
        let pick = |bound: Option<&str>| -> Option<String> {
            let id = bound?;
            accounts.iter().find(|a| a.id == id).map(|a| a.id.clone())
        };
        assert_eq!(pick(Some("acc-1")), Some("acc-1".into()));
        // 未绑定 → None（跳过并 warn）
        assert_eq!(pick(None), None);
        // 绑了但账号已删 → None（跳过并 warn，而不是退回默认）
        assert_eq!(pick(Some("gone")), None);
    }
}
