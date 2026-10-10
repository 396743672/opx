//! opx-http —— opx 的 HTTP/WS 入口（阶段 4）。
//!
//! 设计文档：`docs/2026-10-10-opx-http-design.md`（D1 选型 axum、D3 token、
//! D4 FanOutSink/WsEventSink、D7 AppContext 共享胶水）。与 `opx-core` 的
//! 关系：单向依赖（opx-http → opx-core），零 tauri 依赖——桌面壳与未来
//! headless 壳共用本 crate 提供 web 入口。
//!
//! ## 本 crate 的职责边界
//!
//! - [`token`]：token 生成 / 常量时间校验 / 受限写盘
//! - [`sinks`]：[`sinks::FanOutSink`]（多入口组合广播）与
//!   [`sinks::WsEventSink`]（WS 广播，R3 背压对策）
//! - [`auth`]：Bearer 中间件
//! - [`router`] / [`serve`]：axum 路由装配与启动（4.2/4.3 批次扩展
//!   `POST /api/:cmd` 分发器与 WS 端点，本批只挂 `/api/health`）
//!
//! ## R1：runtime 归属（本批次钉死）
//!
//! axum server **必须跑在宿主已有的 tokio runtime 上**，禁止自建：
//! - 桌面模式：一律 `tauri::async_runtime::spawn`（tauri 的 async_runtime
//!   底层就是共享 tokio runtime，见设计 F4）
//! - headless 模式：`#[tokio::main]` 直启
//! - 集成测试 `tests/http_gate.rs` 验证 server 能在普通 `#[tokio::test]`
//!   runtime 上启动并完成鉴权往返——这就是「同宿可行」的回归证明

pub mod auth;
pub mod sinks;
pub mod token;

use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

pub use auth::AppState;
pub use sinks::{FanOutSink, WsEventSink};

/// `/api/health`：无业务语义的存活探针（受 Bearer 保护）。
///
/// 返回 `{"status":"ok"}` —— 集成测试与设置页「打开浏览器」入口都用它验证
/// 「token 有效 + server 可达」。
async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

/// 装配 axum 路由：`/api/*` 全部经 Bearer 中间件。
///
/// 4.2/4.3 批次在 `api_routes` 里追加 `POST /:cmd` 分发器与 WS 端点。
pub fn router(state: AppState) -> Router {
    let api_routes = Router::new().route("/api/health", get(health));

    Router::new()
        .merge(api_routes)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_bearer,
        ))
        .with_state(state)
}

/// 在给定 listener 上启动 server（薄封装 `axum::serve`）。
///
/// 调用方负责 runtime 归属（R1）：桌面侧 `tauri::async_runtime::spawn(serve(..))`，
/// headless 侧在 `#[tokio::main]` 里 `.await` 或 `tokio::spawn`。
pub async fn serve(
    listener: tokio::net::TcpListener,
    app: Router,
) -> std::io::Result<()> {
    axum::serve(listener, app).await
}

/// 便捷装配：绑定 `addr` 并返回 `(本地地址, serve future)`。
///
/// 供测试与 4.5 批次的 headless 横幅（需先知道实际端口再打印 URL）使用。
/// 端口冲突在此处显式报错（设计 D6：明确报错，不自动重试）。
pub async fn bind(
    addr: std::net::SocketAddr,
    state: AppState,
) -> std::io::Result<(std::net::SocketAddr, std::pin::Pin<Box<dyn std::future::Future<Output = std::io::Result<()>> + Send>>)> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local = listener.local_addr()?;
    let app = router(state);
    Ok((local, Box::pin(serve(listener, app))))
}

/// 单测：路由装配本身不 panic、health 路由存在。
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_is_wired_behind_bearer() {
        let state = AppState::new(crate::token::generate());
        let app = router(state.clone());

        // 无 token → 401
        let res = app
            .clone()
            .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        // 正确 token → 200 + {"status":"ok"}
        let res = app
            .oneshot(
                Request::get("/api/health")
                    .header("Authorization", format!("Bearer {}", state.token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        assert_eq!(body.as_ref(), br#"{"status":"ok"}"#);
    }
}
