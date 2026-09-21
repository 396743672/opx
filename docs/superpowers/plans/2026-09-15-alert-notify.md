# 告警通知渠道（Webhook + SMTP）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 资源告警触发时外送通知——webhook（通用 JSON / 钉钉加签 / 企微 / 飞书）与 SMTP 邮件，设置页可配可测。

**Architecture:** 纯函数层（文案 / payload 构造 / 钉钉加签 / 收件人解析）+ 发送层（reqwest POST、lettre SMTP）+ 单入口 `notify::dispatch`（在 `recorder.rs::eval` 触发分支调用，内部 spawn 游离发送不阻塞 30s 轮询）。失败不重试：warn + 审计 `webhook_failed`。

**Tech Stack:** Rust（reqwest 0.13 已有、sha2 0.10 已有；新增 hmac、base64、lettre 0.11 with tokio1+tokio1-rustls）、Vue 3 + TS。

## Global Constraints

- 全部工作在新分支 `feat/alert-notify`（从 `dev` 建）上，禁止直接改 `dev`/`master`。
- 新增设置字段全部 `#[serde(default)]`（旧 settings.json 零迁移，`smtp_port` 默认 465、`alert_webhook_format` 默认 `"json"`）。
- webhook 格式取值固定四种：`"json" | "dingtalk" | "wecom" | "feishu"`，由用户在设置页选择，不自动识别 URL。
- 事件范围：仅 `alert_high` 触发时发送；`alert_recovered` 不发送。
- 发送失败不重试：`tracing::warn!` + `oplog!("webhook_failed", …)`，不阻断告警链。
- HTTP 客户端超时 10s；发送在游离 spawn 中，不阻塞采样轮询。
- 后端测试 `cd src-tauri && cargo test --lib`，基线 160 passed，只增不减。
- 前端 `npx vue-tsc --noEmit` 必须通过；i18n 键同时加 `zh-CN.ts` 与 `en-US.ts`。
- **格式化只作用于改动文件**：`cd src-tauri && rustfmt --edition 2021 <相对路径>`；**禁止 `cargo fmt`**（仓库非 rustfmt-clean，会重排 67 个无关文件）。
- 提交信息中文 conventional commits，无 `Co-Authored-By`。

---

### Task 1: 依赖 + 设置字段

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/models/settings.rs`
- Modify: `src/models/settings.ts`
- Test: `src-tauri/src/models/settings.rs`（`mod tests`）

**Interfaces:**
- Consumes: 无
- Produces: `AppSettings` 新字段——`alert_webhook_url: String`、`alert_webhook_format: String`（默认 `"json"`）、`alert_webhook_secret: String`、`smtp_host: String`、`smtp_port: u16`（默认 465）、`smtp_user: String`、`smtp_pass: String`、`smtp_to: String`、`smtp_enabled: bool`（Task 3/4 依赖这些确切字段名）

- [ ] **Step 1: 加依赖**

```bash
cd src-tauri && cargo add hmac base64 && cargo add lettre --features tokio1,tokio1-rustls
```

若 `tokio1-rustls` feature 名不识别，用 `cargo add lettre --dry-run` 列出的 rustls 相关 feature 名替代（lettre 0.11.x 的 tokio 异步 rustls TLS feature）。加完 `cargo build 2>&1 | tail -5` 确认编译（首次编译 lettre 较慢）。

- [ ] **Step 2: 写失败测试（settings.rs `mod tests` 末尾追加）**

```rust
    #[test]
    fn notify_fields_default_on_legacy_json() {
        let json = r#"{
            "theme":"auto","language":"zh-CN","sidebar_collapsed":false,
            "software_root":"apps","config_root":"config","mirror_url":"",
            "auto_check_update":true,"close_window_action":"CloseToTray","ask_on_close":true,
            "jre_default_id":null,"github_proxy_url":"","proxy_url":""
        }"#;
        let s: AppSettings = serde_json::from_str(json).expect("legacy settings must load");
        assert_eq!(s.alert_webhook_url, "");
        assert_eq!(s.alert_webhook_format, "json");
        assert_eq!(s.alert_webhook_secret, "");
        assert_eq!(s.smtp_host, "");
        assert_eq!(s.smtp_port, 465);
        assert_eq!(s.smtp_user, "");
        assert_eq!(s.smtp_pass, "");
        assert_eq!(s.smtp_to, "");
        assert!(!s.smtp_enabled);
    }
