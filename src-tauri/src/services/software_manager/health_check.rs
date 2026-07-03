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

pub fn is_process_alive(pid: u32) -> bool {
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All);
    sys.process(sysinfo::Pid::from_u32(pid)).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_only_returns_healthy_when_pid_alive() {
        let spec = HealthCheckSpec::ProcessOnly;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async {
            run_health_check(&spec, true, 1, 100).await
        });
        assert_eq!(result, HealthCheckResult::Healthy);
    }

    #[test]
    fn process_only_returns_process_exited_when_pid_not_alive() {
        let spec = HealthCheckSpec::ProcessOnly;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async {
            run_health_check(&spec, false, 1, 100).await
        });
        assert_eq!(result, HealthCheckResult::ProcessExited);
    }

    #[test]
    fn tcp_probe_unreachable_port_returns_false() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async {
            tcp_probe("127.0.0.1", 59999, Duration::from_millis(200)).await
        });
        assert!(!result);
    }

    #[test]
    fn http_probe_invalid_url_returns_false() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async {
            http_probe("http://127.0.0.1:59999/", 200, Duration::from_millis(200)).await
        });
        assert!(!result);
    }

    #[test]
    fn is_process_alive_returns_true_for_self() {
        let pid = std::process::id();
        assert!(is_process_alive(pid));
    }

    #[test]
    fn is_process_alive_returns_false_for_invalid_pid() {
        assert!(!is_process_alive(99999999));
    }

    #[test]
    fn tcp_probe_times_out_on_unreachable_port() {
        // 验证 tcp_probe 在超时内返回（不挂起）
        let rt = tokio::runtime::Runtime::new().unwrap();
        let start = std::time::Instant::now();
        let _ = rt.block_on(async {
            tcp_probe("127.0.0.1", 59998, Duration::from_millis(100)).await
        });
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_millis(500),
            "tcp_probe 应在超时内返回，实际 {:?}",
            elapsed
        );
    }
}
