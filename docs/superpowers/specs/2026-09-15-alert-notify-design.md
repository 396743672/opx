# 告警通知渠道（Webhook + SMTP）设计

> 状态：已确认设计
> 日期：2026-09-15
> 范围：资源告警（alert_high 触发时）的外部通知渠道

## 背景

指标历史 + 告警规则（2026-09-14 落地）已有完整触发链路：`recorder.rs` 30s 采样 → `eval` 判定 → 触发时写审计 + emit `resource-alert`（前端 toast）。但通知只到本机 UI——人不在电脑前就看不到。本期把触发通知外送到 webhook（钉钉/企微/飞书/通用 JSON）和邮件（SMTP）。

## 已确认决策

1. **事件范围 = 资源告警**：仅 `alert_high` 触发时发送；`alert_recovered` 恢复不发送（只写审计 + 本机 toast，保持现状）。
2. **渠道**：Webhook（单 URL）+ SMTP 邮件，两者并行独立，一个失败不影响另一个。
3. **Webhook 格式由用户在设置页选择**（不自动识别 URL）：通用 JSON / 钉钉 / 企业微信 / 飞书。
4. **钉钉支持加签**（可选 secret；空 = 不加签）。
5. **失败处理**：不重试。HTTP 非 2xx / SMTP 错误 → `tracing::warn` + 审计一条 `webhook_failed`（detail 注明渠道），不阻断告警链。
6. **测试按钮**：webhook「发送测试通知」+ SMTP「发送测试邮件」，均构造假事件走真实发送链路（沿用 ACME「测试 Token」的真实验证哲学）。

## 设计

### 1. 配置（`models/settings.rs` + `src/models/settings.ts`）

`AppSettings` 增 9 字段，全部 `#[serde(default)]`（旧 settings.json 零迁移）：

```rust
// --- Webhook ---
/// 告警 webhook URL，空 = 不发送
#[serde(default)]
pub alert_webhook_url: String,
/// "json" | "dingtalk" | "wecom" | "feishu"，默认 "json"
#[serde(default = "default_webhook_format")]
pub alert_webhook_format: String,
/// 钉钉加签 secret（仅 dingtalk 格式生效，空 = 不加签）
#[serde(default)]
pub alert_webhook_secret: String,
// --- SMTP ---
/// SMTP 服务器（如 smtp.qq.com），enabled 且非空才发
#[serde(default)]
pub smtp_host: String,
#[serde(default = "default_smtp_port")]
pub smtp_port: u16,          // 默认 465（TLS）
#[serde(default)]
pub smtp_user: String,       // 发件账号
/// SMTP 授权码（明文存储，UI 提示风险——与 cloudflare token 同策略）
#[serde(default)]
pub smtp_pass: String,
/// 收件人，逗号分隔多个
#[serde(default)]
pub smtp_to: String,
/// SMTP 总开关：false 时即使填了配置也不发
#[serde(default)]
pub smtp_enabled: bool,
```

TypeScript 侧同步增字段（`string` / `number` / `boolean`）。

### 2. 发送模块 `src-tauri/src/services/system_monitor/notify.rs`（新）

```rust
/// 一条告警事件（触发时从 eval 传入；测试按钮构造假值）
pub struct AlertEvent {
    pub name: String,     // "整机" / 进程显示名
    pub metric: String,   // "cpu" | "mem"
    pub value: f64,       // 百分比
    pub threshold: u32,
}

/// 统一文案（webhook text 与邮件标题正文共用）
pub fn alert_text(e: &AlertEvent) -> String   // "[OPX 告警] {name} cpu 95%（阈值 90%）"

/// 纯函数：按格式构造 (最终 URL, 请求体 JSON) —— 单测主战场
pub fn build_payload(format: &str, secret: &str, ts_millis: i64, e: &AlertEvent) -> (String, String)

/// 纯函数：钉钉加签 = HMAC-SHA256(secret, "{ts}\n{secret}") → base64 → urlencode
pub fn sign_dingtalk(secret: &str, ts_millis: i64) -> String

/// 入口：读最新设置，两个渠道各自判断启用后并行发送（tokio::spawn 各自游离，不阻塞采样轮询）。
/// 失败：tracing::warn + oplog!("webhook_failed", 文案, 渠道+原因)。
pub async fn dispatch(app: &AppHandle, e: AlertEvent)
```

**各格式请求体**（各家机器人最简 text 消息）：

