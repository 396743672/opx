//! 集成测试：opx-http 的鉴权门禁与命令分发器（批次 4.0/4.2 验收，R1 钉死）。
//!
//! 关键证明点：axum server 能在**普通 tokio runtime**（`#[tokio::test]`）上
//! 启动并完成「带 token 200 / 错 token 401 / 分发 404/400/500」往返 —— tauri 的
//! `async_runtime` 底层就是共享 tokio runtime（设计 F4），桌面侧
//! `tauri::async_runtime::spawn(serve(..))` 的同宿可行性由本测试钉死。

use opx_core::event::EventSink; // ws.emit 是 trait 方法，测试侧需在作用域内
use opx_http::api::{arg, ok, Registry};
use opx_http::{token, AppContext, AppState, TicketStore, WsEventSink};
use std::sync::Arc;

/// 测试用空事件出口（分发器测试不关心事件，只占位满足 AppContext 字段）。
struct NoopSink;
impl opx_core::event::EventSink for NoopSink {
    fn emit(&self, _: &str, _: serde_json::Value) {}
}

/// 构造真实管理器的测试 AppContext（D7-A 全字段；管理器 ::new() 只读缺省
/// 文件，缺文件回默认值，无写入副作用）。
fn test_context() -> Arc<AppContext> {
    let software = Arc::new(opx_core::services::software_manager::SoftwareManager::new());
    let springboot = Arc::new(opx_core::services::springboot_manager::SpringBootManager::new());
    Arc::new(AppContext {
        software: software.clone(),
        website: Arc::new(opx_core::services::website_manager::WebsiteManager::new()),
        springboot: springboot.clone(),
        node: Arc::new(opx_core::services::node_app_manager::NodeAppManager::new()),
        dns: Arc::new(opx_core::services::dns_account::DnsAccountManager::new()),
        stack: Arc::new(opx_core::services::stack_manager::StackManager::new(
            software, springboot,
        )),
        sink: Arc::new(NoopSink),
        node_exe: None,
    })
}

/// 测试注册表：三个探针命令覆盖分发器的全部响应路径（200/400/500）+ 一个
/// desktop_only 形态探针（409）。
fn test_registry() -> Registry {
    let mut reg = Registry::new();
    // 有参命令：验证 arg 提取 + 200 包装
    reg.register("echo", |_ctx, args| {
        Box::pin(async move {
            let msg: String = arg(&args, "msg")?;
            ok(Ok::<serde_json::Value, String>(serde_json::json!({ "echo": msg })))
        })
    });
    // 无参命令：验证 AppContext 穿透（D7-A 接线）+ 空 body 等价 invoke(cmd)
    // （Handler 的 future 是 'static：对 &AppContext 只能同步预取，不得捕获借用）
    reg.register("ctx_probe", |ctx, _args| {
        let node_exe_is_none = ctx.node_exe.is_none();
        Box::pin(async move {
            ok(Ok::<serde_json::Value, String>(serde_json::json!({
                "node_exe_is_none": node_exe_is_none,
                "software_ready": true,
            })))
        })
    });
    // 业务失败命令：验证 Result<_, String> 的 Err → 500 + command_failed
    reg.register("failing_cmd", |_ctx, _args| {
        Box::pin(async move {
            ok::<serde_json::Value>(Err("i18n:someBusinessError".to_string()))
        })
    });
    // 桌面专属形态：handler 直接返回 desktop_only 信封（批次 4.3 语义）
    reg.register("quit_app", |_ctx, _args| {
        Box::pin(async move { Err(opx_http::ApiError::desktop_only("quit_app")) })
    });
    reg
}

/// 起 server（127.0.0.1:0 随机端口）→ 返回 `(实际地址, token, ws 广播 sink)`。
///
/// 端口 0 由内核分配，测试间无冲突；D6 的「端口占用明确报错」由 `bind` 的
/// `Err` 路径承担，此处不触发。token/ticket 仅存于测试进程内存，不落盘、
/// 不入 URL —— 这正是设计 D3 对 token 泄漏面的约定。
async fn spawn_server(token: String) -> (std::net::SocketAddr, String, Arc<WsEventSink>) {
    let ws = WsEventSink::shared();
    let (addr, serve_fut) = opx_http::bind(
        std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
        AppState::new(token.clone(), test_context(), test_registry(), ws.clone(), Arc::new(TicketStore::new())),
    )
    .await
    .expect("绑定 127.0.0.1:0 不应失败");
    tokio::spawn(serve_fut);
    (addr, token, ws)
}

