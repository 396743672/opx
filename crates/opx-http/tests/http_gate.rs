//! 集成测试：opx-http 骨架的鉴权门禁（批次 4.0 验收，R1 钉死）。
//!
//! 关键证明点：axum server 能在**普通 tokio runtime**（`#[tokio::test]`）上
//! 启动并完成「带 token 200 / 错 token 401」往返 —— tauri 的
//! `async_runtime` 底层就是共享 tokio runtime（设计 F4），桌面侧
//! `tauri::async_runtime::spawn(serve(..))` 的同宿可行性由本测试钉死。

use opx_http::{token, AppState};

/// 起 server（127.0.0.1:0 随机端口）→ 返回 `(实际地址, token)`。
///
/// 端口 0 由内核分配，测试间无冲突；D6 的「端口占用明确报错」由 `bind` 的
/// `Err` 路径承担，此处不触发。token 仅存于测试进程内存，不落盘、不入 URL
/// —— 这正是设计 D3 对 token 泄漏面的约定。
async fn spawn_server(token: String) -> (std::net::SocketAddr, String) {
    let (addr, serve_fut) = opx_http::bind(
        std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
        AppState::new(token.clone()),
    )
    .await
    .expect("绑定 127.0.0.1:0 不应失败");
    tokio::spawn(serve_fut);
    (addr, token)
}

/// 带 token 打通 health：200 + {"status":"ok"}。
#[tokio::test]
async fn health_with_valid_token() {
    let (addr, token) = spawn_server(token::generate()).await;
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
    let (addr, _token) = spawn_server(token::generate()).await;
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
    let (addr, _token) = spawn_server(token::generate()).await;
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
/// 语义，不碰磁盘（写盘持久化由 token 单测 + 4.2 settings 集成覆盖）。
#[tokio::test]
async fn token_stays_stable_across_connections() {
    let t = token::generate();
    let (addr, token) = spawn_server(t.clone()).await;
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