```

- [ ] **Step 3: 跑测试确认失败**

Run: `cd src-tauri && cargo test --lib settings 2>&1 | tail -5`
Expected: 编译失败（字段不存在）。

- [ ] **Step 4: 加字段**

`AppSettings` 结构体在 `alert_process_mem` 之后追加：

```rust
    // --- 告警通知（webhook + SMTP）---
    /// 告警 webhook URL，空 = 不发送
    #[serde(default)]
    pub alert_webhook_url: String,
    /// "json" | "dingtalk" | "wecom" | "feishu"
    #[serde(default = "default_webhook_format")]
    pub alert_webhook_format: String,
    /// 钉钉加签 secret（仅 dingtalk 生效，空 = 不加签）
    #[serde(default)]
    pub alert_webhook_secret: String,
    #[serde(default)]
    pub smtp_host: String,
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
    #[serde(default)]
    pub smtp_user: String,
    /// SMTP 授权码（明文存储，UI 提示风险）
    #[serde(default)]
    pub smtp_pass: String,
    /// 收件人，逗号分隔多个
    #[serde(default)]
    pub smtp_to: String,
    /// SMTP 总开关：false 时即使填了配置也不发
    #[serde(default)]
    pub smtp_enabled: bool,
```

在 `default_ninety` 旁加两个默认函数，`Default::default()` 实现的 `Self { … }` 里同步补齐九个字段（webhook 串用 `String::new()`，format 用 `default_webhook_format()`，port 用 `default_smtp_port()`，enabled 用 `false`）：

```rust
fn default_webhook_format() -> String {
    "json".to_string()
}

fn default_smtp_port() -> u16 {
    465
}
```

- [ ] **Step 5: 跑测试确认通过**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3`
Expected: 161 passed（160 + 1 新）。

- [ ] **Step 6: TS 侧同步**

`src/models/settings.ts` 的 `AppSettings` 在 `alert_process_mem` 后追加：

```typescript
  alert_webhook_url: string
  alert_webhook_format: string
  alert_webhook_secret: string
  smtp_host: string
  smtp_port: number
  smtp_user: string
  smtp_pass: string
  smtp_to: string
  smtp_enabled: boolean
```

Run: `npx vue-tsc --noEmit`（前端此时不读新字段，应通过）。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/models/settings.rs src/models/settings.ts
git commit -m "feat(alert): 通知渠道设置字段（webhook + SMTP）"
```

---

### Task 2: notify.rs 纯函数层（TDD）

**Files:**
- Create: `src-tauri/src/services/system_monitor/notify.rs`
- Modify: `src-tauri/src/services/system_monitor/mod.rs`（`pub mod notify;`）
- Test: `notify.rs` 内 `mod tests`

**Interfaces:**
- Consumes: Task 1 的设置字段（本任务只用纯函数，不读设置）
- Produces（Task 3 依赖的确切签名）：
  - `pub struct AlertEvent { pub name: String, pub metric: String, pub value: f64, pub threshold: u32 }`
  - `pub fn alert_text(e: &AlertEvent) -> String`
  - `pub fn sign_dingtalk(secret: &str, ts_millis: i64) -> String`
  - `pub fn build_webhook(format: &str, url: &str, secret: &str, ts_millis: i64, e: &AlertEvent) -> (String, String)` — 返回 (完整请求 URL, JSON body)
  - `pub fn parse_recipients(to: &str) -> Vec<String>`

- [ ] **Step 1: 写文件骨架 + 失败测试**

`notify.rs` 先只放 `AlertEvent` 结构体和空 `mod tests`，`mod.rs` 加 `pub mod notify;`。测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev() -> AlertEvent {
        AlertEvent { name: "MySQL 8.0".into(), metric: "cpu".into(), value: 95.4, threshold: 90 }
    }

    #[test]
    fn alert_text_format() {
        assert_eq!(alert_text(&ev()), "[OPX 告警] MySQL 8.0 cpu 95%（阈值 90%）");
    }

    #[test]
    fn sign_dingtalk_is_deterministic_urlsafe_and_ts_sensitive() {
        let a = sign_dingtalk("secret", 1700000000000);
        assert_eq!(a, sign_dingtalk("secret", 1700000000000), "确定性");
        assert_ne!(a, sign_dingtalk("secret", 1700000000001), "随时间戳变化");
        for c in a.chars() {
            assert!(
                c.is_ascii_alphanumeric() || c == '%' || c == '.' || c == '_' || c == '-' || c == '~',
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
            vec!["a@x.com".to_string(), "b@y.com".to_string(), "c@z.com".to_string()]
        );
        assert!(parse_recipients("  ").is_empty());
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && cargo test --lib notify 2>&1 | tail -5`
Expected: 编译失败（函数未定义）。

