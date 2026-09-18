//! Cloudflare DDNS：Bearer Token，GET/POST/PATCH /zones/{zid}/dns_records。

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::services::acme::dns::{BoxFuture, DnsProvider};

const API: &str = "https://api.cloudflare.com/client/v4";

pub struct Cloudflare {
    token: String,
    client: reqwest::Client,
}

impl Cloudflare {
    pub fn new(token: String) -> Self {
        Self {
            token,
            client: crate::utils::http::client(),
        }
    }

    async fn json(&self, resp: reqwest::Response) -> Result<Value> {
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
                .map(|_| {
                    "（需要【区域(Zone)作用域】的 DNS → Edit 权限；账户作用域的 DNS View/Settings 不能替代。\
最简单做法：Cloudflare → API Tokens → Create Token → 选「Edit zone DNS」模板 → Zone Resources 选目标域名）"
                })
                .unwrap_or("");
            let detail = if body.is_null() {
                text
            } else {
                body.to_string()
            };
            return Err(anyhow!(
                "Cloudflare API 失败（{}）：{}{}",
                status,
                detail,
                hint
            ));
        }
        Ok(body)
    }

    async fn get(&self, url: &str) -> Result<Value> {
        self.json(self.client.get(url).bearer_auth(&self.token).send().await?)
            .await
    }

    /// zone 名 → zone id
    async fn zone_id(&self, zone: &str) -> Result<String> {
        let body = self
            .get(&format!("{}/zones?name={}", API, zone))
            .await?;
        body["result"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .first()
            .and_then(|z| z["id"].as_str().map(String::from))
            .ok_or_else(|| anyhow!("未找到 zone id：{}", zone))
    }
}

impl DnsProvider for Cloudflare {
    fn id(&self) -> &str {
        "cloudflare"
    }

    fn list_zones<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
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
            Ok(names)
        })
    }

    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move {
            let names = self.list_zones().await?;
            crate::services::acme::dns::longest_zone_match(domain, &names)
                .ok_or_else(|| anyhow!("该域名不在 Cloudflare 账户的 zone 中：{}", domain))
        })
    }

    fn get_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str)
        -> BoxFuture<'a, Result<Option<String>>>
    {
        Box::pin(async move {
            let zone = self.find_zone(fqdn).await?;
            let zone_id = self.zone_id(&zone).await?;
            let body = self
                .get(&format!(
                    "{}/zones/{}/dns_records?type={}&name={}",
                    API, zone_id, rtype, fqdn
                ))
                .await?;
            Ok(body["result"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|r| r["content"].as_str())
                .map(|s| s.to_string()))
        })
    }

    fn set_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str, value: &'a str)
        -> BoxFuture<'a, Result<()>>
    {
        Box::pin(async move {
            let zone = self.find_zone(fqdn).await?;
            let zone_id = self.zone_id(&zone).await?;
            let list = self
                .get(&format!(
                    "{}/zones/{}/dns_records?type={}&name={}",
                    API, zone_id, rtype, fqdn
                ))
                .await?;
            let existing = list["result"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|r| r["id"].as_str())
                .map(|s| s.to_string());
            let body = serde_json::json!({
                "type": rtype, "name": fqdn, "content": value, "ttl": 120
            });
            let resp = match existing {
                Some(rid) => {
                    self.client
                        .put(format!("{}/zones/{}/dns_records/{}", API, zone_id, rid))
                        .bearer_auth(&self.token)
                        .json(&body)
                        .send()
                        .await?
                }
                None => {
                    self.client
                        .post(format!("{}/zones/{}/dns_records", API, zone_id))
                        .bearer_auth(&self.token)
                        .json(&body)
                        .send()
                        .await?
                }
            };
            self.json(resp).await.map(|_| ())
        })
    }

    fn delete_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str)
        -> BoxFuture<'a, Result<()>>
    {
        Box::pin(async move {
            let zone = self.find_zone(fqdn).await?;
            let zone_id = self.zone_id(&zone).await?;
            let body = self
                .get(&format!(
                    "{}/zones/{}/dns_records?type={}&name={}",
                    API, zone_id, rtype, fqdn
                ))
                .await?;
            if let Some(records) = body["result"].as_array() {
                for r in records {
                    let Some(id) = r["id"].as_str() else { continue };
                    match self
                        .client
                        .delete(format!("{}/zones/{}/dns_records/{}", API, zone_id, id))
                        .bearer_auth(&self.token)
                        .send()
                        .await
                    {
                        Ok(resp) if resp.status().is_success() => {}
                        Ok(resp) => tracing::warn!(
                            status = %resp.status(), name = %fqdn, "删除 DNS 记录失败（响应非成功）"
                        ),
                        Err(e) => tracing::warn!(
                            error = %e, name = %fqdn, "删除 DNS 记录请求失败"
                        ),
                    }
                }
            }
            Ok(())
        })
    }
}
