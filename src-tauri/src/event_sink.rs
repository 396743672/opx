//! 桌面壳的 [`EventSink`] 实现：把 core 的事件转发给前端（经 Tauri 事件总线）。

/// 壳层重新导出 core 的事件 trait，使commands 层可用
/// `crate::event_sink::{EventSink, EventSinkExt}` 单一路径同时拿到 trait 与
/// `emit_ser` 扩展方法（`pub use` 同时把名字引入本模块作用域）。
pub use opx_core::event::{EventSink, EventSinkExt};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

/// 包装 `AppHandle`，让 core 内的服务无需依赖 Tauri 即可推送事件。
///
/// 行为与原先直接 `app.emit(...)` 完全一致：投递失败（无订阅者等）静默忽略，
/// 事件推送不应影响业务流程。
pub struct TauriEventSink {
    app: AppHandle,
}

impl TauriEventSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl EventSink for TauriEventSink {
    fn emit(&self, event: &str, payload: Value) {
        let _ = self.app.emit(event, payload);
    }

    /// 桌面壳的资源目录：Windows 返回 exe 目录（资源在其 `resources/` 子目录下），
    /// macOS 返回 `.app/Contents/Resources/`。解析细节由
    /// `opx_core::utils::paths::resolve_builtin_resource` 处理。
    fn resource_dir(&self) -> Option<std::path::PathBuf> {
        self.app.path().resource_dir().ok()
    }
}