- [ ] **Step 3: 实现**

```rust
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
        e.name, e.metric, e.value.round(), e.threshold
    )
}

/// 钉钉加签：HMAC-SHA256(secret, "{ts}\n{secret}") → base64 → URL 转义
pub fn sign_dingtalk(secret: &str, ts_millis: i64) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(format!("{}\n{}", ts_millis, secret).as_bytes());
    // ponytail: base64 输出只可能出现 + / = 三个需转义字符，逐个替换即够
    base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        mac.finalize().into_bytes(),
    )
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
        "wecom" => (url.to_string(), json!({"msgtype": "text", "text": {"content": text}}).to_string()),
        "feishu" => (url.to_string(), json!({"msg_type": "text", "content": {"text": text}}).to_string()),
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
```

（若 `base64::Engine::encode` 路径报错，用 `STANDARD.encode(...)` 形式：`use base64::engine::general_purpose::STANDARD; STANDARD.encode(...)`——两者等价，取编译通过者。）

- [ ] **Step 4: 跑测试确认通过**

Run: `cd src-tauri && cargo test --lib notify 2>&1 | tail -3`
Expected: 6 个新测试 PASS（总数 167）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/services/system_monitor/notify.rs src-tauri/src/services/system_monitor/mod.rs
git commit -m "feat(alert): 通知纯函数层（文案/payload 构造/钉钉加签/收件人解析）"
```

---

### Task 3: 发送层 + 触发接线 + 测试命令

**Files:**
- Modify: `src-tauri/src/services/system_monitor/notify.rs`（追加发送层）
- Modify: `src-tauri/src/services/system_monitor/recorder.rs`（`eval` 触发分支接线）
- Modify: `src-tauri/src/commands/config.rs`（新命令）
- Modify: `src-tauri/src/lib.rs`（注册命令）

**Interfaces:**
- Consumes: Task 1 的 `AppSettings` 字段、Task 2 的 `AlertEvent` / `build_webhook` / `parse_recipients` / `alert_text`；`crate::commands::config::read_settings()`；既有 `reqwest`、`oplog!`
- Produces: `pub fn dispatch(e: AlertEvent)`（同步入口，内部 spawn）；`pub async fn send_webhook(url: &str, format: &str, secret: &str, e: &AlertEvent) -> anyhow::Result<()>`；`pub async fn send_mail(s: &AppSettings, e: &AlertEvent) -> anyhow::Result<()>`；命令 `test_alert_webhook`

- [ ] **Step 1: notify.rs 追加发送层**

```rust
use crate::models::settings::AppSettings;

/// 发送入口（recorder 触发分支调用）：读最新设置，两个渠道各自游离 spawn，
/// 不阻塞采样轮询。失败 warn + 审计，不重试。
pub fn dispatch(e: AlertEvent) {
    let s = crate::commands::config::read_settings().unwrap_or_default();
    if !s.alert_webhook_url.trim().is_empty() {
        let url = s.alert_webhook_url.clone();
        let fmt = s.alert_webhook_format.clone();
        let sec = s.alert_webhook_secret.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(err) = send_webhook(&url, &fmt, &sec, &e).await {
                tracing::warn!(error = %err, "告警 webhook 发送失败");
                crate::oplog!("webhook_failed", "webhook", &format!("{}", err));
            }
        });
    }
    if s.smtp_enabled && !s.smtp_host.trim().is_empty() && !s.smtp_to.trim().is_empty() {
        let smtp = s.clone();
        let ev = e; // 同一事件两个渠道共用（webhook 分支已各持一份）
        tauri::async_runtime::spawn(async move {
            if let Err(err) = send_mail(&smtp, &ev).await {
                tracing::warn!(error = %err, "告警邮件发送失败");
                crate::oplog!("webhook_failed", "smtp", &format!("{}", err));
            }
        });
    }
}
```

注意 `AlertEvent` 无 `Clone`，`dispatch` 内第一个 spawn 会 move `e`——给 `AlertEvent` 加 `#[derive(Clone)]`，两处 spawn 各 clone 一份。

