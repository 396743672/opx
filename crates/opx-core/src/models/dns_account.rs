//! DNS 服务商账号（证书 DNS-01 用）。一份凭证可被多个站点引用。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DnsAccount {
    pub id: String,
    pub name: String,
    /// "cloudflare" | "aliyun" | "dnspod" | "huawei"
    pub provider: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub access_key_id: String,
    #[serde(default)]
    pub access_key_secret: String,
    #[serde(default)]
    pub zones: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tested_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 只填必填三字段必须能反序列化（老文件/手写文件容错）
    #[test]
    fn deserializes_with_required_fields_only() {
        let a: DnsAccount = serde_json::from_str(
            r#"{"id":"a1","name":"公司 CF","provider":"cloudflare"}"#,
        )
        .unwrap();
        assert_eq!(a.id, "a1");
        assert_eq!(a.provider, "cloudflare");
        assert!(a.token.is_empty());
        assert!(a.access_key_id.is_empty());
        assert!(a.access_key_secret.is_empty());
        assert!(a.zones.is_empty());
        assert!(a.tested_at.is_none());
    }

    /// tested_at 为 None 时不得写进 JSON（保持文件干净）
    #[test]
    fn omits_untested_timestamp_on_serialize() {
        let a = DnsAccount {
            id: "a1".into(),
            name: "n".into(),
            provider: "aliyun".into(),
            token: String::new(),
            access_key_id: "k".into(),
            access_key_secret: "s".into(),
            zones: vec!["example.com".into()],
            tested_at: None,
        };
        let json = serde_json::to_string(&a).unwrap();
        assert!(!json.contains("tested_at"), "未测试不应出现 tested_at：{}", json);
        assert!(json.contains("example.com"));
    }

    /// 往返一致
    #[test]
    fn round_trips() {
        let a = DnsAccount {
            id: "a1".into(),
            name: "公司 CF".into(),
            provider: "cloudflare".into(),
            token: "tok".into(),
            access_key_id: String::new(),
            access_key_secret: String::new(),
            zones: vec!["a.com".into(), "b.com".into()],
            tested_at: Some("2026-09-16T10:00:00+08:00".into()),
        };
        let back: DnsAccount =
            serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        assert_eq!(back.id, a.id);
        assert_eq!(back.zones, a.zones);
        assert_eq!(back.tested_at, a.tested_at);
    }
}
