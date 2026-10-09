//! 事件推送抽象（ADR `2026-10-09-opx-web-architecture.md` §3.3）。
//!
//! core 内部**只依赖本 trait**，不感知宿主壳：桌面壳转发 Tauri 事件总线，
//! headless 壳（`opx-server`）则广播给 WebSocket 订阅者。这是「同一后端进程 +
//! 两个 UI 入口」形态能成立的前提。

use serde_json::Value;
use std::path::PathBuf;

/// 事件推送目标（平台无关）。
///
/// 实现方只需保证：事件能送达「本进程内的 UI 订阅者」。桌面壳把它映射到
/// Tauri 的 `emit`，headless 壳映射到 WebSocket 广播。
pub trait EventSink: Send + Sync {
    /// 推送一个事件及其 JSON 负载。
    ///
    /// 事件名与负载必须与前端订阅方约定一致（见各 service 处的既有常量）。
    fn emit(&self, event: &str, payload: Value);

    /// 宿主应用资源目录（用于定位「内置安装包」）。
    ///
    /// 为什么挂在事件 trait 上：这是除事件外的**唯一**宿主上下文依赖（原先是
    /// `AppHandle::path().resource_dir()`），若改为逐层传参会穿透 5 层函数签名；
    /// Rust 又无法把两个 trait 对象合成一个参数。默认 `None` = 无 bundle 资源
    /// （headless 场景本就如此，此时内置包缺失，回退在线下载）。
    fn resource_dir(&self) -> Option<PathBuf> {
        None
    }
}

/// [`EventSink`] 的便捷扩展：允许推送**任意 `Serialize`** 负载。
///
/// ## 为什么需要它
/// Tauri 的 `app.emit(event, payload)` 接受任意 `Serialize`——元组会被序列化成
/// **JSON 数组**、结构体序列化成 **JSON 对象**。而 [`EventSink::emit`] 为保持
/// **对象安全**（必需 `dyn EventSink`）只能限定 `serde_json::Value`
/// ——trait 里若直接写泛型方法 `emit<S: Serialize>`，`dyn EventSink` 立刻失效。
///
/// 本扩展 trait 把 `Serialize → Value` 的转换收在一处，**保证前端收到的 JSON 形状
/// 与迁移前逐字节一致**。迁移旧代码时务必用它，而不是把元组/结构体改写成
/// `json!({...})`——那会把数组变成对象，造成**静默的行为变更**。
pub trait EventSinkExt: EventSink {
    /// 推送任意 `Serialize` 负载；序列化失败时告警并丢弃（不打断业务流程）。
    fn emit_ser<S: serde::Serialize>(&self, event: &str, payload: S) {
        match serde_json::to_value(payload) {
            Ok(v) => self.emit(event, v),
            Err(e) => tracing::warn!(error = %e, event, "事件负载序列化失败，已丢弃"),
        }
    }
}

impl<T: EventSink + ?Sized> EventSinkExt for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// 记录式 sink：验证事件名与负载被原样转发。
    struct Recorder(Arc<Mutex<Vec<(String, Value)>>>);

    impl EventSink for Recorder {
        fn emit(&self, event: &str, payload: Value) {
            self.0.lock().unwrap().push((event.to_string(), payload));
        }
    }

    #[test]
    fn sink_receives_event_name_and_payload() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = Recorder(log.clone());
        sink.emit("install-progress", serde_json::json!({ "percent": 42 }));
        let got = log.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, "install-progress");
        assert_eq!(got[0].1["percent"], 42);
    }

    /// 🚨 **形状回归**：`emit_ser` 必须复刻 Tauri `app.emit` 的 JSON 形状
    /// （元组 → **数组**、结构体 → **对象**），否则前端解析会**静默失败**。
    ///
    /// 这是迁移期最危险的陷阱：把 `app.emit(ev, (a, b, c))` 改写成
    /// `json!({"a":…, "b":…, "c":…})` 能编译通过，但事件结构从数组变对象，
    /// 前端拿到的数据结构完全变了。本测试把三种形态钉死。
    #[test]
    fn emit_ser_preserves_tauri_json_shape() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct Ev {
            status: String,
            pid: u32,
        }

        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = Recorder(log.clone());

        sink.emit_ser("tuple", ("Running", 1234u32));
        sink.emit_ser("struct", Ev { status: "Running".into(), pid: 1234 });
        sink.emit_ser("value", serde_json::json!({ "a": 1 }));

        let got = log.lock().unwrap();
        assert_eq!(got.len(), 3);
        assert_eq!(
            got[0].1,
            serde_json::json!(["Running", 1234]),
            "元组必须序列化为 JSON 数组（Tauri 原行为）"
        );
        assert_eq!(
            got[1].1,
            serde_json::json!({ "status": "Running", "pid": 1234 }),
            "结构体必须序列化为 JSON 对象"
        );
        assert_eq!(got[2].1, serde_json::json!({ "a": 1 }), "Value 应原样透传");
    }
}
