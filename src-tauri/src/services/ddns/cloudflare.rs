//! Cloudflare DDNS：Bearer Token，GET/POST/PATCH /zones/{zid}/dns_records。

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::{BoxFuture, DdnsProvider};

const API: &str = "https://api.cloudflare.com/client/v4";

pub struct Cloudflare {
    token: String,
    client: reqwest::Client,
}

impl Cloudflare {
    pub fn new(token: String) -> Self {
        Self {
            token,
            client: reqwest::Client::new(),
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

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<Value> {
        self.json(req.bearer_auth(&self.token).send().await?).await
    }

    /// 列账户 zone 名并取最长匹配。
    /// ponytail: 只查首页 50 个 zone；账户 zone 数超过 50 时请改为 ?name= 精确查询。
    async fn find_zone(&self, fqdn: &str) -> Result<String> {
        let body = self
            .send(self.client.get(format!("{}/zones?per_page=50", API)))
            .await?;
        let names: Vec<String> = body["result"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|z| z["name"].as_str().map(String::from))
            .collect();
        crate::services::acme::dns::longest_zone_match(fqdn, &names)
            .ok_or_else(|| anyhow!("该域名不在 Cloudflare 账户的 zone 中：{}", fqdn))
    }
}

impl DdnsProvider for Cloudflare {
    fn id(&self) -> &str {
        "cloudflare"
    }

    fn sync_record<'a>(
        &'a self,
        fqdn: &'a str,
        rtype: &'a str,
        ip: &'a str,
    ) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move {
            let zone = self.find_zone(fqdn).await?;
            let zid = self
                .send(self.client.get(format!("{}/zones?name={}", API, zone)))
                .await?["result"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .first()
                .and_then(|z| z["id"].as_str().map(String::from))
                .ok_or_else(|| anyhow!("未找到 zone id：{}", zone))?;
            let list = self
                .send(self.client.get(format!(
                    "{}/zones/{}/dns_records?type={}&name={}",
                    API, zid, rtype, fqdn
                )))
                .await?;
            // CF 的 AAAA 记录 content 是裸 IP；TXT/CNAME 才带引号，这里统一去引号后比较
            let cur = list["result"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .first()
                .map(|r| {
                    (
                        r["id"].as_str().unwrap_or("").to_string(),
                        r["content"]
                            .as_str()
                            .unwrap_or("")
                            .trim_matches('"')
                            .to_string(),
                    )
                });
            let body = serde_json::json!({ "type": rtype, "name": fqdn, "content": ip });
            match cur {
                None => {
                    self.send(
                        self.client
                            .post(format!("{}/zones/{}/dns_records", API, zid))
                            .json(&body),
                    )
                    .await?;
                    Ok(format!("{} 记录新建 {}", rtype, ip))
                }
                Some((rid, old)) if old != ip => {
                    // 更新必须用 PATCH：PUT 会重置未提交的字段（ttl/proxied/comment 等）
                    self.send(
                        self.client
                            .patch(format!("{}/zones/{}/dns_records/{}", API, zid, rid))
                            .json(&body),
                    )
                    .await?;
                    Ok(format!("{} 记录更新 {} → {}", rtype, old, ip))
                }
                // 值未变：不发生任何写入，也就不会白刷 Cloudflare 的编辑配额
                Some(_) => Ok("未变".to_string()),
            }
        })
    }
}
