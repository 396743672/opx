//! 日志文件监听（notify）：监听已注册日志文件的变化事件，实时推送前端 `log-file-changed`。
//!
//! ponytail: 使用全局单例 + 后台线程。前端在查看某日志源时注册路径，收到事件即增量读取；
//! 关闭/切源时注销。轮询仍保留为兜底，监听仅为「即时触发」。
//!
//! ## ⚠️ 死代码（阶段 3批次 3A1 确认，保留待清理）
//!
//! 本模块**全仓库零调用方**——`grep log_watcher` 只命中 `software_manager/mod.rs`
//! 里的 `pub mod` 声明，没有任何实际调用点。
//!
//! 因此阶段 3 搬迁时**刻意不搬本文件**：搬它需要给 `opx-core` 新增 `notify` 依赖
//! 却零收益（无调用方）。故保留在壳层，与同模块下`lifecycle.rs` 的
//! `auto_start_all` 三段同属「死代码保留待清理」一类。
//!
//! 后续清理轮次可考虑：要么接上前端真正用它，要么整体删除。

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};
use opx_core::event::EventSink;

enum Cmd {
    Register { path: String, id: u64 },
    Unregister { id: u64 },
}

pub struct LogWatcher {
    tx: Mutex<Option<Sender<Cmd>>>,
    next_id: Mutex<u64>,
}

fn watcher() -> &'static LogWatcher {
    static W: OnceLock<LogWatcher> = OnceLock::new();
    W.get_or_init(LogWatcher::new)
}

impl LogWatcher {
    fn new() -> Self {
        Self {
            tx: Mutex::new(None),
            next_id: Mutex::new(1),
        }
    }

    /// 应用启动时初始化后台监听线程（幂等）
    pub fn init(sink: std::sync::Arc<dyn EventSink>) {
        let w = watcher();
        let mut guard = w.tx.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        *guard = Some(tx);
        std::thread::spawn(move || run_loop(sink, rx));
    }

    pub fn register(path: &str) -> Option<u64> {
        let w = watcher();
        let id = {
            let mut n = w.next_id.lock().unwrap_or_else(|e| e.into_inner());
            let id = *n;
            *n += 1;
            id
        };
        let tx = w.tx.lock().unwrap_or_else(|e| e.into_inner()).as_ref()?.clone();
        tx.send(Cmd::Register {
            path: path.to_string(),
            id,
        })
        .ok()?;
        Some(id)
    }

    pub fn unregister(id: u64) {
        let w = watcher();
        if let Some(tx) = w.tx.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            let _ = tx.send(Cmd::Unregister { id });
        }
    }
}

fn run_loop(sink: std::sync::Arc<dyn EventSink>, rx: Receiver<Cmd>) {
    let (ev_tx, ev_rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let Ok(mut watcher) = notify::recommended_watcher(move |res| {
        let _ = ev_tx.send(res);
    }) else {
        return;
    };
    let mut registered: HashMap<String, u64> = HashMap::new(); // path → id
    let mut last_emit: HashMap<String, Instant> = HashMap::new(); // path → 上次推送时间（节流）
    const THROTTLE: Duration = Duration::from_millis(300);

    loop {
        // 处理控制命令（注册/注销）
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                Cmd::Register { path, id } => {
                    if !registered.contains_key(&path) {
                        if let Some(dir) = std::path::Path::new(&path).parent() {
                            let _ = watcher.watch(dir, RecursiveMode::NonRecursive);
                        }
                    }
                    registered.insert(path.clone(), id);
                }
                Cmd::Unregister { id } => {
                    registered.retain(|_, v| *v != id);
                }
            }
        }

        // 事件：命中已注册文件 → 节流后推送前端
        match ev_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Ok(ev)) => {
                let relevant = ev.kind.is_modify() || ev.kind.is_create();
                if !relevant {
                    continue;
                }
                for p in ev.paths {
                    let pstr = p.to_string_lossy().to_string();
                    if let Some(id) = registered.get(&pstr).copied() {
                        let due = last_emit
                            .get(&pstr)
                            .map(|t| t.elapsed() >= THROTTLE)
                            .unwrap_or(true);
                        if due {
                            last_emit.insert(pstr.clone(), Instant::now());
                            let payload = serde_json::json!({ "id": id, "path": pstr });
                            sink.emit("log-file-changed", payload);
                        }
                    }
                }
            }
            Ok(Err(_)) | Err(_) => {}
        }
    }
}