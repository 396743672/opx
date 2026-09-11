//! ACME（Let's Encrypt）DNS-01 签发与续期。

pub mod dns;
pub mod renew_scheduler;

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use instant_acme::{
    Account, AccountCredentials, ChallengeType, Identifier, LetsEncrypt, NewAccount, NewOrder,
    RetryPolicy,
};

use dns::provider_for;

pub struct AcmeSettings {
    pub dns_provider: String,
    pub cloudflare_api_token: String,
    pub use_staging: bool,
}

/// ACME 账户凭据持久化路径（续期进程共用一个账户）。
pub fn account_path() -> PathBuf {
    crate::utils::paths::data_dir().join("acme-account.json")
}

pub fn directory_url(staging: bool) -> String {
    if staging {
        LetsEncrypt::Staging.url().to_string()
    } else {
        LetsEncrypt::Production.url().to_string()
    }
}

pub fn cert_paths(dir: &Path, domain: &str) -> (PathBuf, PathBuf) {
    (
        dir.join(format!("{}.crt", domain)),
        dir.join(format!("{}.key", domain)),
    )
}

/// 签发（或重新签发）指定域名的证书，返回 (cert_path, key_path)。
pub async fn issue_certificate(
    domain: &str,
    settings: &AcmeSettings,
    cert_dir: &Path,
    on_progress: impl Fn(&str, &str) + Send + Sync,
) -> Result<(PathBuf, PathBuf)> {
    let provider = provider_for(&settings.dns_provider, &settings.cloudflare_api_token)
        .ok_or_else(|| anyhow!("未配置 DNS 服务商或服务商不支持：{}", settings.dns_provider))?;

    std::fs::create_dir_all(cert_dir).context("创建证书目录失败")?;

    on_progress("creating-order", "创建 ACME 订单");
    let account = load_or_create_account(settings.use_staging).await?;
    let mut order = account
        .new_order(&NewOrder::new(&[Identifier::Dns(domain.to_string())]))
        .await?;

    // 记录本次创建的所有挑战记录，验证结束后统一清理
    let mut created: Vec<(String, String)> = Vec::new();

    let mut authorizations = order.authorizations();
    while let Some(result) = authorizations.next().await {
        let mut authz = result?;
        let Some(mut challenge) = authz.challenge(ChallengeType::Dns01) else {
            return Err(anyhow!("该 ACME 服务器未提供 DNS-01 challenge"));
        };
        let value = challenge.key_authorization().dns_value();
        let fqdn = dns::acme_challenge_fqdn(domain);
        on_progress("waiting-dns", &format!("写入 TXT 记录 {}", fqdn));
        if let Err(e) = provider.create_txt(&fqdn, &value).await {
            for (f, v) in &created {
                let _ = provider.delete_txt(f, v).await;
            }
            return Err(e).context("创建 DNS 挑战记录失败");
        }
        created.push((fqdn, value));
        // 传播等待；ACME 轮询会重试兜底
        tokio::time::sleep(Duration::from_secs(dns::cloudflare::DNS_PROPAGATION_WAIT_SECS)).await;
        on_progress("validating", "等待 ACME 验证");
        if let Err(e) = challenge.set_ready().await {
            for (f, v) in &created {
                let _ = provider.delete_txt(f, v).await;
            }
            return Err(e).context("提交 DNS-01 challenge 失败");
        }
    }

    // 验证（此时 DNS 记录必须仍在）
    let poll_result = order.poll_ready(&RetryPolicy::default()).await;

    // 无论验证成败，验证结束后统一清理 TXT
    for (f, v) in &created {
        let _ = provider.delete_txt(f, v).await;
    }
    poll_result?;

    on_progress("downloading", "签发完成，下载证书");
    let key_pem = order.finalize().await?;
    let cert_pem = order.poll_certificate(&RetryPolicy::default()).await?;

    let (cert_path, key_path) = cert_paths(cert_dir, domain);
    std::fs::write(&cert_path, cert_pem).context("写入证书失败")?;
    std::fs::write(&key_path, key_pem).context("写入私钥失败")?;
    on_progress("done", "证书已保存");
    Ok((cert_path, key_path))
}

async fn load_or_create_account(staging: bool) -> Result<Account> {
    let dir_url = directory_url(staging);
    let path = account_path();
    if let Ok(raw) = std::fs::read_to_string(&path) {
        // directory 字段为 pub(crate) 无法直接访问，序列化后的 JSON 中含明文 "directory"
        let stored_dir = serde_json::from_str::<serde_json::Value>(&raw)
            .ok()
            .and_then(|v| v.get("directory")?.as_str().map(str::to_owned));
        if stored_dir.as_deref() == Some(dir_url.as_str()) {
            let creds = serde_json::from_str::<AccountCredentials>(&raw)?;
            return Ok(Account::builder()?.from_credentials(creds).await?);
        }
        tracing::warn!("ACME 账户凭据目录不匹配（staging 切换），将重新注册账户");
    }
    let (account, creds) = Account::builder()?
        .create(
            &NewAccount {
                contact: &[],
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            dir_url,
            None,
        )
        .await?;
    match serde_json::to_string_pretty(&creds) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                tracing::warn!(error = %e, "写入 ACME 账户凭据失败");
            }
        }
        Err(e) => tracing::warn!(error = %e, "序列化 ACME 账户凭据失败"),
    }
    Ok(account)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_url_switches_on_staging() {
        assert!(directory_url(true).contains("staging"));
        assert!(!directory_url(false).contains("staging"));
    }

    #[test]
    fn cert_paths_use_domain() {
        let dir = std::path::Path::new("C:/tmp/certs");
        let (c, k) = cert_paths(dir, "example.com");
        assert!(c.ends_with("example.com.crt"));
        assert!(k.ends_with("example.com.key"));
    }
}