| 格式 | body | URL |
|---|---|---|
| `json` | `{"event":"alert_high","name":…,"metric":…,"value":…,"threshold":…,"ts":…}` | 原样 |
| `dingtalk` | `{"msgtype":"text","text":{"content":"[OPX 告警] …"}}` | secret 非空时追加 `&timestamp={ts}&sign={sign_dingtalk(...)}` |
| `wecom` | `{"msgtype":"text","text":{"content":"[OPX 告警] …"}}` | 原样 |
| `feishu` | `{"msg_type":"text","content":{"text":"[OPX 告警] …"}}` | 原样 |

**HTTP**：复用项目已有的 `reqwest`（下载/ACME 已依赖，不新增），`Client::new().post(url).json(body).timeout(10s).send()`，非 2xx 视为失败。

**SMTP**：新增依赖 `lettre`（with `tokio1` + `rustls-tls` feature；发邮件无法手写 TLS，这是本设计唯一新增 crate）。

```rust
async fn send_mail(smtp: &SmtpSettings, e: &AlertEvent) -> anyhow::Result<()> {
    // Message::builder().from(user).to(逐个解析 smtp_to 逗号分隔).subject(标题)
    //   .body(正文) → SmtpTransport::relay(host)?.port(port).credentials(...).build()
    //   .send(Message).await
    // 标题 "[OPX 告警] {name} {metric} 超阈值"；正文 = alert_text(e) + RFC3339 时间
}
```

### 3. 触发点（`recorder.rs::eval` 一处收口）

`should_alert` 命中分支：现有 `oplog!("alert_high", …)` + `emit("resource-alert")` 之后加：

```rust
notify::dispatch(app, AlertEvent { name, metric, value, threshold }).await;
```

`eval` 目前是同步 fn 在 async `sample_once` 里被调用——改为直接 `async fn`（调用方本就在 async 上下文，无需 spawn 包裹）。`dispatch` 内部对两个渠道各自 `tokio::spawn` 游离发送（30s 轮询不能被外部网络 IO 卡住，timeout 10s 兜底）。

### 4. 测试命令（`commands/config.rs`）

```rust
/// 发送测试通知/邮件：构造固定 AlertEvent（name="测试", cpu 95%, 阈值 90%）走真实 dispatch。
/// 返回 Result<(), String>，成功即链路通。
#[tauri::command]
pub async fn test_alert_webhook() -> Result<(), String>
```

一个命令同时测两个渠道（按当前设置启用情况发送），结果 toast 提示。失败返回具体原因（HTTP 状态码 / SMTP 错误文本）。

### 5. 设置页（`SettingsPage.vue`）

告警阈值区下方新增「告警通知」区块，两组：

**Webhook 组**：
- URL 输入框（placeholder 提示钉钉/企微/飞书机器人 URL 形态）
- 格式下拉：通用 JSON / 钉钉 / 企业微信 / 飞书
- 加签 Secret 输入框（仅格式 = 钉钉时显示，`v-if`）
- 「发送测试通知」按钮

**SMTP 组**：
- 启用开关
- 服务器 / 端口（默认 465）/ 账号 / 授权码（type=password）/ 收件人输入框
- 「发送测试邮件」按钮

授权码输入框旁小字提示明文存储风险（与 Cloudflare Token 同文案风格）。

### 6. 测试

单测（`cargo test --lib`，notify.rs 内）：
- `build_payload` 四种格式：JSON 结构关键字段、钉钉/企微/飞书的 msgtype 与文案、钉钉 secret 非空时 URL 含 timestamp+sign；
- `sign_dingtalk`：固定 secret + 固定 ts 的已知签值（按钉钉文档算法手算一个基准值）；
- `alert_text` 文案格式；
- SMTP 收件人逗号分隔解析纯函数（`parse_recipients`）。

前端 `npx vue-tsc --noEmit`。

实机验证：钉钉（加签）真实收一条、通用 JSON POST 到 httpbin 类服务、SMTP 真发一封（用户环境验）、阈值压低触发真实告警链路。

## 边界（不做）

- 不做多 webhook / 多 SMTP 配置。
- 不做重试队列（失败仅记审计 + warn）。
- 不做恢复（alert_recovered）通知。
- 不做按格式自动识别 URL。
- 不做通知历史页（发送结果在操作记录里可见）。
- 不做代理支持（webhook/SMTP 走系统直连；用户内网代理场景出现再说）。

## 改动文件清单

- `src-tauri/Cargo.toml`（+lettre）
- `src-tauri/src/models/settings.rs`、`src/models/settings.ts`（9 字段）
- `src-tauri/src/services/system_monitor/notify.rs`（新）
- `src-tauri/src/services/system_monitor/{recorder,mod}.rs`（触发接线）
- `src-tauri/src/commands/config.rs`（test_alert_webhook 命令）
- `src/modules/settings/pages/SettingsPage.vue`
- `src/locales/zh-CN.ts`、`src/locales/en-US.ts`
