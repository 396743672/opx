//! 告警通知：资源告警触发时外送 webhook / SMTP。
//! 纯函数层（payload/签名/文案）+ 发送层（Task 3 接线）。

use serde_json::json;

/// 一条告警事件（recorder 触发时传入；测试命令构造假值）
pub struct AlertEvent {
    pub name: String,
    pub metric: String,
    pub value: f64,
    pub threshold: u32,
}

/// 统一文案，webhook text 与邮件正文共用
pub fn alert_text(e: &AlertEvent) -> String {
    format!(
        "[OPX 告警] {} {} {}%（阈值 {}%）",
        e.name,
        e.metric,
        e.value.round(),
        e.threshold
    )
}

/// 钉钉加签：HMAC-SHA256(secret, "{ts}\n{secret}") → base64 → URL 转义
pub fn sign_dingtalk(secret: &str, ts_millis: i64) -> String {
    use base64::Engine as _;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(format!("{}\n{}", ts_millis, secret).as_bytes());
    // ponytail: base64 输出只可能出现 + / = 三个需转义字符，逐个替换即够
    base64::engine::general_purpose::STANDARD
        .encode(mac.finalize().into_bytes())
        .replace('+', "%2B")
        .replace('/', "%2F")
        .replace('=', "%3D")
}

/// 按格式构造 (完整请求 URL, JSON body)。
pub fn build_webhook(
    format: &str,
    url: &str,
    secret: &str,
    ts_millis: i64,
    e: &AlertEvent,
) -> (String, String) {
    let text = alert_text(e);
    match format {
        "dingtalk" => {
            let body = json!({"msgtype": "text", "text": {"content": text}}).to_string();
            let url = if secret.is_empty() {
                url.to_string()
            } else {
                format!(
                    "{}&timestamp={}&sign={}",
                    url,
                    ts_millis,
                    sign_dingtalk(secret, ts_millis)
                )
            };
            (url, body)
        }
        "wecom" => (
            url.to_string(),
            json!({"msgtype": "text", "text": {"content": text}}).to_string(),
        ),
        "feishu" => (
            url.to_string(),
            json!({"msg_type": "text", "content": {"text": text}}).to_string(),
        ),
        _ => (
            url.to_string(),
            json!({
                "event": "alert_high", "name": e.name, "metric": e.metric,
                "value": e.value.round(), "threshold": e.threshold, "ts": ts_millis,
            })
            .to_string(),
        ),
    }
}

/// SMTP 收件人解析：逗号分隔、trim、去空
pub fn parse_recipients(to: &str) -> Vec<String> {
    to.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev() -> AlertEvent {
        AlertEvent {
            name: "MySQL 8.0".into(),
            metric: "cpu".into(),
            value: 95.4,
            threshold: 90,
        }
    }

    #[test]
    fn alert_text_format() {
        assert_eq!(
            alert_text(&ev()),
            "[OPX 告警] MySQL 8.0 cpu 95%（阈值 90%）"
        );
    }

    #[test]
    fn sign_dingtalk_is_deterministic_urlsafe_and_ts_sensitive() {
        let a = sign_dingtalk("secret", 1700000000000);
        assert_eq!(a, sign_dingtalk("secret", 1700000000000), "确定性");
        assert_ne!(a, sign_dingtalk("secret", 1700000000001), "随时间戳变化");
        for c in a.chars() {
            assert!(
                c.is_ascii_alphanumeric()
                    || c == '%'
                    || c == '.'
                    || c == '_'
                    || c == '-'
                    || c == '~',
                "签名须 URL 安全，含非法字符: {c}"
            );
        }
    }

    #[test]
    fn build_webhook_json_generic() {
        let (url, body) = build_webhook("json", "https://x.dev/hook", "", 123, &ev());
        assert_eq!(url, "https://x.dev/hook");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["event"], "alert_high");
        assert_eq!(v["name"], "MySQL 8.0");
        assert_eq!(v["metric"], "cpu");
        assert_eq!(v["threshold"], 90);
    }

    #[test]
    fn build_webhook_dingtalk_appends_sign_only_with_secret() {
        let base = "https://oapi.dingtalk.com/robot/send?access_token=tok";
        let (url, body) = build_webhook("dingtalk", base, "sec", 123, &ev());
        assert!(url.starts_with(base), "原 URL 的 query 保留");
        assert!(url.contains("timestamp=123"), "URL 带时间戳");
        assert!(url.contains("sign="), "URL 带签名");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["msgtype"], "text");
        assert_eq!(v["text"]["content"], alert_text(&ev()));

        let (url2, _) = build_webhook("dingtalk", base, "", 123, &ev());
        assert_eq!(url2, base, "无 secret 不加签");
    }

    #[test]
    fn build_webhook_wecom_and_feishu_shapes() {
        let (_, wb) = build_webhook("wecom", "https://qyapi.weixin.qq.com/hook", "", 1, &ev());
        let v: serde_json::Value = serde_json::from_str(&wb).unwrap();
        assert_eq!(v["msgtype"], "text");
        assert_eq!(v["text"]["content"], alert_text(&ev()));

        let (_, fb) = build_webhook("feishu", "https://open.feishu.cn/hook", "", 1, &ev());
        let v: serde_json::Value = serde_json::from_str(&fb).unwrap();
        assert_eq!(v["msg_type"], "text");
        assert_eq!(v["content"]["text"], alert_text(&ev()));
    }

    #[test]
    fn parse_recipients_splits_trims_and_drops_empty() {
        assert_eq!(
            parse_recipients("a@x.com, b@y.com ,,c@z.com"),
            vec![
                "a@x.com".to_string(),
                "b@y.com".to_string(),
                "c@z.com".to_string()
            ]
        );
        assert!(parse_recipients("  ").is_empty());
    }
}