/// 带 token 打通 health：200 + {"status":"ok"}。
#[tokio::test]
async fn health_with_valid_token() {
    let (addr, token, _ws) = spawn_server(token::generate()).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("http://{addr}/api/health"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("请求已启动的 server 不应失败");
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

/// 缺 token → 401。
#[tokio::test]
async fn health_without_token_is_401() {
    let (addr, _token, _ws) = spawn_server(token::generate()).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("http://{addr}/api/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

/// 错 token → 401（与缺 token 响应一致，不帮助探测）。
#[tokio::test]
async fn health_with_wrong_token_is_401() {
    let (addr, _token, _ws) = spawn_server(token::generate()).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("http://{addr}/api/health"))
        .header("Authorization", "Bearer deadbeef")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

/// 「存活探针」语义的完整验证：load_or_generate 后紧邻两次 connect 之间
/// token 保持稳定（ADR：手动重置才轮换）。本测试只验证 generate 稳定性
/// 语义，不碰磁盘（写盘持久化由 token 单测覆盖）。
#[tokio::test]
async fn token_stays_stable_across_connections() {
    let t = token::generate();
    let (addr, token, _ws) = spawn_server(t.clone()).await;
    assert_eq!(token, t, "spawn 不应轮换传入的 token");

    let client = reqwest::Client::new();
    for _ in 0..2 {
        let res = client
            .get(format!("http://{addr}/api/health"))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200, "同一 token 两次连接都应通过");
    }
}

/// 分发器：已注册命令 + 正确参数 → 200 + 命令返回值 JSON（批次 4.2）。
#[tokio::test]
async fn dispatch_registered_command_returns_200() {
    let (addr, token, _ws) = spawn_server(token::generate()).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("http://{addr}/api/echo"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "msg": "hello" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["echo"], "hello");
}

/// 分发器：AppContext 穿透分发器抵达处理器（D7-A 接线证明）；空 body
/// 等价 `invoke(cmd)` 无参调用 → 200。
#[tokio::test]
async fn dispatch_empty_body_and_context_passthrough() {
    let (addr, token, _ws) = spawn_server(token::generate()).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("http://{addr}/api/ctx_probe"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Vec::new()) // 空 body
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "空 body 应等价无参调用");
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["node_exe_is_none"], true);
    assert_eq!(body["software_ready"], true);
}

/// 分发器：未注册命令 → 404 + unknown_command 信封（前端 translateError 可衔接）。
#[tokio::test]
async fn dispatch_unknown_command_is_404() {
    let (addr, token, _ws) = spawn_server(token::generate()).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("http://{addr}/api/no_such_cmd"))
        .header("Authorization", format!("Bearer {token}"))
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["code"], "unknown_command");
    assert!(body["message"].as_str().unwrap().contains("no_such_cmd"));
}

/// 分发器：已注册命令但参数缺失 → 400 + invalid_args 信封。
#[tokio::test]
async fn dispatch_bad_args_is_400() {
    let (addr, token, _ws) = spawn_server(token::generate()).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("http://{addr}/api/echo"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "wrong_key": 1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["code"], "invalid_args");
    assert!(body["message"].as_str().unwrap().contains("msg"));
}

/// 分发器：请求体非法 JSON → 400。
#[tokio::test]
async fn dispatch_malformed_json_is_400() {
    let (addr, token, _ws) = spawn_server(token::generate()).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("http://{addr}/api/echo"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body("not-json{{")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["code"], "invalid_args");
}

/// 分发器：命令执行失败（Result 的 Err）→ 500 + command_failed 信封，
/// message 原样透传（i18n:key 形态留给前端 translateError）。
#[tokio::test]
async fn dispatch_command_error_is_500_with_envelope() {
    let (addr, token, _ws) = spawn_server(token::generate()).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("http://{addr}/api/failing_cmd"))
        .header("Authorization", format!("Bearer {token}"))
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["code"], "command_failed");
    assert_eq!(body["message"], "i18n:someBusinessError");
}

