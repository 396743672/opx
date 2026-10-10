//! Web 服务器生命周期管理（批次 4.5，设计 D1 生命周期 / D6 端口策略）。
//!
//! ## 装配约束（team-lead 钦定）：启停 server 不得改变已注入 sink 的对象身份
//!
//! 6 个后台任务（bootstrap/watchdog/recorder/backup_scheduler/renew_scheduler/
//! ddns）持 `AppContext.sink` 引用。本模块**只增删 axum 任务**，永不重建 sink：
//! - `WsEventSink` 常驻（setup 时 manage 一次），无客户端/未启 server 时
//!   `emit` 为 no-op（空订阅者表）——开关切换零重建；
//! - 桌面模式 sink = `FanOut[Tauri, Ws]` 自进程启动即固定（批次 4.5 起，
//!   替换 4.1 的 `FanOut[Tauri]`，桌面行为不变：Tauri 分支逐字节透传）。
//!
//! ## 生命周期（D1 表 + D6）
//!
//! - 触发：2s 轮询 supervisor 对比「已应用 (enabled, port, lan, token)」与
//!   settings.json / token 文件现状——变更即 abort 旧任务 + 重启（热生效）。
//!   轮询而非事件钩子的原因：`save_settings` 经 Tauri IPC 与 HTTP 分发器
//!   两条路都能改设置，文件侧轮询统一覆盖两条路且零命令签名改动。
//! - 端口冲突（D6，关闭 ADR 开放项 #2）：`bind` 失败 → 桌面 emit
//!   `web-server-error` 事件（设置页横幅显示），headless 打印控制台；
//!   **不自动重试、不自动换端口**，由用户改设置。
//! - LAN 开关：`web_lan_access=true` 绑 `0.0.0.0`（风险提示见设置页）。
//! - headless：无条件启动（它是唯一入口，`web_enabled=false` 无意义）。

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};

use opx_http::{token, AppContext, AppState, TicketStore, WsEventSink};

use crate::commands::http_registry::build_registry;

/// 已应用的 server 配置（比对基准）。
#[derive(Clone, PartialEq, Eq)]
struct Applied {
    enabled: bool,
    port: u16,
    lan: bool,
    token: String,
}

/// 受管 server 任务句柄 + 应用状态（setup 时 manage，supervisor 与命令共用）。
pub struct WebServerState {
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    applied: Mutex<Option<Applied>>,
    /// headless 模式：enabled 强制为 true（server 是唯一入口）
    headless: bool,
}

impl WebServerState {
    pub fn new(headless: bool) -> Self {
        Self {
            task: Mutex::new(None),
            applied: Mutex::new(None),
            headless,
        }
    }
}

/// 出口 IP 探测（批次 4.6，零依赖）：`UdpSocket connect 8.8.8.8` 不发包、
/// 只让内核选默认路由，`local_addr` 即多网卡场景的默认出口 IP。
/// 不做全网卡枚举（需要额外依赖，用户裁定先按零依赖做）。
pub fn outbound_ip() -> Option<String> {
    use std::net::UdpSocket;
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    Some(s.local_addr().ok()?.ip().to_string())
}

/// headless 启动横幅（纯函数便于单测钉格式）：完整 URL（含 `#token=`，
/// fragment 不随请求上送、不入访问日志——D3.1 A+B 组合）+ 可选 LAN URL
///（批次 4.6，移动端扫码/手输用）+ settings 路径提示 + Ctrl-C 退出说明。
pub fn banner_lines(
    host: &str,
    port: u16,
    token: &str,
    settings_path: &PathBuf,
    lan_ip: Option<&str>,
) -> Vec<String> {
    let mut lines = vec![
        "==============================================".to_string(),
        "  OPX Web 管理入口已启动".to_string(),
        format!("  地址: http://{host}:{port}/#/?token={token}"),
    ];
    if let Some(ip) = lan_ip {
        lines.push(format!("  局域网: http://{ip}:{port}/#/?token={token}"));
    }
    lines.push(
        "  （点击上方链接即自动登录；token 亦可在 settings.json 查看/重置）".to_string(),
    );
    lines.push(format!("  配置文件: {}", settings_path.display()));
    lines.push("  按 Ctrl-C 停止服务（将级联停止运行中的软件）".to_string());
    lines.push("==============================================".to_string());
    lines
}