```rust
/// 实际发送 webhook（测试命令也直接调它做真实验证）。10s 超时，非 2xx 或
/// 机器人 errcode/code 非 0 视为失败。
pub async fn send_webhook(
    url: &str,
    format: &str,
    secret: &str,
    e: &AlertEvent,
) -> anyhow::Result<()> {
    let ts = chrono::Local::now().timestamp_millis();
    let (full_url, body) = build_webhook(format, url, secret, ts, e);
    let resp = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?
        .post(&full_url)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("HTTP {}: {}", status.as_u16(), &text.chars().take(200).collect::<String>());
    }
    // 钉钉/企微（errcode）、飞书（code）业务错误也返回 200，需查 body
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
        let code = v
            .get("errcode")
            .or_else(|| v.get("code"))
            .and_then(|c| c.as_i64())
            .unwrap_or(0);
        if code != 0 {
            let msg = v.get("errmsg").or_else(|| v.get("msg"))
                .and_then(|m| m.as_str()).unwrap_or("未知错误");
            anyhow::bail!("机器人返回错误 {}: {}", code, msg);
        }
    }
    Ok(())
}

/// 实际发送邮件（lettre async SMTP + rustls TLS）。
pub async fn send_mail(s: &AppSettings, e: &AlertEvent) -> anyhow::Result<()> {
    use lettre::{
        transport::smtp::authentication::Credentials, AsyncSmtpTransport, AsyncTransport,
        Message, Tokio1Executor,
    };
    let subject = format!("[OPX 告警] {} {} 超阈值", e.name, e.metric);
    let mut builder = Message::builder()
        .from(format!("OPX <{}>", s.smtp_user).parse()?)
        .subject(subject);
    for r in parse_recipients(&s.smtp_to) {
        builder = builder.to(r.parse()?);
    }
    let email = builder.body(format!(
        "{}\n时间: {}",
        alert_text(e),
        chrono::Local::now().to_rfc3339()
    ))?;
    let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&s.smtp_host)?
        .port(s.smtp_port)
        .credentials(Credentials::new(s.smtp_user.clone(), s.smtp_pass.clone()))
        .build();
    mailer.send(email).await?;
    Ok(())
}
```

（`Message::builder().from/to` 的参数是 `lettre::message::Mailbox`，`"...".parse()?` 自动转换；编译器会指路。）

- [ ] **Step 2: recorder.rs 触发接线**

`eval` 的 `should_alert` 命中分支，现有 `oplog!` 与 `emit` 之后加一行：

```rust
    if alerts::should_alert(value, threshold, alerting.contains(key)) {
        alerting.insert(key.to_string());
        crate::oplog!("alert_high", &format!("{} {} {}%（阈值 {}%）", name, metric, value.round(), threshold));
        let _ = app.emit(
            "resource-alert",
            serde_json::json!({ "kind": if key.starts_with("proc:") { "process" } else { "system" }, "name": name, "metric": metric, "value": value.round(), "threshold": threshold }),
        );
        super::notify::dispatch(super::notify::AlertEvent {
            name: name.to_string(),
            metric: metric.to_string(),
            value,
            threshold,
        });
    } else if alerting.contains(key) && alerts::is_recovered(value, threshold) {
```

（`eval` 保持同步 fn——`dispatch` 是同步入口。）

- [ ] **Step 3: 测试命令（commands/config.rs 末尾追加）**

```rust
/// 发送测试通知：构造固定告警事件，按当前设置对启用的渠道真实发送一遍。
/// 未配置任何渠道时报错提示。
#[tauri::command]
pub async fn test_alert_webhook() -> Result<(), String> {
    let e = crate::services::system_monitor::notify::AlertEvent {
        name: "测试".to_string(),
        metric: "cpu".to_string(),
        value: 95.0,
        threshold: 90,
    };
    let s = read_settings()?;
    let mut sent = false;
    if !s.alert_webhook_url.trim().is_empty() {
        crate::services::system_monitor::notify::send_webhook(
            &s.alert_webhook_url,
            &s.alert_webhook_format,
            &s.alert_webhook_secret,
            &e,
        )
        .await
        .map_err(|err| format!("webhook 发送失败: {:#}", err))?;
        sent = true;
    }
    if s.smtp_enabled && !s.smtp_host.trim().is_empty() && !s.smtp_to.trim().is_empty() {
        crate::services::system_monitor::notify::send_mail(&s, &e)
            .await
            .map_err(|err| format!("邮件发送失败: {:#}", err))?;
        sent = true;
    }
    if !sent {
        return Err("未配置任何通知渠道（webhook URL 为空且 SMTP 未启用）".to_string());
    }
    Ok(())
}
```

