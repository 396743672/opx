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
}
