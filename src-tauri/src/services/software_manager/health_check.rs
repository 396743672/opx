use std::time::Duration;

use crate::models::software::HealthCheckSpec;

#[derive(Debug, Clone, PartialEq)]
pub enum HealthCheckResult {
    Healthy,
    Timeout,
    ProcessExited,
}

/// 执行健康检查调度
/// - ProcessOnly: 进程存活即 Healthy（每轮复核 pid，中途退出返回 ProcessExited）
/// - Tcp: 每 interval_ms 尝试连接 127.0.0.1:port，max_attempts 次
/// - Http: GET url，期望 expected_status（不跟随重定向）
///
/// # PID 中途退出检测
/// 入参 `pid: Option<u32>`：若提供，在每轮探测前复核进程是否存活。
/// 进程在轮询期间崩溃会被正确判为 `ProcessExited` 而非 `Timeout`，
/// 解决了旧实现「循环外仅查一次 pid、中途崩溃被误报为超时」的边界问题。
pub async fn run_health_check(
    spec: &HealthCheckSpec,
    pid: Option<u32>,
    max_attempts: u32,
    interval_ms: u64,
) -> HealthCheckResult {
    // 每轮探测前复核进程存活：中途退出立即返回 ProcessExited（而非耗尽轮询判 Timeout）
    let pid_dead = || match pid {
        Some(p) => !is_process_alive(p),
        None => false,
    };

    match spec {
        HealthCheckSpec::ProcessOnly => {
            if pid_dead() {
                return HealthCheckResult::ProcessExited;
            }
            HealthCheckResult::Healthy
        }
        HealthCheckSpec::Tcp { port, timeout_ms } => {
            for _ in 0..max_attempts {
                if pid_dead() {
                    return HealthCheckResult::ProcessExited;
                }
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
                if pid_dead() {
                    return HealthCheckResult::ProcessExited;
                }
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
        // 健康检查语义：HTTP 2xx 即视为服务已就绪响应（比精确状态码相等更稳健，
        // 可兼容 200/201/204 等；仍优先接受精确 expected_status 命中）。
        Ok(resp) => {
            resp.status().as_u16() == expected_status || resp.status().is_success()
        }
        Err(_) => false,
    }
}

/// 检测本机端口是否空闲：同时尝试 bind 127.0.0.1 与 0.0.0.0，
/// 仅当「任一地址」均未被占用时才视为空闲。
/// 旧实现只探 127.0.0.1，会漏报绑定到外部网卡/0.0.0.0 的冲突（误判空闲），
/// 并导致 probe_ready 对只监听非 lo 地址的软件永远判未就绪（误超时）。
pub fn is_port_free(port: u16) -> bool {
    let lo_free = std::net::TcpListener::bind(("127.0.0.1", port)).is_ok();
    let any_free = std::net::TcpListener::bind(("0.0.0.0", port)).is_ok();
    lo_free && any_free
}

pub fn is_process_alive(pid: u32) -> bool {
    let mut sys = sysinfo::System::new();
    // 单 PID 刷新，比 refresh_processes(All) 快 10-50 倍
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]));
    sys.process(sysinfo::Pid::from_u32(pid)).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn is_port_free_detects_any_interface_listener() {
        // 旧实现只探 127.0.0.1，会漏报绑 0.0.0.0 的监听；新实现应判被占用。
        let listener = std::net::TcpListener::bind(("0.0.0.0", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(!is_port_free(port), "绑 0.0.0.0 的端口应判被占用");
        drop(listener);
        assert!(is_port_free(port), "释放后应空闲");
    }

    #[tokio::test]
    async fn run_health_check_returns_process_exited_when_pid_dies() {
        // 进程中途退出应判 ProcessExited 而非 Timeout（修复 #3 边界）
        let mut child = if cfg!(windows) {
            std::process::Command::new("cmd")
                .args(["/c", "exit"])
                .spawn()
                .unwrap()
        } else {
            std::process::Command::new("true").spawn().unwrap()
        };
        let pid = child.id();
        let _ = child.kill();
        let _ = child.wait();
        // 端口必然不可达，但 pid 已死应优先返回 ProcessExited（不应判 Timeout）
        let spec = HealthCheckSpec::Tcp {
            port: 1,
            timeout_ms: 100,
        };
        let r = run_health_check(&spec, Some(pid), 5, 50).await;
        assert_eq!(r, HealthCheckResult::ProcessExited);
    }

    #[tokio::test]
    async fn http_probe_accepts_2xx_as_healthy() {
        // 用独立 OS 线程起一个最小 HTTP 服务，返回 200（先消费请求、显式关闭，避免 RST）
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            // 循环接受多个连接（测试会发起两次探针）
            for _ in 0..2 {
                if let Ok((mut stream, _)) = listener.accept() {
                    use std::io::{Read, Write};
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf); // 消费客户端请求
                    let _ = stream.write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                    let _ = stream.flush();
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                }
            }
        });
        let url = format!("http://{addr}/health");
        // expected_status 命中
        assert!(http_probe(&url, 200, Duration::from_millis(1000)).await);
        // expected_status 不命中（204）但 2xx 仍判 Healthy（修复 #4）
        assert!(http_probe(&url, 204, Duration::from_millis(1000)).await);
    }
}
