//! Bearer 鉴权中间件（设计 §3 D3.3：统一 `Authorization: Bearer <token>`，
//! 不做 cookie/session，免疫 CSRF）。
//!
//! 所有 `/api/*` 路由经本中间件；WS 握手走一次性 ticket（D3.2，4.3 批次接入
//! `/api/ws-ticket` + query 参数），不复用本中间件。

use crate::api::Registry;
use crate::context::AppContext;
use crate::token;
use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

/// axum 路由共享状态。
#[derive(Clone)]
pub struct AppState {
    /// 期望的 token（hex 64 字符）。`Arc<String>` 便于未来热替换（token 重置）。
    pub token: Arc<String>,
    /// 命令层共享上下文（D7-A：六管理器 + sink + node_exe）
    pub ctx: Arc<AppContext>,
    /// 命令注册表（注册点在 src-tauri，见 `api` 模块文档）
    pub registry: Registry,
}

impl AppState {
    pub fn new(token: String, ctx: Arc<AppContext>, registry: Registry) -> Self {
        Self {
            token: Arc::new(token),
            ctx,
            registry,
        }
    }
}

/// 从请求头提取并校验 Bearer token；通过则放行，否则 401。
///
/// 401 响应体固定为纯文本 `unauthorized`，不带任何提示性差异（token 错误 /
/// 缺失返回一致，不帮助探测）。
pub async fn require_bearer(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|presented| token::verify(presented, &state.token))
        .unwrap_or(false);
    if ok {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}
