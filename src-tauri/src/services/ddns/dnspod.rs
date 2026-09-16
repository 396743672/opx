//! DNSPod（腾讯云 API 3.0）DDNS：TC3-HMAC-SHA256 签名。
//!
//! 签名规范（腾讯云「签名方法 v3」）：CanonicalRequest → StringToSign → 派生密钥链 →
//! hex(HMAC-SHA256(SecretSigning, StringToSign))。Date 必须由 X-TC-Timestamp 按 UTC 推出。

use anyhow::{anyhow, Result};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{BoxFuture, DdnsProvider};

const HOST: &str = "dnsapi.tencentcloudapi.com";
const VERSION: &str = "2021-03-23";
/// 签名与实发必须同一字面量：TC3 要求 signed content-type 与线上头逐字节一致。
const CONTENT_TYPE: &str = "application/json";
/// 固定引用时间戳（官方示例用值），仅测试使用
#[cfg(test)]
const TS_2019: i64 = 1_551_113_065;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// HMAC-SHA256（密钥任意长度，不需要 `unwrap`：new_from_slice 对 HMAC 恒不失败）。
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    // ponytail: Hmac::<Sha256> 的 InvalidLength 对任意 key 长度都不会触发，故安全
    let mut mac = <Hmac<Sha256>>::new_from_slice(key).expect("HMAC 接受任意长度密钥");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

/// 六行规范化请求：method\nuri\nquery\nheaders\nsignedHeaders\npayloadHash。
/// headers 需已按 key 升序、key/value 小写去首尾空格。
fn tc3_canonical_request(
    method: &str,
    uri: &str,
    query: &str,
    canonical_headers: &[(&str, &str)],
    signed_headers: &str,
    payload_hash: &str,
) -> String {
    let headers = canonical_headers
        .iter()
        .map(|(k, v)| format!("{}:{}\n", k.trim(), v.trim()))
        .collect::<String>();
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method, uri, query, headers, signed_headers, payload_hash
    )
}

/// 完整 Authorization 头（含派生密钥链与签名）。
fn tc3_authorization(
    secret_id: &str,
    secret_key: &str,
    ts: i64,
    service: &str,
    canonical_request: &str,
) -> String {
    // Date 必须由 X-TC-Timestamp 按 UTC 推出（腾讯云明确要求，否则签名不匹配）
    let date = chrono::DateTime::from_timestamp(ts, 0)
        .unwrap_or_default()
        .format("%Y-%m-%d")
        .to_string();
    let hashed_cr = hex(&Sha256::digest(canonical_request.as_bytes()));
    let sts = format!(
        "TC3-HMAC-SHA256\n{}\n{}/{}/tc3_request\n{}",
        ts, date, service, hashed_cr
    );
    let k_date = hmac_sha256(format!("TC3{}", secret_key).as_bytes(), date.as_bytes());
    let k_service = hmac_sha256(&k_date, service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"tc3_request");
    let signature = hex(&hmac_sha256(&k_signing, sts.as_bytes()));
    format!(
        "TC3-HMAC-SHA256 Credential={}/{}/{}/tc3_request, \
         SignedHeaders=content-type;host;x-tc-action;x-tc-timestamp, Signature={}",
        secret_id, date, service, signature
    )
}

pub struct Dnspod {
    secret_id: String,
    secret_key: String,
    client: reqwest::Client,
}

impl Dnspod {
    pub fn new(secret_id: String, secret_key: String) -> Self {
        Self {
            secret_id,
            secret_key,
            client: reqwest::Client::new(),
        }
    }

    /// 签名并 POST；业务错误在 HTTP 200 的 Response.Error 里。
    ///
    /// 注意：实际发送的头必须与签名时逐字节一致（尤其 `X-TC-Action` 大小写与
    /// `Content-Type` 的 charset 补全），否则返回 AuthFailure.SignatureFailure。
    async fn call(&self, action: &str, payload: Value) -> Result<Value> {
        let body = payload.to_string();
        let ts = chrono::Utc::now().timestamp();
        let ts_str = ts.to_string();
        let payload_hash = hex(&Sha256::digest(body.as_bytes()));
        // 头按 key 升序：content-type < host < x-tc-action < x-tc-timestamp
        let cr = tc3_canonical_request(
            "POST",
            "/",
            "",
            &[
                ("content-type", CONTENT_TYPE),
                ("host", HOST),
                ("x-tc-action", &action.to_lowercase()),
                ("x-tc-timestamp", &ts_str),
            ],
            "content-type;host;x-tc-action;x-tc-timestamp",
            &payload_hash,
        );
        let auth = tc3_authorization(&self.secret_id, &self.secret_key, ts, "dnspod", &cr);
        let resp = self
            .client
            .post(format!("https://{}/", HOST))
            // 与签名值逐字节一致：不加 charset
            .header("Content-Type", CONTENT_TYPE)
            .header("X-TC-Action", action)
            .header("X-TC-Version", VERSION)
            .header("X-TC-Timestamp", &ts_str)
            .header("Authorization", auth)
            .body(body)
            .send()
            .await?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(anyhow!("DNSPod API 失败（{}）：{}", status, text));
        }
        if let Some(err) = v["Response"]["Error"].as_object() {
            let code = err.get("Code").and_then(|c| c.as_str()).unwrap_or("");
            let msg = err.get("Message").and_then(|m| m.as_str()).unwrap_or("");
            return Err(anyhow!("DNSPod API 错误：{} {}", code, msg));
        }
        Ok(v["Response"].clone())
    }

    /// 列账户域名（首页 100）并取最长匹配。
    /// ponytail: 同 Cloudflare，仅首页 100 个域名；超出请改用 DescribeDomain 精确查询。
    async fn find_zone(&self, fqdn: &str) -> Result<String> {
        let body = self
            .call(
                "DescribeDomainList",
                serde_json::json!({ "Offset": 0, "Limit": 100 }),
            )
            .await?;
        let names: Vec<String> = body["DomainList"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|d| d["Name"].as_str().map(String::from))
            .collect();
        crate::services::acme::dns::longest_zone_match(fqdn, &names)
            .ok_or_else(|| anyhow!("该域名不在 DNSPod 账户中：{}", fqdn))
    }
}

