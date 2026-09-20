//! 一次性实机验证：下载失败时的**同源重试**与**多加速前缀轮换**确实发生。
//!
//! 不进常规测试（用 `--ignored` 显式跑），因为要真实等待退避：
//!
//! ```text
//! cargo test --test live_download_retry -- --ignored --nocapture
//! ```
//!
//! 验证方式：起本地 TCP 监听器，收到连接即立刻断开（模拟「首连抖动」）。
//! 下载侧每失败一次就会重新发起连接 → 用 **accept 计数**确定性地证明尝试次数，
//! 再用耗时下限确认退避生效。这比只读代码可靠。
//!
//! 注：两个测试并行安全——只有第二个会写全局下载配置，第一个的 URL 不含
//! github.com，本就不会叠加任何前缀。

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// 期望与 utils/download.rs 的常量一致
const EXPECTED_ATTEMPTS_PER_MIRROR: usize = 3;
const EXPECTED_MIN_ELAPSED_ONE_MIRROR: Duration = Duration::from_secs(4); // 退避 1s + 3s

fn spawn_counter(listener: TcpListener, counter: Arc<AtomicUsize>) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Ok((sock, _)) = listener.accept().await {
            counter.fetch_add(1, Ordering::SeqCst);
            drop(sock); // 立刻断开，让客户端下载失败
        }
    })
}

fn probe_dest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/opx-retry-probe.zip")
}

#[tokio::test]
#[ignore = "实机：本地起 TCP 监听器，验证失败重试与退避"]
async fn download_retries_and_backs_off_on_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("绑定本地端口失败");
    let port = listener.local_addr().unwrap().port();
    let hits = Arc::new(AtomicUsize::new(0));
    let server = spawn_counter(listener, Arc::clone(&hits));

    // 127.0.0.1 不含 github.com → 不会被叠加加速前缀，请求原样打到这里
    let url = format!("http://127.0.0.1:{port}/probe.zip");
    let started = std::time::Instant::now();
    let result =
        opx_lib::utils::download::download_with_progress(&url, &probe_dest(), |_, _| {}).await;
    let elapsed = started.elapsed();
    let accepts = hits.load(Ordering::SeqCst);

    println!("[single] err = {:?}", result.as_ref().err().map(|e| e.to_string()));
    println!("[single] elapsed = {elapsed:?}, tcp hits = {accepts}");

    assert!(result.is_err(), "连接被断开，应返回错误而不是成功");
    assert_eq!(
        accepts, EXPECTED_ATTEMPTS_PER_MIRROR,
        "同源应尝试 {EXPECTED_ATTEMPTS_PER_MIRROR} 次（实际发起 {accepts} 次连接）"
    );
    assert!(
        elapsed >= EXPECTED_MIN_ELAPSED_ONE_MIRROR,
        "应有退避等待（≥ {EXPECTED_MIN_ELAPSED_ONE_MIRROR:?}），实际 {elapsed:?}"
    );

    server.abort();
}

#[tokio::test]
#[ignore = "实机：本地起两个 TCP 监听器，验证加速前缀按序轮换"]
async fn download_rotates_through_multiple_proxy_prefixes() {
    // 两个本地监听器分别扮演两个「加速前缀」
    let l1 = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let l2 = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();
    let p2 = l2.local_addr().unwrap().port();
    let h1 = Arc::new(AtomicUsize::new(0));
    let h2 = Arc::new(AtomicUsize::new(0));
    let s1 = spawn_counter(l1, Arc::clone(&h1));
    let s2 = spawn_counter(l2, Arc::clone(&h2));

    // 两个前缀，换行分隔（同时验证解析器接受这种写法）
    opx_lib::utils::download::init_download_config(
        format!("http://127.0.0.1:{p1}\nhttp://127.0.0.1:{p2}"),
        String::new(),
    );

    // 含 github.com → 会被依次加上两个前缀
    let url = "https://github.com/pgsty/silo/releases/download/V/probe.zip";
    let result = opx_lib::utils::download::download_with_progress(url, &probe_dest(), |_, _| {}).await;
    let n1 = h1.load(Ordering::SeqCst);
    let n2 = h2.load(Ordering::SeqCst);

    println!("[rotate] err = {:?}", result.as_ref().err().map(|e| e.to_string()));
    println!("[rotate] prefix1 hits = {n1}, prefix2 hits = {n2}");

    assert!(result.is_err(), "两个前缀都不可用，应返回错误");
    assert_eq!(
        n1, EXPECTED_ATTEMPTS_PER_MIRROR,
        "第一个前缀应先被尝试 {EXPECTED_ATTEMPTS_PER_MIRROR} 次（实际 {n1}）"
    );
    assert_eq!(
        n2, EXPECTED_ATTEMPTS_PER_MIRROR,
        "首个前缀耗尽后应轮换到第二个前缀（实际 {n2}）"
    );

    s1.abort();
    s2.abort();
}
