//! `POST /api/{cmd}` 通用分发器（设计 §5 / D7）。
//!
//! ## 职责边界（D7 的关键约束）
//!
//! 本模块**只含分发基建**：错误信封、注册表类型、参数提取/结果包装的通用胶水、
//! axum 路由处理函数。**零业务逻辑**——真正的命令注册点在 src-tauri
//! （`commands/http_registry.rs`），那里的闭包经 `AppHandle::state::<T>()` 调用
//! 与 Tauri IPC **同一个命令函数**（tauri `State` 内部字段私有、无公开构造器，
//! opx-http 零 tauri 依赖无法凭空构造，故注册点必须唯一落在壳层；headless 复用
//! 留待设计 C 方案「用例层下沉 core」演进）。
//!
//! ## HTTP 契约（供 4.4 前端 HttpTransport 对接）
//!
//! - 请求：`POST /api/<cmd>`，body 为 JSON 对象 = Tauri `invoke(cmd, args)` 的
//!   同名参数表（如 `{"id": "..."}`）；空 body 等价 `{}`
//! - 成功：`200` + 命令返回值序列化的 JSON（与 Tauri IPC 返回形状一致）
//! - 未注册命令：`404` + `{"code":"unknown_command","message":"..."}`
//! - 参数缺失/类型不符：`400` + `{"code":"invalid_args","message":"..."}`
//! - 命令执行失败（`Result<_, String>` 的 Err）：`500` +
//!   `{"code":"command_failed","message":"<原始错误串>"}`
//!   —— `message` 保持与 Tauri reject 相同的字符串（含 `i18n:key` 形态），
//!   HttpTransport 取出后走 ipc.ts `translateError`，错误转译单一入口不变
//! - 全部 `/api/*` 受 Bearer 中间件保护（401 纯文本，见 `auth`）

use crate::auth::AppState;
use crate::context::AppContext;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// 统一错误信封（D5：前端 HttpTransport 读取 `message` 走 translateError）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiError {
    /// 机器可读错误码；业务命令错误固定 `command_failed`（原始串在 message）
    pub code: String,
    /// 人读消息（与 Tauri IPC reject 的字符串一致，含 `i18n:key` 形态）
    pub message: String,
}

impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    /// 未注册命令 → 404
    pub fn unknown_command(cmd: &str) -> Self {
        Self::new("unknown_command", format!("未注册的命令: {cmd}"))
    }

    /// 参数缺失 / 类型不符 → 400
    pub fn invalid_args(message: impl Into<String>) -> Self {
        Self::new("invalid_args", message)
    }

    /// 命令执行失败（`Result<_, String>` 的 Err）→ 500
    pub fn command_failed(message: impl Into<String>) -> Self {
        Self::new("command_failed", message)
    }

    /// code → HTTP 状态码映射（分发器统一裁定，注册点不感知 HTTP 语义）
    fn status(&self) -> StatusCode {
        match self.code.as_str() {
            "unknown_command" => StatusCode::NOT_FOUND,
            "invalid_args" => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status(), Json(self)).into_response()
    }
}

/// 单个命令处理器返回的 boxed future（跨 await、Send，注册点闭包产出）。
pub type BoxFut = Pin<Box<dyn Future<Output = Result<Value, ApiError>> + Send>>;

/// 命令处理器：收共享上下文 + JSON 参数表，返回 JSON 结果或统一错误。
///
/// 注册点闭包**同步**部分只做参数预取（clone Arc 等），业务 await 全部在
/// 返回的 future 内——future 必须 `'static`，故不得捕获 `&AppContext` 的借用。
pub type Handler = Arc<dyn Fn(&AppContext, Value) -> BoxFut + Send + Sync>;

/// 命令注册表：cmd 名 → 处理器。构建期填充（`register`），运行期只读。
///
/// `Clone` 语义 = 共享同一张表（HashMap 里的值全是 `Arc<Handler>`，克隆廉价），
/// 便于整体塞进 `AppState`。
#[derive(Clone, Default)]
pub struct Registry {
    handlers: HashMap<&'static str, Handler>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个命令（构建期调用；同名重复注册 panic——注册点写错的显式失败）。
    pub fn register<F>(&mut self, cmd: &'static str, f: F)
    where
        F: Fn(&AppContext, Value) -> BoxFut + Send + Sync + 'static,
    {
        let replaced = self.handlers.insert(cmd, Arc::new(f)).is_some();
        assert!(!replaced, "命令重复注册: {cmd}");
    }

    /// 查命令（分发器用）。
    pub fn get(&self, cmd: &str) -> Option<&Handler> {
        self.handlers.get(cmd)
    }

    /// 已注册命令数（测试/监控用）。
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    /// 已注册命令名列表（诊断/测试用）。
    pub fn command_names(&self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = self.handlers.keys().copied().collect();
        names.sort_unstable();
        names
    }
}

/// 从 JSON 参数表提取具名参数（Tauri invoke 的 args 形状 = `{"id": ...}`）。
///
/// 缺失或类型不符一律 `invalid_args`（400），错误消息带参数名便于前端定位。
pub fn arg<T: DeserializeOwned>(args: &Value, key: &str) -> Result<T, ApiError> {
    serde_json::from_value(args.get(key).cloned().unwrap_or(Value::Null))
        .map_err(|e| ApiError::invalid_args(format!("参数 {key} 缺失或类型不符: {e}")))
}

/// 包装命令返回值：`Result<T, String>` → JSON / `command_failed`。
///
/// 错误串原样透传（保持 `i18n:key` 形态），由前端 translateError 统一转译。
pub fn ok<T: Serialize>(r: Result<T, String>) -> Result<Value, ApiError> {
    r.map(|v| serde_json::to_value(v).unwrap_or(Value::Null))
        .map_err(ApiError::command_failed)
}

/// 包装无 `Result` 的命令返回值（同步纯读命令直接返回值）→ JSON。
pub fn val<T: Serialize>(v: T) -> Result<Value, ApiError> {
    Ok(serde_json::to_value(v).unwrap_or(Value::Null))
}

/// `POST /api/{cmd}` 分发器：查注册表 → 解析 body → 调处理器。
///
/// 顺序：先 404（未注册）再 400（坏参数）——未注册命令不浪费参数解析。
pub async fn dispatch(
    State(state): State<AppState>,
    Path(cmd): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    use axum::response::IntoResponse;

    let Some(handler) = state.registry.get(&cmd) else {
        return ApiError::unknown_command(&cmd).into_response();
    };

    // 空 body 等价 `invoke(cmd)`（无参调用）；非空但非合法 JSON → 400
    let args: Value = if body.is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => return ApiError::invalid_args(format!("请求体不是合法 JSON: {e}")).into_response(),
        }
    };

    match handler(&state.ctx, args).await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => e.into_response(),
    }
}
