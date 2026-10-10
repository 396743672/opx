//! WS 事件通路（设计 §3 D3.2 + §4 D4）。
//!
//! ## 端点
//!
//! - `POST /api/ws-ticket`（Bearer 保护）：签发 60 秒单次 ticket
//! - `GET /api/events/ws?ticket=...`（**不经 Bearer 中间件**——浏览器
//!   WebSocket 无法设 Authorization 头，这正是 D3.2 ticket 机制的存在理由；
//!   由 ticket 鉴权替代）
//!
//! ## WS 消息格式定稿（供 4.4 前端 HttpTransport 对接）
//!
//! **一帧一条 JSON 文本**，信封形状（与 [`crate::sinks::WsEventSink::emit`]
//! 产出一致，与 Tauri `emit(event, payload)` 语义对齐）：
//!
//! ```json
//! {"event": "<事件名，如 software-status-changed>", "payload": {<与桌面事件相同的负载>}}
//! ```
//!
//! 前端按 `event` 名多路分发到各 listen 回调；断线重连后前端发全量刷新
//! （resync，设计 D4 补偿方案 1）——**后端不做重放**，只管广播。
//!
//! ## 生命周期
//!
//! 握手成功 → [`crate::sinks::WsEventSink::subscribe`] 取接收端挂进广播组 →
//! 循环转发（per-client bounded(64)，慢客户端丢弃不阻塞，R3）→ 客户端断开
//! → `unsubscribe` 摘除 + channel 关闭。

use crate::auth::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::collections::HashMap;

/// `POST /api/ws-ticket`：签发一次性握手 ticket（60 秒有效，单次使用）。
pub async fn issue_ticket(State(state): State<AppState>) -> Json<serde_json::Value> {
    let ticket = state.tickets.issue();
    Json(json!({
        "ticket": ticket,
        "expires_in": crate::ticket::TICKET_TTL.as_secs(),
        "ws_url": "/api/events/ws",
    }))
}

/// `GET /api/events/ws?ticket=...`：校验 ticket → 升级 WS → 挂进广播组。
pub async fn events_ws(
    Query(params): Query<HashMap<String, String>>,
    State(state): State<AppState>,
    upgrade: WebSocketUpgrade,
) -> Response {
    // ticket 鉴权（D3.2）：缺 ticket 与无效 ticket 一律 401，不做区分
    // （不帮助探测）；ticket 单次有效，consume 即焚。
    let consumed = params
        .get("ticket")
        .map(|t| state.tickets.consume(t))
        .unwrap_or(false);
    if !consumed {
        return (StatusCode::UNAUTHORIZED, "invalid ticket").into_response();
    }
    upgrade.on_upgrade(move |socket| handle_socket(socket, state))
}

/// 单连接转发循环：广播组接收端 → WS Text 帧。
async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let (id, mut rx) = state.ws.subscribe();
    loop {
        tokio::select! {
            // 广播：WsEventSink::emit 产出的信封字符串原样作一帧 Text
            forwarded = rx.recv() => {
                match forwarded {
                    Some(text) => {
                        if socket.send(Message::Text(text.into())).await.is_err() {
                            break; // 客户端已断，路由层随后 unsubscribe
                        }
                    }
                    None => break, // 广播组关闭（进程退出场景）
                }
            }
            // 客户端上行：浏览器 WS 不承载命令（命令走 POST /api/{cmd}），
            // 这里只消费帧以探测断开；Ping/Pong 由底层自动处理
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(_)) => {}          // 忽略客户端消息
                    Some(Err(_)) => break,     // 协议错误
                    None => break,             // 客户端关闭
                }
            }
        }
    }
    // 断开 → 从广播组摘除（订阅表清理，R3 的注册表防泄漏约定）
    state.ws.unsubscribe(id);
}