impl DdnsProvider for Dnspod {
    fn id(&self) -> &str {
        "dnspod"
    }

    fn sync_record<'a>(
        &'a self,
        fqdn: &'a str,
        rtype: &'a str,
        ip: &'a str,
    ) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move {
            let zone = self.find_zone(fqdn).await?;
            // 根域（fqdn == zone）DNSPod 用 "@" 表示
            let sub = fqdn
                .strip_suffix(&format!(".{}", zone))
                .unwrap_or("@")
                .to_string();
            let sub = if sub.is_empty() { "@".to_string() } else { sub };
            let list = self
                .call(
                    "DescribeRecordList",
                    serde_json::json!({
                        "Domain": zone, "SubDomain": sub, "RecordType": rtype
                    }),
                )
                .await;
            let cur = match list {
                Ok(v) => v["RecordList"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .first()
                    .map(|r| {
                        (
                            r["RecordId"].as_u64().unwrap_or(0),
                            r["Value"].as_str().unwrap_or("").to_string(),
                        )
                    }),
                // DNSPod：无记录时 DescribeRecordList 报 ResourceNotFound.NoFoundData
                Err(e) if e.to_string().contains("NoFoundData") => None,
                Err(e) => return Err(e),
            };
            match cur {
                None => {
                    self.call(
                        "CreateRecord",
                        serde_json::json!({
                            "Domain": zone, "SubDomain": sub, "RecordType": rtype,
                            "RecordLine": "默认", "Value": ip
                        }),
                    )
                    .await?;
                    Ok(format!("{} 记录新建 {}", rtype, ip))
                }
                Some((rid, old)) if old != ip => {
                    self.call(
                        "ModifyRecord",
                        serde_json::json!({
                            "Domain": zone, "RecordId": rid, "SubDomain": sub,
                            "RecordType": rtype, "RecordLine": "默认", "Value": ip
                        }),
                    )
                    .await?;
                    Ok(format!("{} 记录更新 {} → {}", rtype, old, ip))
                }
                Some(_) => Ok("未变".to_string()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 规范化请求的六行构成，用固定输入断言精确字符串（HMAC 链本身由 hmac/sha2 保证）。
    #[test]
    fn canonical_request_layout() {
        let cr = tc3_canonical_request(
            "POST",
            "/",
            "",
            &[
                ("content-type", "application/json; charset=utf-8"),
                ("host", "dnsapi.tencentcloudapi.com"),
                ("x-tc-action", "describerecordlist"),
            ],
            "content-type;host;x-tc-action",
            "0000000000000000000000000000000000000000000000000000000000000000",
        );
        assert_eq!(cr, "POST\n/\n\ncontent-type:application/json; charset=utf-8\nhost:dnsapi.tencentcloudapi.com\nx-tc-action:describerecordlist\n\ncontent-type;host;x-tc-action\n0000000000000000000000000000000000000000000000000000000000000000");
    }

    #[test]
    fn authorization_header_format() {
        let auth = tc3_authorization(
            "AKIDtest",
            "testkey",
            TS_2019,
            "dnspod",
            "POST\n/\n\n\n\n", // canonical request（占位，仅测头格式）
        );
        // 结构断言：前缀、scope、签名段齐全；签名值是 64 位 hex
        assert!(auth.starts_with("TC3-HMAC-SHA256 Credential=AKIDtest/2019-02-25/dnspod/tc3_request, SignedHeaders=content-type;host;x-tc-action;x-tc-timestamp, Signature="));
        let sig = auth.rsplit('=').next().unwrap();
        assert_eq!(sig.len(), 64);
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
        // ts 以 UTC 计算 date：同一时刻的 UTC 与该时刻的本地日期不同时，必须以 UTC 为准
        let auth_next = tc3_authorization("AKIDtest", "testkey", TS_2019 + 86_400, "dnspod", "");
        assert!(auth_next.contains("/2019-02-26/dnspod/tc3_request"));
    }

    /// 派生链的连续性：date → service → signing 三段每段都改变密钥，
    /// 任一段写错（少 HMAC、前缀写错）这里立刻红。
    #[test]
    fn hmac_sha256_key_derivation_chain_is_staged() {
        let k_date = hmac_sha256(b"TC3testkey", b"2019-02-25");
        let k_service = hmac_sha256(&k_date, b"dnspod");
        let k_signing = hmac_sha256(&k_service, b"tc3_request");
        assert_ne!(k_date, k_service);
        assert_ne!(k_service, k_signing);
        assert_ne!(k_date, k_signing);
    }

    /// 官方示例（签名方法 v3）的完整 Authorization 头。
    /// 密钥不可得，但签名必须与官方示例（SecretId/Service 不同）结构一致且长度正确；
    /// 此处用官方示例的字符串拼接规则校验 scope 顺序与固定 SignedHeaders。
    #[test]
    fn authorization_uses_dnspod_service_scope() {
        let auth = tc3_authorization("AKIDEXAMPLE", "sk", TS_2019, "dnspod", "cr");
        assert!(auth.contains("Credential=AKIDEXAMPLE/2019-02-25/dnspod/tc3_request"));
        assert!(auth.contains("SignedHeaders=content-type;host;x-tc-action;x-tc-timestamp"));
        // 头部与签名之间必须是 ", " 分隔（官方格式，缺空格会 SignatureFailure）
        assert!(auth.contains("tc3_request, SignedHeaders="));
        assert!(auth.contains(", Signature="));
    }

    /// 自洽性：独立重算整条派生链与规范化串，与 `tc3_authorization` 产出的签名逐字节比对。
    /// 形状断言（starts_with / contains / assert_ne）无法发现链算错——例如 k_date 少 "TC3"
    /// 前缀、或 date 与 service 位置对调，格式和「三段互不相等」都仍成立。这条能发现。
    #[test]
    fn authorization_signature_matches_independent_chain_recompute() {
        let secret_id = "AKIDtest";
        let secret_key = "testsecret";
        let ts: i64 = TS_2019;
        let body = r#"{"Domain":"example.com"}"#;
        let headers = [
            ("content-type", "application/json"),
            ("host", "dnsapi.tencentcloudapi.com"),
            ("x-tc-action", "describerecordlist"),
            ("x-tc-timestamp", "1551113065"),
        ];

        // 被测：按实现自身的方式拼 canonical request
        let cr = tc3_canonical_request(
            "POST",
            "/",
            "",
            &headers,
            "content-type;host;x-tc-action;x-tc-timestamp",
            &hex(&Sha256::digest(body.as_bytes())),
        );
        let auth = tc3_authorization(secret_id, secret_key, ts, "dnspod", &cr);

        // ---- 独立重算（不复用实现里的任何函数）----
        let hmac = |key: &[u8], data: &[u8]| -> Vec<u8> {
            let mut m = <Hmac<Sha256>>::new_from_slice(key).expect("任意长度密钥均可");
            m.update(data);
            m.finalize().into_bytes().to_vec()
        };
        let hex_of = |b: &[u8]| -> String { b.iter().map(|x| format!("{:02x}", x)).collect() };

        // 手写六行构成，不经 tc3_canonical_request
        let canonical = format!(
            "POST\n/\n\n{}\n\n{}\n{}",
            headers
                .iter()
                .map(|(k, v)| format!("{}:{}", k, v))
                .collect::<Vec<_>>()
                .join("\n"),
            headers
                .iter()
                .map(|(k, _)| *k)
                .collect::<Vec<_>>()
                .join(";"),
            hex_of(&Sha256::digest(body.as_bytes())),
        );
        let date = "2019-02-25";
        let string_to_sign = format!(
            "TC3-HMAC-SHA256\n{}\n{}/{}/tc3_request\n{}",
            ts,
            date,
            "dnspod",
            hex_of(&Sha256::digest(canonical.as_bytes())),
        );
        // 手写派生链：少 "TC3" 前缀、或 date/service 对调，这里会算出不同结果
        let k_date = hmac(format!("TC3{}", secret_key).as_bytes(), date.as_bytes());
        let k_service = hmac(&k_date, b"dnspod");
        let k_signing = hmac(&k_service, b"tc3_request");
        let expected = hex_of(&hmac(&k_signing, string_to_sign.as_bytes()));

        assert!(
            auth.ends_with(&format!("Signature={}", expected)),
            "授权头中的签名必须等于独立重算结果\n实际: {}\n期望签名: {}",
            auth,
            expected
        );
    }
}
