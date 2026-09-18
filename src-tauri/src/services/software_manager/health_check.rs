use std::time::Duration;

use crate::models::software::HealthCheckSpec;

#[derive(Debug, Clone, PartialEq)]
pub enum HealthCheckResult {
    Healthy,
    Timeout,
    ProcessExited,
}

/// 执行健康检查调度
/// - ProcessOnly: 直接返回 Healthy（进程存活由调用方先检查）
/// - Tcp: 每 interval_ms 尝试连接，max_attempts 次
/// - Http: GET url，期望 expected_status（不跟随重定向）
///
/// # PID 检查职责约定
/// 本函数接受 `pid_alive: bool` 一次性参数，仅在循环外检查一次。
/// 调用方（lifecycle 层）应额外 spawn 一个 `child.wait()` 监听器，
/// 发现进程退出后通过 `CancellationToken` 取消本 task，
/// 否则进程在 30s 轮询期间崩溃会被误报为 Timeout 而非 ProcessExited。
pub async fn run_health_check(
    spec: &HealthCheckSpec,
    pid_alive: bool,
    max_attempts: u32,
    interval_ms: u64,
) -> HealthCheckResult {
    if !pid_alive {
        return HealthCheckResult::ProcessExited;
    }

    match spec {
        HealthCheckSpec::ProcessOnly => HealthCheckResult::Healthy,
        HealthCheckSpec::Tcp { port, timeout_ms } => {
            for _ in 0..max_attempts {
                if tcp_probe("127.0.0.1", *port, Duration::from_millis(*timeout_ms)).await {
                    return HealthCheckResult::Healthy;
                }
                tokio::time::sleep(Duration::from_millis(interval_ms)).await;
            }
            HealthCheckResult::Timeout
        }
        HealthCheckSpec::Http {
            url,
            expected_status,
            timeout_ms,
        } => {
            for _ in 0..max_attempts {
                if http_probe(url, *expected_status, Duration::from_millis(*timeout_ms)).await {
                    return HealthCheckResult::Healthy;
                }
                tokio::time::sleep(Duration::from_millis(interval_ms)).await;
            }
            HealthCheckResult::Timeout
        }
    }
}

pub async fn tcp_probe(host: &str, port: u16, timeout: Duration) -> bool {
    let addr = format!("{}:{}", host, port);
    let connect_fut = tokio::net::TcpStream::connect(addr);
    match tokio::time::timeout(timeout, connect_fut).await {
        Ok(Ok(_)) => true,
        _ => false,
    }
}

pub async fn http_probe(url: &str, expected_status: u16, timeout: Duration) -> bool {
    // 健康检查语义上不应跟随重定向（如 Nginx 默认 301 → /index.html）
    let client = match reqwest::Client::builder()
        // 探的是 127.0.0.1 本机端口，环境代理（ALL_PROXY 等）必然劫持并失败，
        // 这里显式关掉环境变量探测；无需支持用户代理，故不引 utils::http
        .no_proxy()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    match client.get(url).send().await {
        Ok(resp) => resp.status().as_u16() == expected_status,
        Err(_) => false,
    }
}

/// 检测本机端口是否空闲：能 bind 127.0.0.1:port 即视为空闲，bind 失败即被占用。
/// ponytail: 只探 127.0.0.1，绑 0.0.0.0 的软件极少见；需要时再扩。
pub fn is_port_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

pub fn is_process_alive(pid: u32) -> bool {
    let mut sys = sysinfo::System::new();
    // 单 PID 刷新，比 refresh_processes(All) 快 10-50 倍
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]));
    sys.process(sysinfo::Pid::from_u32(pid)).is_some()
}