/// 单次对齐：读 settings + token，与已应用状态比对，需要时启停/重启 server。
///
/// 返回 `Ok(())` 表示已对齐（含无需变更）；`Err` 仅在 token 读盘失败等
/// 致命场景（此时不动机上已有 server）。
pub fn sync(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<WebServerState>();
    let headless = state.headless;

    let settings = crate::commands::config::read_settings()
        .map_err(|e| format!("读取设置失败: {e}"))?;
    let token = token::load_or_generate().map_err(|e| format!("读取/生成 token 失败: {e}"))?;

    // headless：enabled 强制 true（设计 §3 web_enabled 表：headless 默认 true）
    let want = Applied {
        enabled: headless || settings.web_enabled,
        port: settings.web_port,
        lan: settings.web_lan_access,
        token: token.clone(),
    };

    let unchanged = state.applied.lock().unwrap_or_else(|e| e.into_inner()).as_ref() == Some(&want);
    if unchanged {
        return Ok(());
    }

    // 先停旧任务（端口/token 变更都要重启）
    abort_current(app);

    if !want.enabled {
        *state.applied.lock().unwrap_or_else(|e| e.into_inner()) = Some(want);
        tracing::info!("web server 已停止");
        return Ok(());
    }

    let bind_addr: std::net::SocketAddr = if want.lan {
        ([0, 0, 0, 0], want.port).into()
    } else {
        ([127, 0, 0, 1], want.port).into()
    };

    // AppState 组装：ctx/registry/ws 全部取自 manage 的既有实例（身份不变约束）；
    // tickets 每次重启新建（60s 单次票据，生命周期短于 server，无跨重启价值）
    let ctx = app.state::<std::sync::Arc<AppContext>>().inner().clone();
    let ws = app
        .state::<std::sync::Arc<WsEventSink>>()
        .inner()
        .clone();
    let app_state = AppState::new(
        token.clone(),
        ctx,
        build_registry(app),
        ws,
        std::sync::Arc::new(TicketStore::new()),
    );

    let handle = app.clone();
    let headless_flag = headless;
    let task = tauri::async_runtime::spawn(async move {
        match opx_http::bind(bind_addr, app_state).await {
            Err(e) => {
                // D6：明确报错，不自动重试/换端口。applied 保留为本次目标——
                // supervisor 不再重试；用户改设置（端口/开关）后自然触发重启。
                let msg = format!("绑定 {} 失败: {e}（端口被占用？）", bind_addr);
                tracing::error!("{msg}");
                if headless_flag {
                    eprintln!("[web] {msg}");
                } else {
                    let _ = handle.emit("web-server-error", msg.clone());
                }
                let st = handle.state::<WebServerState>();
                let want_now = Applied {
                    enabled: true,
                    port: bind_addr.port(),
                    lan: bind_addr.ip().is_unspecified(),
                    token: token.clone(),
                };
                *st.applied.lock().unwrap_or_else(|e| e.into_inner()) = Some(want_now);
            }
            Ok((local, serve_fut)) => {
                let settings_path = opx_core::utils::paths::settings_path();
                // 0.0.0.0 绑定下 local_addr 是 0.0.0.0，URL 展示用 127.0.0.1，
                // 并经默认路由探测出口 IP 补一行 LAN URL（批次 4.6）
                let (host, lan_ip) = if local.ip().is_unspecified() {
                    ("127.0.0.1".to_string(), outbound_ip())
                } else {
                    (local.ip().to_string(), None)
                };
                for line in banner_lines(
                    &host,
                    local.port(),
                    &token,
                    &settings_path,
                    lan_ip.as_deref(),
                ) {
                    if headless_flag {
                        println!("{line}");
                    } else {
                        tracing::info!("{line}");
                    }
                }
                if let Err(e) = serve_fut.await {
                    let msg = format!("web server 异常退出: {e}");
                    tracing::error!("{msg}");
                    if headless_flag {
                        eprintln!("[web] {msg}");
                    } else {
                        let _ = handle.emit("web-server-error", msg);
                    }
                    let st = handle.state::<WebServerState>();
                    *st.applied.lock().unwrap_or_else(|e| e.into_inner()) = None;
                }
            }
        }
    });

    *state.task.lock().unwrap_or_else(|e| e.into_inner()) = Some(task);
    *state.applied.lock().unwrap_or_else(|e| e.into_inner()) = Some(want);
    Ok(())
}

/// abort 当前 server 任务（若在跑）。
fn abort_current(app: &AppHandle) {
    let state = app.state::<WebServerState>();
    // MutexGuard 先落绑定再取值：guard 的 Drop 不能晚于 state（E0597）
    let taken = {
        let mut guard = state.task.lock().unwrap_or_else(|e| e.into_inner());
        guard.take()
    };
    drop(state);
    if let Some(task) = taken {
        task.abort();
        tracing::info!("web server 任务已终止（配置变更/重启）");
    }
}

/// 2s 轮询 supervisor：覆盖 Tauri IPC 与 HTTP 两条设置写入路径 +
/// token 文件外部变更（如手工重置）。首跑立即对齐（不等首个 2s）。
pub fn spawn_supervisor(app: &AppHandle) {
    if let Err(e) = sync(app) {
        tracing::error!("web server 初始对齐失败: {e}");
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            if let Err(e) = sync(&handle) {
                tracing::warn!("web server 对齐失败: {e}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn banner_contains_token_url_and_hints() {
        let lines = banner_lines(
            "127.0.0.1",
            17580,
            "abc123",
            &PathBuf::from("/tmp/settings.json"),
            None,
        );
        let joined = lines.join("\n");
        assert!(joined.contains("http://127.0.0.1:17580/#/?token=abc123"));
        assert!(joined.contains("settings.json"));
        assert!(joined.contains("Ctrl-C"));
        assert!(!joined.contains("局域网"), "无出口 IP 时不打 LAN 行");
    }

    /// 批次 4.6：有出口 IP 时补 LAN URL 行（移动端扫码/手输）。
    #[test]
    fn banner_includes_lan_url_when_available() {
        let lines = banner_lines(
            "127.0.0.1",
            17580,
            "abc123",
            &PathBuf::from("/tmp/settings.json"),
            Some("192.168.1.7"),
        );
        let joined = lines.join("\n");
        assert!(joined.contains("http://192.168.1.7:17580/#/?token=abc123"));
    }
}