`lib.rs` 的 `invoke_handler` 里 `commands::config::test_dns_token,` 之后加一行 `commands::config::test_alert_webhook,`。

- [ ] **Step 4: 构建 + 测试**

Run: `cd src-tauri && rustfmt --edition 2021 src/services/system_monitor/notify.rs src/services/system_monitor/recorder.rs src/commands/config.rs src/lib.rs && cargo build 2>&1 | tail -5 && cargo test --lib 2>&1 | tail -3`
Expected: 编译干净（无新 warning），167 passed 不减。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/services/system_monitor/notify.rs src-tauri/src/services/system_monitor/recorder.rs src-tauri/src/commands/config.rs src-tauri/src/lib.rs
git commit -m "feat(alert): webhook/SMTP 发送层并接入告警触发链，附测试命令"
```

---

### Task 4: 设置页 UI + i18n

**Files:**
- Modify: `src/modules/settings/pages/SettingsPage.vue`
- Modify: `src/locales/zh-CN.ts`、`src/locales/en-US.ts`

**Interfaces:**
- Consumes: Task 1 的 `AppSettings` 字段、Task 3 的 `test_alert_webhook` 命令；页面既有模式（`watch` 数组自动保存、`dnsTest*` 测试按钮样式与流程）
- Produces: 无

- [ ] **Step 1: i18n 键（两文件同位置、`auditNone` 风格追加在告警相关键附近）**

`zh-CN.ts`：

```typescript
  alertNotify: '告警通知',
  webhookUrl: 'Webhook 地址',
  webhookUrlPlaceholder: 'https://oapi.dingtalk.com/robot/send?access_token=…',
  webhookFormat: 'Webhook 格式',
  webhookFormatJson: '通用 JSON',
  webhookFormatDingtalk: '钉钉机器人',
  webhookFormatWecom: '企业微信机器人',
  webhookFormatFeishu: '飞书机器人',
  dingtalkSecret: '钉钉加签 Secret（可选）',
  smtpEnabled: '启用邮件通知',
  smtpHost: 'SMTP 服务器',
  smtpPort: '端口',
  smtpUser: '发件账号',
  smtpPass: '授权码',
  smtpTo: '收件人（逗号分隔）',
  smtpPassWarning: '授权码明文存储于本机 settings.json',
  sendTestNotify: '发送测试通知',
  testNotifySending: '发送中…',
  testNotifyNoChannel: '请先配置 webhook 或启用 SMTP',
```

`en-US.ts`：

```typescript
  alertNotify: 'Alert Notifications',
  webhookUrl: 'Webhook URL',
  webhookUrlPlaceholder: 'https://oapi.dingtalk.com/robot/send?access_token=…',
  webhookFormat: 'Webhook format',
  webhookFormatJson: 'Generic JSON',
  webhookFormatDingtalk: 'DingTalk bot',
  webhookFormatWecom: 'WeCom bot',
  webhookFormatFeishu: 'Feishu bot',
  dingtalkSecret: 'DingTalk sign secret (optional)',
  smtpEnabled: 'Enable email',
  smtpHost: 'SMTP server',
  smtpPort: 'Port',
  smtpUser: 'From account',
  smtpPass: 'Password / auth code',
  smtpTo: 'Recipients (comma separated)',
  smtpPassWarning: 'Auth code is stored in plaintext in local settings.json',
  sendTestNotify: 'Send test notification',
  testNotifySending: 'Sending…',
  testNotifyNoChannel: 'Configure webhook or enable SMTP first',
