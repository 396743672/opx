//! Cloudflare DNS API（Bearer Token）实现 DNS-01 的 TXT 记录增删。

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::{longest_zone_match, BoxFuture, DnsProvider};

const API: &str = "https://api.cloudflare.com/client/v4";
/// DNS 传播等待：新建记录后固定等待，随后由 ACME 轮询兜底。
pub const DNS_PROPAGATION_WAIT_SECS: u64 = 15;

pub struct Cloudflare {
    token: String,
    client: reqwest::Client,
}

impl Cloudflare {
    pub fn new(token: String) -> Self {
        Self { token, client: reqwest::Client::new() }
    }

    async fn get(&self, url: &str) -> Result<Value> {
        let resp = self.client.get(url).bearer_auth(&self.token).send().await?;
        Self::json(resp).await
    }

    async fn json(resp: reqwest::Response) -> Result<Value> {
        let status = resp.status();
        // 先取文本再尽力解析：非 JSON 错误体（如网关 HTML）也能把原文带出来
        let text = resp.text().await.unwrap_or_default();
        let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if !status.is_success() || body.get("success").and_then(|v| v.as_bool()) != Some(true) {
            // Cloudflare 10000 = Authentication error：绝大多数是 Token 缺少 Zone→DNS→Edit 权限
            let hint = body["errors"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|e| e["code"].as_i64())
                .filter(|c| *c == 10000)
                .map(|_| "（Token 可能缺少 Zone → DNS → Edit 权限，请在 Cloudflare 编辑该 Token 时补上）")
                .unwrap_or("");
            let detail = if body.is_null() { text } else { body.to_string() };
            return Err(anyhow!("Cloudflare API 失败（{}）：{}{}", status, detail, hint));
        }
        Ok(body)
    }

    async fn zone_id(&self, zone: &str) -> Result<String> {
        let body = self.get(&format!("{}/zones?name={}", API, zone)).await?;
        body["result"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|z| z["id"].as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("未找到 zone id：{}", zone))
    }
}

impl DnsProvider for Cloudflare {
    fn id(&self) -> &str {
        "cloudflare"
    }

    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move {
            // ponytail: 分页上限，防账户 zone 过多时反复全量列举；20 * 50 = 1000 个 zone，
            // 超出请改用按 name 过滤查询
            const MAX_PAGES: u32 = 20;
            let mut names: Vec<String> = Vec::new();
            let mut page = 1u32;
            loop {
                let body = self
                    .get(&format!("{}/zones?per_page=50&page={}", API, page))
                    .await?;
                let arr = body["result"].as_array().cloned().unwrap_or_default();
                if arr.is_empty() {
                    break;
                }
                for z in &arr {
                    if let Some(n) = z["name"].as_str() {
                        names.push(n.to_string());
                    }
                }
                if arr.len() < 50 || page >= MAX_PAGES {
                    break;
                }
                page += 1;
            }
            longest_zone_match(domain, &names)
                .ok_or_else(|| anyhow!("该域名不在 Cloudflare 账户的 zone 中：{}", domain))
        })
    }

    fn create_txt<'a>(&'a self, fqdn: &'a str, value: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let zone = self.find_zone(fqdn.trim_start_matches("_acme-challenge.")).await?;
            let zone_id = self.zone_id(&zone).await?;
            let body = serde_json::json!({ "type": "TXT", "name": fqdn, "content": value, "ttl": 120 });
            let resp = self
                .client
                .post(format!("{}/zones/{}/dns_records", API, zone_id))
                .bearer_auth(&self.token)
                .json(&body)
                .send()
                .await?;
            Self::json(resp).await.map(|_| ())
        })
    }

    fn delete_txt<'a>(&'a self, fqdn: &'a str, value: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let zone = self.find_zone(fqdn.trim_start_matches("_acme-challenge.")).await?;
            let zone_id = self.zone_id(&zone).await?;
            let body = self
                .get(&format!(
                    "{}/zones/{}/dns_records?type=TXT&name={}",
                    API, zone_id, fqdn
                ))
                .await?;
            if let Some(records) = body["result"].as_array() {
                for r in records {
                    if r["content"].as_str() == Some(value) {
                        if let Some(id) = r["id"].as_str() {
                            let _ = self
                                .client
                                .delete(format!("{}/zones/{}/dns_records/{}", API, zone_id, id))
                                .bearer_auth(&self.token)
                                .send()
                                .await;
                        }
                    }
                }
            }
            Ok(())
        })
    }
}
