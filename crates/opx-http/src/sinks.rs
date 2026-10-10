//! 事件 sink 的组合与广播（设计文档 §4 / D4）。
//!
//! - [`FanOutSink`]：把一个事件依序转发给多个 sink —— 双入口（桌面窗口 +
//!   浏览器）同时收到事件的机制；`opx-core::event::EventSink` 的纯组合器，
//!   不含任何 HTTP/WS 依赖，桌面壳/headless 壳/测试通用。
//! - [`WsEventSink`]：把事件广播给 WebSocket 订阅者 —— headless/浏览器侧
//!   的事件出口；per-client bounded channel 满则丢弃（R3 背压对策）。

use opx_core::event::EventSink;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// 组合广播：emit 依序转发给每个内部 sink。
///
/// 装配约定（设计 §8）：
/// - 桌面 + web 开启：`FanOutSink([TauriEventSink, WsEventSink])` → 双入口同收
/// - 桌面 + web 关闭：仅 `TauriEventSink`（与 web 开关前逐字节一致，零回归面）
/// - headless：仅 `WsEventSink`
#[derive(Clone, Default)]
pub struct FanOutSink {
    pub sinks: Vec<Arc<dyn EventSink>>,
}

impl FanOutSink {
    pub fn new(sinks: Vec<Arc<dyn EventSink>>) -> Self {
        Self { sinks }
    }
}

impl EventSink for FanOutSink {
    fn emit(&self, event: &str, payload: Value) {
        for sink in &self.sinks {
            // payload 必须逐份 clone：各 sink 侧是独立的序列化/投递路径
            sink.emit(event, payload.clone());
        }
    }

    fn resource_dir(&self) -> Option<PathBuf> {
        // 宿主资源目录以第一个非 None 的实现为准（实际装配中只有 TauriEventSink
        // 会给出 Some，headless 纯 WS 组合自然返回 None —— 语义与单 sink 一致）。
        self.sinks.iter().find_map(|s| s.resource_dir())
    }
}

/// WS 事件广播 sink。
///
/// 内部维护「订阅者注册表」：每个 WS 连接在握手成功后调用 [`WsEventSink::subscribe`]
/// 取得一个接收端（bounded channel），emit 时向所有订阅者广播。本类型只管广播，
/// WS upgrade / 帧转发由 axum 路由侧（4.3 批次接入）消费接收端完成。
///
/// 背压对策（R3）：per-client channel 容量 64，**满则丢弃本条事件并记 warn**，
/// 绝不阻塞 emit（事件是通知不是数据；前端重连后有全量刷新兜底）。这也是
/// emit 必须用 `try_send` 而非 `send().await` 的原因——`EventSink::emit` 是
/// 同步签名（对象安全约束），且单个慢客户端不得拖垮全部入口。
pub struct WsEventSink {
    clients: Mutex<HashMap<u64, mpsc::Sender<String>>>,
    next_id: AtomicU64,
}

/// 单订阅者 channel 容量（R3：满即丢弃）。
const CLIENT_CHANNEL_CAPACITY: usize = 64;

impl Default for WsEventSink {
    fn default() -> Self {
        Self::new()
    }
}

impl WsEventSink {
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// 以 `Arc` 形态构造（`EventSink` 以 `dyn` 使用，装配点统一 `Arc`）。
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// 注册一个订阅者，返回 `(id, 接收端)`。WS 路由侧 / 测试持有接收端消费
    /// 事件；连接关闭时必须调用 [`WsEventSink::unsubscribe`]，否则注册表泄漏。
    pub fn subscribe(&self) -> (u64, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(CLIENT_CHANNEL_CAPACITY);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.clients
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, tx);
        (id, rx)
    }

    /// 注销订阅者（WS 连接关闭时调用）。
    pub fn unsubscribe(&self, id: u64) {
        self.clients
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
    }

    /// 当前订阅者数量（监控/测试用）。
    pub fn client_count(&self) -> usize {
        self.clients
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }
}