```

- [ ] **Step 2: 模板——「告警阈值」区块之后追加「告警通知」区块**

结构与告警阈值一致（`border-t` 分隔 + h3 标题 + divide-y 行）。要点：

```html
      <div class="border-t border-border" />

      <!-- 告警通知 -->
      <div class="px-5 pt-4 pb-1">
        <h3 class="text-sm font-semibold tracking-tight">{{ $t('alertNotify') }}</h3>
      </div>
      <div class="px-5 pb-4 divide-y divide-border">
        <!-- webhook url -->
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('webhookUrl') }}</span>
          <input v-model="webhookUrlValue" class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" :placeholder="$t('webhookUrlPlaceholder')" />
        </div>
        <!-- 格式 -->
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('webhookFormat') }}</span>
          <select v-model="webhookFormatValue" class="h-8 px-2 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary cursor-pointer">
            <option value="json">{{ $t('webhookFormatJson') }}</option>
            <option value="dingtalk">{{ $t('webhookFormatDingtalk') }}</option>
            <option value="wecom">{{ $t('webhookFormatWecom') }}</option>
            <option value="feishu">{{ $t('webhookFormatFeishu') }}</option>
          </select>
        </div>
        <!-- 钉钉加签（仅 dingtalk 显示） -->
        <div v-if="webhookFormatValue === 'dingtalk'" class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('dingtalkSecret') }}</span>
          <input v-model="webhookSecretValue" type="password" class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
        </div>
        <!-- SMTP 开关 -->
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('smtpEnabled') }}</span>
          <SwitchBtn v-model="smtpEnabledValue" />
        </div>
        <!-- host / port / user / pass / to：同款行（host/to 用 w-72，port 用 w-20 number，pass 用 type=password） -->
        <!-- ……五个输入行，模式与 webhookUrl 行一致，绑定 smtpHostValue / smtpPortValue / smtpUserValue / smtpPassValue / smtpToValue …… -->
        <div v-if="smtpEnabledValue" class="flex items-start justify-between gap-4 py-3">
          <span class="text-xs text-warning max-w-72">{{ $t('smtpPassWarning') }}</span>
        </div>
        <!-- 测试按钮（沿用 dnsTest 的行内结果反馈模式） -->
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('sendTestNotify') }}</span>
          <div class="flex flex-col gap-1 items-end min-w-0">
            <button class="btn text-xs h-7 px-2" :disabled="notifyTesting" @click="testNotify">
              {{ notifyTesting ? $t('testNotifySending') : $t('sendTestNotify') }}
            </button>
            <span v-if="notifyTestResult" class="text-xs max-w-72 text-right" :class="notifyTestOk ? 'text-success' : 'text-destructive'" style="overflow-wrap: anywhere; word-break: break-word">{{ notifyTestResult }}</span>
          </div>
        </div>
      </div>
```

SMTP 五个输入行完整形态（测试按钮行之前依序放置）：

```html
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('smtpHost') }}</span>
          <input v-model="smtpHostValue" class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" placeholder="smtp.qq.com" />
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('smtpPort') }}</span>
          <input v-model.number="smtpPortValue" type="number" min="1" max="65535" class="h-8 px-2 w-20 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary" />
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('smtpUser') }}</span>
          <input v-model="smtpUserValue" class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('smtpPass') }}</span>
          <input v-model="smtpPassValue" type="password" class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('smtpTo') }}</span>
          <input v-model="smtpToValue" class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" placeholder="a@x.com, b@y.com" />
        </div>
```

- [ ] **Step 3: script——refs / watch / save / 测试函数**

refs（与告警阈值 refs 相邻放置）：

```typescript
const webhookUrlValue = ref('')
const webhookFormatValue = ref('json')
const webhookSecretValue = ref('')
const smtpEnabledValue = ref(false)
const smtpHostValue = ref('')
const smtpPortValue = ref(465)
const smtpUserValue = ref('')
const smtpPassValue = ref('')
const smtpToValue = ref('')
const notifyTesting = ref(false)
const notifyTestResult = ref('')
const notifyTestOk = ref(false)
```

settings watch 的 `(s) => { … }` 内追加：

```typescript
      webhookUrlValue.value = s.alert_webhook_url || ''
      webhookFormatValue.value = s.alert_webhook_format || 'json'
      webhookSecretValue.value = s.alert_webhook_secret || ''
      smtpEnabledValue.value = s.smtp_enabled
      smtpHostValue.value = s.smtp_host || ''
      smtpPortValue.value = s.smtp_port ?? 465
      smtpUserValue.value = s.smtp_user || ''
      smtpPassValue.value = s.smtp_pass || ''
      smtpToValue.value = s.smtp_to || ''
