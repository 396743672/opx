//! 华为云 DNS DDNS：SDK-HMAC-SHA256 签名，zone 列表用 v2、记录集用 v2.1。
//! 注意：华为的记录名是带尾点的 FQDN（"home.example.com."）。

use anyhow::{anyhow, Result};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::services::acme::dns::{BoxFuture, DnsProvider};

const HOST: &str = "dns.myhuaweicloud.com";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// SDK-HMAC-SHA256 待签串（CanonicalRequest）：
/// method \n uri \n query \n canonical_headers \n signed_headers \n payload_hash
/// canonical_headers 每行自带尾 \n，故其后紧跟一个空行。
fn hws_canonical_request(
    method: &str,
    uri: &str,
    query: &str,
    canonical_headers: &[(&str, &str)],
    signed_headers: &str,
    payload_hash: &str,
) -> String {
    let headers = canonical_headers
        .iter()
        .map(|(k, v)| format!("{}:{}\n", k, v))
        .collect::<String>();
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method, uri, query, headers, signed_headers, payload_hash
    )
}

/// StringToSign = "SDK-HMAC-SHA256\n{date}\n{sha256(canonical_request)}"，
/// 再 HMAC-SHA256(sk) 得签名。
fn hws_authorization(ak: &str, sk: &str, date: &str, canonical_request: &str) -> String {
    let hashed_cr = hex(&Sha256::digest(canonical_request.as_bytes()));
    let sts = format!("SDK-HMAC-SHA256\n{}\n{}", date, hashed_cr);
    let mut mac = Hmac::<Sha256>::new_from_slice(sk.as_bytes()).unwrap();
    mac.update(sts.as_bytes());
    format!(
        "SDK-HMAC-SHA256 Access={}, SignedHeaders=content-type;host;x-sdk-date, Signature={}",
        ak,
        hex(&mac.finalize().into_bytes())
    )
}

/// RFC3986 percent-encode（与阿里云同规则；用于 query 的 k/v）
fn pe(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// 华为记录名：FQDN，且必须以点结尾（官方要求「以"."结束的完整主机名」）。
fn record_name(fqdn: &str) -> String {
    format!("{}.", fqdn.trim_end_matches('.'))
}

pub struct Huawei {
    ak: String,
    sk: String,
    client: reqwest::Client,
}

impl Huawei {
    pub fn new(ak: String, sk: String) -> Self {
        Self {
            ak,
            sk,
            client: crate::utils::http::client(),
        }
    }

    /// 逐请求签名（无需先取 IAM token）。GET 时 payload 为空串，用 sha256("") 常量。
    async fn call(
        &self,
        method: &str,
        uri: &str,
        query: &[(&str, String)],
        payload: Option<&Value>,
    ) -> Result<Value> {
        let date = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let mut q = query.to_vec();
        q.sort_by(|a, b| a.0.cmp(b.0));
        let query_str = q
            .iter()
            .map(|(k, v)| format!("{}={}", pe(k), pe(v)))
            .collect::<Vec<_>>()
            .join("&");
        let body = payload.map(|v| v.to_string()).unwrap_or_default();
        let payload_hash = if body.is_empty() {
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string()
        } else {
            hex(&Sha256::digest(body.as_bytes()))
        };
        let cr = hws_canonical_request(
            method,
            uri,
            &query_str,
            &[
                ("content-type", "application/json"),
                ("host", HOST),
                ("x-sdk-date", &date),
            ],
            "content-type;host;x-sdk-date",
            &payload_hash,
        );
        let auth = hws_authorization(&self.ak, &self.sk, &date, &cr);
        let url = if query_str.is_empty() {
            format!("https://{}{}", HOST, uri)
        } else {
            format!("https://{}{}?{}", HOST, uri, query_str)
        };
        let mut req = self
            .client
            .request(reqwest::Method::from_bytes(method.as_bytes()).unwrap(), url)
            .header("Content-Type", "application/json")
            .header("X-Sdk-Date", &date)
            .header("Authorization", auth);
        if payload.is_some() {
            req = req.body(body);
        }
        let resp = req.send().await?;
        let status = resp.status();
        // 华为 DNS 常见 202（已受理）也算成功
        if !(status.is_success() || status.as_u16() == 202) {
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("华为云 API 失败（{}）：{}", status, text));
        }
        let text = resp.text().await.unwrap_or_default();
        // 空体合法（写操作 2xx 常见）；非空但解析不了说明是网关/HTML 错误页，
        // 必须显式报错——否则会退化成 Null，让调用方误报成「域名不在账户中」
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text)
            .map_err(|e| anyhow!("华为云 API 返回了非 JSON 响应（{}）：{}", e, text))
    }

    /// 返回 (zone 名去尾点, zone id)。华为 zone 名带尾点（"example.com."）。
    async fn find_zone(&self, fqdn: &str) -> Result<(String, String)> {
        // ponytail: zone 列表留 v2 —— 仅能确认同组 zone 接口（POST /v2/zones、
        // GET /v2/zones/{zone_id}）为 v2，ListPublicZones 文档页取不到。
        // 若首次查找即 404，就是这里：改 /v2.1/zones（同记录集一处前缀）。
        let body = self
            .call("GET", "/v2/zones", &[("limit", "100".into())], None)
            .await?;
        let zones = body["zones"].as_array().cloned().unwrap_or_default();
        let names: Vec<String> = zones
            .iter()
            .filter_map(|z| z["name"].as_str().map(String::from))
            .collect();
        let zone = crate::services::acme::dns::longest_zone_match(fqdn, &names)
            .ok_or_else(|| anyhow!("该域名不在华为云账户的 zone 中：{}", fqdn))?;
        let zid = zones
            .iter()
            .find(|z| z["name"].as_str().map(|n| n.trim_end_matches('.')) == Some(zone.as_str()))
            .and_then(|z| z["id"].as_str().map(String::from))
            .ok_or_else(|| anyhow!("未找到 zone id：{}", zone))?;
        Ok((zone, zid))
    }
}