/// 分发器同样受 Bearer 保护：缺 token 的 POST → 401（中间件全量生效）。
#[tokio::test]
async fn dispatch_requires_bearer() {
    let (addr, _token, _ws) = spawn_server(token::generate()).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("http://{addr}/api/echo"))
        .json(&serde_json::json!({ "msg": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// ===== 批次 4.3：ws-ticket + WS 事件通路 + desktop_only =====

/// 换取 ticket 的辅助：POST /api/ws-ticket（Bearer）→ ticket 字符串。
async fn fetch_ticket(addr: std::net::SocketAddr, token: &str) -> String {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{addr}/api/ws-ticket"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("ws-ticket 请求不应失败");
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    body["ticket"].as_str().expect("ticket 为字符串").to_string()
}

/// ws-ticket 需要 Bearer 保护：缺 token → 401。
#[tokio::test]
async fn ws_ticket_requires_bearer() {
    let (addr, _token, _ws) = spawn_server(token::generate()).await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{addr}/api/ws-ticket"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

/// WS 全链路：Bearer 换 ticket → 握手 → 经 WsEventSink emit → 客户端收到
/// `{"event","payload"}` 信封帧（D3.2 + D4 通路钉死）。message 格式定稿的
/// 回归证明。
#[tokio::test]
async fn ws_roundtrip_broadcasts_envelope_frame() {
    let (addr, token, ws) = spawn_server(token::generate()).await;
    let ticket = fetch_ticket(addr, &token).await;

    let (mut client, _resp) = tokio_tungstenite::connect_async(format!(
        "ws://{addr}/api/events/ws?ticket={ticket}"
    ))
    .await
    .expect("合法 ticket 握手应成功");

    // 服务端侧经同一个广播 sink 发事件（等价于命令/后台任务 emit）
    ws.emit(
        "software-status-changed",
        serde_json::json!({ "id": "abc", "status": "Running" }),
    );

    use futures_util::StreamExt;
    let msg = tokio::time::timeout(std::time::Duration::from_secs(5), client.next())
        .await
        .expect("5 秒内应收到广播帧")
        .expect("流未结束")
        .expect("帧非错误");
    let text = msg.into_text().expect("应为文本帧");
    let v: serde_json::Value = serde_json::from_str(&text).expect("帧为合法 JSON");
    assert_eq!(v["event"], "software-status-changed");
    assert_eq!(v["payload"]["id"], "abc");
    assert_eq!(v["payload"]["status"], "Running");
}

/// 双客户端同收广播（D4 fan-out 语义经真实 WS 验证）。
#[tokio::test]
async fn ws_two_clients_both_receive() {
    let (addr, token, ws) = spawn_server(token::generate()).await;
    use futures_util::StreamExt;

    let ticket_a = fetch_ticket(addr, &token).await;
    let ticket_b = fetch_ticket(addr, &token).await;
    let (mut a, _) = tokio_tungstenite::connect_async(format!(
        "ws://{addr}/api/events/ws?ticket={ticket_a}"
    ))
    .await
    .unwrap();
    let (mut b, _) = tokio_tungstenite::connect_async(format!(
        "ws://{addr}/api/events/ws?ticket={ticket_b}"
    ))
    .await
    .unwrap();

    ws.emit("install-progress", serde_json::json!({ "phase": "done" }));

    for (name, sock) in [("a", &mut a), ("b", &mut b)] {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), sock.next())
            .await
            .expect("双客户端均应在 5 秒内收到帧")
            .expect("流未结束")
            .expect("帧非错误");
        let v: serde_json::Value = serde_json::from_str(&msg.into_text().unwrap()).unwrap();
        assert_eq!(v["event"], "install-progress", "客户端 {name} 收到广播");
    }
}

/// ticket 单次有效：同一 ticket 第二次握手被拒（D3.2 用后即焚）。
#[tokio::test]
async fn ws_ticket_is_single_use() {
    let (addr, token, _ws) = spawn_server(token::generate()).await;
    let ticket = fetch_ticket(addr, &token).await;

    let (first, _) = tokio_tungstenite::connect_async(format!(
        "ws://{addr}/api/events/ws?ticket={ticket}"
    ))
    .await
    .expect("首次握手成功");
    drop(first);

    // 二次使用同一 ticket → 握手被拒（服务端 401 → upgrade 不发生）
    let second = tokio_tungstenite::connect_async(format!(
        "ws://{addr}/api/events/ws?ticket={ticket}"
    ))
    .await;
    assert!(second.is_err(), "已用 ticket 的第二次握手必须失败");
}

/// 桌面专属命令形态：409 + `desktop_only` 信封（批次 4.3 语义钉死）。
#[tokio::test]
async fn desktop_only_command_returns_409_envelope() {
    let (addr, token, _ws) = spawn_server(token::generate()).await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{addr}/api/quit_app"))
        .header("Authorization", format!("Bearer {token}"))
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 409);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["code"], "desktop_only");
    assert!(body["message"].as_str().unwrap().contains("quit_app"));
}
