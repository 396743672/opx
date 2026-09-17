//! 阿里云云解析 DDNS：RPC API（alidns.aliyuncs.com），HMAC-SHA1 签名。

use anyhow::{anyhow, Result};
use base64::Engine;
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha1::Sha1;

use crate::services::acme::dns::{BoxFuture, DnsProvider};

const API: &str = "https://alidns.aliyuncs.com/";

/// 阿里云 RPC percentEncode：保留 A-Za-z0-9-_.~，其余按 UTF-8 %XX 大写。
pub fn percent_encode(s: &str) -> String {
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

/// 排序 → 拼接 → "GET&%2F&" 前缀 → HMAC-SHA1(secret + "&") → base64。
pub fn rpc_signature(params: &[(&str, &str)], secret: &str) -> String {
    let mut sorted: Vec<&(&str, &str)> = params.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let query = sorted
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let string_to_sign = format!("GET&%2F&{}", percent_encode(&query));
    let mut mac = Hmac::<Sha1>::new_from_slice(format!("{}&", secret).as_bytes()).unwrap();
    mac.update(string_to_sign.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

pub struct Aliyun {
    key_id: String,
    key_secret: String,
    client: reqwest::Client,
}

impl Aliyun {
    pub fn new(key_id: String, key_secret: String) -> Self {
        Self {
            key_id,
            key_secret,
            client: reqwest::Client::new(),
        }
    }

    /// 签名并 GET；业务错误在 HTTP 400 的 JSON body 里（Code/Message）。
    async fn call(&self, action: &str, biz: &[(&str, String)]) -> Result<Value> {
        let mut params: Vec<(&str, String)> = vec![
            ("Format", "JSON".into()),
            ("Version", "2015-01-09".into()),
            ("AccessKeyId", self.key_id.clone()),
            ("SignatureMethod", "HMAC-SHA1".into()),
            ("SignatureVersion", "1.0".into()),
            ("SignatureNonce", uuid::Uuid::new_v4().to_string()),
            (
                "Timestamp",
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            ),
            ("Action", action.into()),
        ];
        params.extend(biz.iter().cloned());
        let refs: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let sig = rpc_signature(&refs, &self.key_secret);
        let mut query: Vec<String> = params
            .iter()
            .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
            .collect();
        query.push(format!("Signature={}", percent_encode(&sig)));
        // 全 URL 手拼（已按签名同规则编码）；.query() 会二次编码
        let resp = self
            .client
            .get(format!("{}?{}", API, query.join("&")))
            .send()
            .await?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if !status.is_success() || body.get("Code").is_some() {
            return Err(anyhow!(
                "阿里云 API 失败（{}）：{}",
                status,
                body.get("Message")
                    .and_then(|m| m.as_str())
                    .unwrap_or(&text)
            ));
        }
        Ok(body)
    }

    /// 列账户域名（首页 100；ponytail: 更多请用 DomainName 过滤）
    async fn zone_of(&self, fqdn: &str) -> Result<String> {
        let names = self.list_zones().await?;
        crate::services::acme::dns::longest_zone_match(fqdn, &names)
            .ok_or_else(|| anyhow!("该域名不在阿里云账户的解析中：{}", fqdn))
    }
}

impl DnsProvider for Aliyun {
    fn id(&self) -> &str {
        "aliyun"
    }

    fn list_zones<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            // ponytail: 首页 100；更多请用 DomainName 关键字过滤
            let body = self
                .call("DescribeDomains", &[("PageSize", "100".into())])
                .await?;
            Ok(body["Domains"]["Domain"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|d| d["DomainName"].as_str().map(String::from))
                .collect())
        })
    }

    fn find_zone<'a>(&'a self, domain: &'a str) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move { self.zone_of(domain).await })
    }

    /// 返回 (RecordId, Value)：无记录 → None
    fn get_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str)
        -> BoxFuture<'a, Result<Option<String>>>
    {
        Box::pin(async move {
            let list = self
                .call(
                    "DescribeSubDomainRecords",
                    &[("SubDomain", fqdn.to_string()), ("Type", rtype.to_string())],
                )
                .await?;
            Ok(list["DomainRecords"]["Record"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|r| r["Value"].as_str())
                .map(|s| s.to_string()))
        })
    }

    fn set_value<'a>(&'a self, fqdn: &'a str, rtype: &'a str, value: &'a str)
        -> BoxFuture<'a, Result<()>>
    {
        Box::pin(async move {
            let zone = self.zone_of(fqdn).await?;
            let rr = fqdn
                .strip_suffix(&format!(".{}", zone))
                .unwrap_or("@")
                .to_string();
            let rr = if rr.is_empty() { "@".to_string() } else { rr };
            let list = self
                .call(
                    "DescribeSubDomainRecords",
                    &[("SubDomain", fqdn.to_string()), ("Type", rtype.to_string())],
                )
                .await?;
            let existing = list["DomainRecords"]["Record"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|r| r["RecordId"].as_str())
                .map(|s| s.to_string());
            match existing {
                Some(rid) => {
                    self.call(
                        "UpdateDomainRecord",
                        &[
                            ("RecordId", rid),
                            ("RR", rr),
                            ("Type", rtype.to_string()),
                            ("Value", value.to_string()),
                        ],
                    )
                    .await?;
                }
                None => {
                    self.call(
                        "AddDomainRecord",
                        &[
                            ("DomainName", zone),
                            ("RR", rr),
                            ("Type", rtype.to_string()),
                            ("Value", value.to_string()),
                        ],
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
            let list = self
                .call(
                    "DescribeSubDomainRecords",
                    &[("SubDomain", fqdn.to_string()), ("Type", rtype.to_string())],
                )
                .await?;
            let ids: Vec<String> = list["DomainRecords"]["Record"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|r| r["RecordId"].as_str().map(String::from))
                .collect();
            for rid in ids {
                self.call("DeleteDomainRecord", &[("RecordId", rid)]).await?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 阿里云官方文档算例（https://help.aliyun.com/document_detail/30563.html）：
    /// 固定参数与密钥 → 已知签名。算法错了这里立刻红。
    /// 注：官方签名基于文档示例的完整 13 个参数（含 Format/RegionId/SignatureNonce/
    /// Timestamp/Version），缺一个结果就不同。
    #[test]
    fn rpc_signature_matches_official_vector() {
        let params = [
            ("AccessKeyId", "testid"),
            ("Action", "Pub"),
            ("Format", "XML"),
            ("MessageContent", "aGVsbG8gd29ybGQ"),
            ("ProductKey", "12345abcde"),
            ("Qos", "0"),
            ("RegionId", "cn-shanghai"),
            ("SignatureMethod", "HMAC-SHA1"),
            ("SignatureNonce", "3ee8c1b8-83d3-44af-a94f-4e0ad82fd6cf"),
            ("SignatureVersion", "1.0"),
            ("Timestamp", "2018-07-31T07:43:57Z"),
            ("TopicFullName", "/12345abcde/testdevice/user/get"),
            ("Version", "2018-01-20"),
        ];
        // 官方示例签名（AccessKeySecret = "testsecret"，密钥补 "&"）
        assert_eq!(
            rpc_signature(&params, "testsecret"),
            "NUh3otvAoXOZmG/a2gDShh6Ze9w="
        );
    }

    #[test]
    fn percent_encode_follows_rfc3986() {
        assert_eq!(percent_encode("aZ09-_.~"), "aZ09-_.~");
        assert_eq!(percent_encode(" "), "%20");
        assert_eq!(percent_encode("/"), "%2F");
        assert_eq!(percent_encode("中"), "%E4%B8%AD");
    }
}