impl DnsProvider for Huawei {
    fn id(&self) -> &str {
        "huawei"
    }

    fn list_zones<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            // ponytail: zone 列表留 v2 —— 仅能确认同组 zone 接口（POST /v2/zones、
            // GET /v2/zones/{zone_id}）为 v2，ListPublicZones 文档页取不到。
            // 若首次查找即 404，就是这里：改 /v2.1/zones（同记录集一处前缀）。
            let body = self
                .call("GET", "/v2/zones", &[("limit", "100".into())], None)
                .await?;
            Ok(body["zones"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|z| z["name"].as_str().map(String::from))
                .collect())
        })
    }

    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move { Ok(Huawei::find_zone(self, domain).await?.0) })
    }

    fn get_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str)
        -> BoxFuture<'a, Result<Option<String>>>
    {
        Box::pin(async move {
            let (_zone, zid) = Huawei::find_zone(self, fqdn).await?;
            let name = record_name(fqdn); // 华为记录名带尾点
            let list = self
                .call(
                    "GET",
                    &format!("/v2.1/zones/{}/recordsets", zid),
                    &[("type", rtype.to_string()), ("name", name)],
                    None,
                )
                .await?;
            Ok(list["recordsets"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|r| r["records"].as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()))
        })
    }

    fn set_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str, value: &'a str)
        -> BoxFuture<'a, Result<()>>
    {
        Box::pin(async move {
            let (_zone, zid) = Huawei::find_zone(self, fqdn).await?;
            let name = record_name(fqdn);
            let list = self
                .call(
                    "GET",
                    &format!("/v2.1/zones/{}/recordsets", zid),
                    &[("type", rtype.to_string()), ("name", name.clone())],
                    None,
                )
                .await?;
            let existing = list["recordsets"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|r| r["id"].as_str())
                .map(|s| s.to_string());
            let body = serde_json::json!({
                "name": name, "type": rtype, "ttl": 300, "records": [value]
            });
            match existing {
                Some(rid) => {
                    self.call(
                        "PUT",
                        &format!("/v2.1/zones/{}/recordsets/{}", zid, rid),
                        &[],
                        Some(&body),
                    )
                    .await?;
                }
                None => {
                    self.call(
                        "POST",
                        &format!("/v2.1/zones/{}/recordsets", zid),
                        &[],
                        Some(&body),
                    )
                    .await?;
                }
            }
            Ok(())
        })
    }

    fn delete_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str)
        -> BoxFuture<'a, Result<()>>
    {
        Box::pin(async move {
            let (_zone, zid) = Huawei::find_zone(self, fqdn).await?;
            let name = record_name(fqdn);
            let list = self
                .call(
                    "GET",
                    &format!("/v2.1/zones/{}/recordsets", zid),
                    &[("type", rtype.to_string()), ("name", name)],
                    None,
                )
                .await?;
            let ids: Vec<String> = list["recordsets"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|r| r["id"].as_str().map(String::from))
                .collect();
            for rid in ids {
                self.call(
                    "DELETE",
                    &format!("/v2.1/zones/{}/recordsets/{}", zid, rid),
                    &[],
                    None,
                )
                .await?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_request_layout() {
        let cr = hws_canonical_request(
            "GET",
            "/v2/zones",
            "limit=100&name=example.com",
            &[
                ("content-type", "application/json"),
                ("host", "dns.myhuaweicloud.com"),
                ("x-sdk-date", "20260916T000000Z"),
            ],
            "content-type;host;x-sdk-date",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", // sha256("")
        );
        assert_eq!(cr, "GET\n/v2/zones\nlimit=100&name=example.com\ncontent-type:application/json\nhost:dns.myhuaweicloud.com\nx-sdk-date:20260916T000000Z\n\ncontent-type;host;x-sdk-date\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn authorization_header_format() {
        let auth = hws_authorization(
            "AKTEST",
            "sk-test",
            "20260916T000000Z",
            "GET\n/\n\n\n\n\n\n",
        );
        assert!(auth.starts_with(
            "SDK-HMAC-SHA256 Access=AKTEST, SignedHeaders=content-type;host;x-sdk-date, Signature="
        ));
        let sig = auth.rsplit('=').next().unwrap();
        assert_eq!(sig.len(), 64);
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn record_name_is_fqdn_with_single_trailing_dot() {
        assert_eq!(record_name("home.example.com"), "home.example.com.");
        // 已带尾点的输入不得变成双点
        assert_eq!(record_name("home.example.com."), "home.example.com.");
        assert!(!record_name("home.example.com.").contains(".."));
    }
}
