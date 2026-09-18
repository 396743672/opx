//! 共享 HTTP 客户端：在设置里配了全局代理时套用它。
//!
//! 各 DNS 服务商原先是 `reqwest::Client::new()`——而 reqwest **默认**会读
//! `ALL_PROXY`/`HTTPS_PROXY` 环境变量。这会让「设置里没配代理」的应用仍然
//! 被环境里的代理接管：同一个 HTTP 代理不支持 CONNECT 到 443，落地成
//! 「tunnel error / tcp connect error 10061（目标计算机积极拒绝）」，
//! 而直连其实是通的。`no_proxy()` 关掉环境变量探测，只认应用内设置。

use std::sync::Mutex;

/// 由设置页写入的全局代理（空 = 直连）。启动时与保存设置时刷新。
static GLOBAL_PROXY: Mutex<String> = Mutex::new(String::new());

pub fn set_global_proxy(url: &str) {
    if let Ok(mut g) = GLOBAL_PROXY.lock() {
        *g = url.trim().to_string();
    }
}

/// 给 DNS 服务商等出站请求用的客户端。
///
/// 目前每次调用新建一个，未复用连接池；DNS 操作是低频人工触发，
/// 够用。若将来要复用，把这里换成 `OnceLock<reqwest::Client>` 并让
/// `set_global_proxy` 负责重建。
pub fn client() -> reqwest::Client {
    let proxy = GLOBAL_PROXY.lock().map(|g| g.clone()).unwrap_or_default();
    let mut b = reqwest::Client::builder().no_proxy();
    if !proxy.is_empty() {
        match reqwest::Proxy::all(&proxy) {
            Ok(p) => b = b.proxy(p),
            // 代理串非法只影响可用性，不该让整个客户端构造失败
            Err(e) => tracing::warn!(proxy = %proxy, error = %e, "全局代理地址无效，本次请求改为直连"),
        }
    }
    b.build().unwrap_or_else(|_| reqwest::Client::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 设置了代理也不该 panic（reqwest 的 Proxy 校验在构造期）
    #[test]
    fn builds_with_and_without_proxy() {
        set_global_proxy("");
        assert!(client().get("https://example.com").build().is_ok());
        set_global_proxy("http://127.0.0.1:1");
        assert!(client().get("https://example.com").build().is_ok());
        set_global_proxy("");
    }
}