```

自动保存 watch 的源数组追加九个 ref；`save()` 内（`alert_process_mem` 赋值之后）追加：

```typescript
  // 端口输入清空时 v-model.number 给 ''，归一回落 465（同告警阈值的 pct 兜底逻辑）
  smtpPortValue.value = Number(smtpPortValue.value) >= 1 && Number(smtpPortValue.value) <= 65535 ? Number(smtpPortValue.value) : 465
  settingsStore.settings.alert_webhook_url = webhookUrlValue.value
  settingsStore.settings.alert_webhook_format = webhookFormatValue.value
  settingsStore.settings.alert_webhook_secret = webhookSecretValue.value
  settingsStore.settings.smtp_enabled = smtpEnabledValue.value
  settingsStore.settings.smtp_host = smtpHostValue.value
  settingsStore.settings.smtp_port = smtpPortValue.value
  settingsStore.settings.smtp_user = smtpUserValue.value
  settingsStore.settings.smtp_pass = smtpPassValue.value
  settingsStore.settings.smtp_to = smtpToValue.value
```

测试函数（沿用 `testToken` 流程；**先保存当前输入再测试**，否则测的是旧配置——这是与 DNS 测试的差异点，DNS 测试参数直接传参而设置页是自动保存）：

```typescript
async function testNotify() {
  notifyTesting.value = true
  notifyTestResult.value = t('testNotifySending')
  try {
    await save() // 先持久化当前输入，后端读的是 settings.json
    await invoke('test_alert_webhook')
    notifyTestOk.value = true
    notifyTestResult.value = t('testTokenOk')
  } catch (e) {
    notifyTestOk.value = false
    notifyTestResult.value = String(e)
  } finally {
    notifyTesting.value = false
  }
}
```

（`testTokenOk` 是既有键「验证成功」，直接复用；若想区分文案可加 `testNotifyOk: '已发送'` / `'Sent'` 两键。）

- [ ] **Step 4: 类型检查**

Run: `npx vue-tsc --noEmit`
Expected: 无错误。

- [ ] **Step 5: 提交**

```bash
git add src/modules/settings/pages/SettingsPage.vue src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat(alert): 设置页通知渠道配置（webhook + SMTP + 测试按钮）"
```

---

### Task 5: 全量验证 + 实机

**Files:** 无改动（发现问题回对应 Task 修）

- [ ] **Step 1: 后端全量**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3 && cargo clippy --lib 2>&1 | tail -5`
Expected: ≥167 passed；clippy 无本分支文件的新告警。

- [ ] **Step 2: 前端全量**

Run: `npx vue-tsc --noEmit && npm run build 2>&1 | tail -5`
Expected: 通过（仅既有 chunk 大小告警）。

- [ ] **Step 3: 实机验证清单（`npm run tauri dev`）**

1. 设置页出现「告警通知」区块；格式切钉钉时加签 Secret 行出现/消失。
2. 填通用 JSON 格式 webhook（内网可自建 `nc -l` 或 httpbin 类服务收请求）→「发送测试通知」→ 收到 `{"event":"alert_high",…}`；按钮行显示成功。
3. 钉钉加签机器人：填 URL + secret → 测试 → 群里收到「[OPX 告警] 测试 cpu 95%（阈值 90%）」；**故意填错 secret → 测试 → 按钮行显示钉钉 errcode 报错**。
4. SMTP：填真实邮箱授权码 → 启用 → 测试 → 收件箱收到邮件。
5. 全不配 → 测试 → 提示「未配置任何通知渠道」。
6. 把 `alert_system_cpu` 阈值压到当前 CPU 之下触发真实告警 → webhook/邮件各收到一条（操作记录有 `alert_high`；恢复不发通知）。
7. 断开 webhook 指向（改成一个不通地址）触发告警 → 操作记录出现 `webhook_failed`，应用不卡。

- [ ] **Step 4: 修复提交（如有）**

```bash
git add -A && git commit -m "fix(alert): 实机验证问题修复"
```

---

## 附：明确不做

- 多 webhook / 多 SMTP、重试队列、恢复通知、URL 自动识别、通知历史页、webhook/SMTP 走代理。