impl EventSink for WsEventSink {
    fn emit(&self, event: &str, payload: Value) {
        // 消息信封：`{"event": ..., "payload": ...}` —— 前端 transport 层按
        // event 名多路分发到各 listen 回调（设计 §5 D5 的抹平约定）。
        let msg = serde_json::json!({ "event": event, "payload": payload }).to_string();
        let clients = self.clients.lock().unwrap_or_else(|e| e.into_inner());
        for (id, tx) in clients.iter() {
            // try_send：不等待、不阻塞。Full = 该客户端消费过慢，丢弃本条。
            if let Err(e) = tx.try_send(msg.clone()) {
                use tokio::sync::mpsc::error::TrySendError::*;
                match e {
                    Full(_) => {
                        tracing::warn!(client = id, event, "ws client 慢，事件丢弃（有全量刷新兜底）");
                    }
                    Closed(_) => {
                        // 连接已关但还没走 unsubscribe：直接清理，避免注册表积累
                        tracing::debug!(client = id, "ws client 已关闭，移除订阅");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opx_core::event::EventSinkExt;

    /// 简单记录型 sink：验证 FanOut 的转发顺序与逐份投递。
    #[derive(Default)]
    struct RecordingSink {
        seen: Mutex<Vec<String>>,
    }
    impl EventSink for RecordingSink {
        fn emit(&self, event: &str, _payload: Value) {
            self.seen
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(event.to_string());
        }
    }

    #[test]
    fn fanout_forwards_to_all_in_order() {
        let a = Arc::new(RecordingSink::default());
        let b = Arc::new(RecordingSink::default());
        let fan = FanOutSink::new(vec![a.clone(), b.clone()]);

        fan.emit("evt-a", serde_json::json!({ "k": 1 }));
        fan.emit_ser("evt-b", ("x", 2)); // EventSinkExt 也要能走 FanOut

        let seen_a = a.seen.lock().unwrap();
        let seen_b = b.seen.lock().unwrap();
        assert_eq!(*seen_a, vec!["evt-a".to_string(), "evt-b".to_string()]);
        assert_eq!(*seen_b, *seen_a, "每个 sink 收到相同的依序转发");
    }

    #[test]
    fn fanout_resource_dir_falls_through_to_first_some() {
        // 两个 None + 一个 Some：resource_dir 取第一个 Some（装配语义）
        struct NoneSink;
        impl EventSink for NoneSink {
            fn emit(&self, _: &str, _: Value) {}
        }
        struct DirSink;
        impl EventSink for DirSink {
            fn emit(&self, _: &str, _: Value) {}
            fn resource_dir(&self) -> Option<PathBuf> {
                Some(PathBuf::from("/res"))
            }
        }
        let fan = FanOutSink::new(vec![
            Arc::new(NoneSink),
            Arc::new(DirSink),
            Arc::new(NoneSink),
        ]);
        assert_eq!(fan.resource_dir(), Some(PathBuf::from("/res")));

        let none_fan: FanOutSink =
            FanOutSink::new(vec![Arc::new(NoneSink), Arc::new(NoneSink)]);
        assert_eq!(none_fan.resource_dir(), None, "全 None（headless）语义保持");
    }

    #[tokio::test]
    async fn ws_sink_broadcasts_to_subscribers() {
        let sink = WsEventSink::shared();
        let (id, mut rx) = sink.subscribe();
        assert_eq!(sink.client_count(), 1);

        sink.emit("software-status-changed", serde_json::json!({ "id": "x" }));

        let msg = rx.recv().await.expect("订阅者应收到消息");
        let v: Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(v["event"], "software-status-changed");
        assert_eq!(v["payload"]["id"], "x");

        sink.unsubscribe(id);
        assert_eq!(sink.client_count(), 0);
    }

    #[tokio::test]
    async fn ws_sink_drops_on_slow_client_without_blocking() {
        let sink = WsEventSink::shared();
        let (_id, mut rx) = sink.subscribe();

        // 先正常投递一条（订阅者未消费，占用 1 格）
        sink.emit("e0", serde_json::json!(0));
        // 灌满剩余容量并再溢出若干条：emit 必须不阻塞、不 panic（R3）
        for i in 0..(CLIENT_CHANNEL_CAPACITY + 10) {
            sink.emit("e1", serde_json::json!(i));
        }
        // 消费端最终能收到全部未丢弃消息的前缀，且 emit 调用全程无阻塞
        let mut got = 0;
        while rx.try_recv().is_ok() {
            got += 1;
        }
        assert_eq!(
            got,
            CLIENT_CHANNEL_CAPACITY,
            "容量 64：64 条保留，溢出的丢弃"
        );
    }
}
