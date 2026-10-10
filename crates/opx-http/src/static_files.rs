//! 静态 SPA 托管（批次 4.5，设计 §5「`/` 服务 Vue SPA 构建产物」）。
//!
//! - dist 目录解析：`OPX_WEB_DIST` 环境变量 → exe 同级 `dist/`（打包形态
//!   adjacent，随 NSIS/MSI 安装目录分发）→ 当前工作目录 `dist/`（开发态从
//!   仓库根启动）。三处都以 `index.html` 存在为判据，找不到则不下挂静态
//!   路由（纯 API 模式，headless 不带前端产物也能跑）。
//! - SPA fallback：未匹配路径回落 `index.html`（vue-router history 模式）。
//!   `ServeDir` 内建路径穿越防护（`..` 归一化后限制在根目录内）。
//! - dist **不进 git**（根 .gitignore 已有 `/dist`）；打包形态取 adjacent
//!   方案（tauri resource 打包会引入 `resource_dir()` 宿主差异——桌面壳有、
//!   headless 无，adjacent 对两形态一致且零额外打包配置，最简可行）。

use std::path::PathBuf;

use tower_http::services::{ServeDir, ServeFile};

/// 依序探测 dist 目录；返回第一个含 `index.html` 的候选。
pub fn resolve_dist_dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(env_dir) = std::env::var("OPX_WEB_DIST") {
        if !env_dir.trim().is_empty() {
            candidates.push(PathBuf::from(env_dir));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("dist"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("dist"));
    }
    candidates.into_iter().find(|d| d.join("index.html").is_file())
}

/// 构造 SPA fallback 服务（挂到主 Router 的 fallback 上）。
///
/// `ServeDir` 处理真实文件（mime_guess 定 Content-Type）；
/// 找不到的路径回落 `index.html`——前端路由（/settings 等深链接）直接刷新可用。
pub fn spa_service(dist: &PathBuf) -> ServeDir<ServeFile> {
    ServeDir::new(dist).fallback(ServeFile::new(dist.join("index.html")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Registry;
    use crate::{AppContext, AppState, TicketStore, WsEventSink};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::sync::Arc;
    use tower::ServiceExt;

    struct NoopSink;
    impl opx_core::event::EventSink for NoopSink {
        fn emit(&self, _: &str, _: serde_json::Value) {}
    }

    fn test_state() -> AppState {
        let software =
            Arc::new(opx_core::services::software_manager::SoftwareManager::new());
        let springboot =
            Arc::new(opx_core::services::springboot_manager::SpringBootManager::new());
        let ctx = Arc::new(AppContext {
            software: software.clone(),
            website: Arc::new(opx_core::services::website_manager::WebsiteManager::new()),
            springboot: springboot.clone(),
            node: Arc::new(opx_core::services::node_app_manager::NodeAppManager::new()),
            dns: Arc::new(opx_core::services::dns_account::DnsAccountManager::new()),
            stack: Arc::new(opx_core::services::stack_manager::StackManager::new(
                software, springboot,
            )),
            sink: Arc::new(NoopSink),
            node_exe: None,
        });
        AppState::new(
            crate::token::generate(),
            ctx,
            Registry::new(),
            WsEventSink::shared(),
            Arc::new(TicketStore::new()),
        )
    }

    /// 临时目录里铺一个最小 dist（index.html + main.js），测 Content-Type 与
    /// SPA fallback；用完即删。
    fn make_tmp_dist(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opx-http-static-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("index.html"), "<!DOCTYPE html><html>opx</html>").unwrap();
        std::fs::write(dir.join("assets").join("main.js"), "console.log(1)").unwrap();
        dir
    }

    #[tokio::test]
    async fn static_index_served_with_html_mime() {
        let dist = make_tmp_dist("index");
        let app = crate::router_with_dist(test_state(), Some(dist.clone()));

        let res = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert!(
            res.headers()["content-type"]
                .to_str()
                .unwrap()
                .starts_with("text/html"),
            "index.html 应以 text/html 提供 service"
        );
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("opx"));
        let _ = std::fs::remove_dir_all(&dist);
    }

    #[tokio::test]
    async fn static_js_mime_and_spa_fallback() {
        let dist = make_tmp_dist("fallback");
        let app = crate::router_with_dist(test_state(), Some(dist.clone()));

        // 真实文件 → application/javascript（mime_guess 语义钉死）
        let res = app
            .clone()
            .oneshot(
                Request::get("/assets/main.js").body(Body::empty()).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        // mime_guess 2.x 对 .js 返回 text/javascript（RFC 9239）；旧称
        // application/javascript 亦接受——两种都是浏览器可执行的合法 MIME
        let ct = res.headers()["content-type"].to_str().unwrap();
        assert!(
            ct.starts_with("text/javascript") || ct.starts_with("application/javascript"),
            "js 文件应以 javascript mime 提供，实际: {ct}"
        );

        // 深链接不存在 → SPA fallback 回 index.html（200，非 404）
        let res = app
            .oneshot(
                Request::get("/settings/deep-link")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "SPA 深链接应回落 index.html");
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("opx"));
        let _ = std::fs::remove_dir_all(&dist);
    }

    #[tokio::test]
    async fn no_dist_keeps_api_only_fallback() {
        // 不下挂静态服务时，未知路径仍是 404（headless 无前端产物形态）
        let app = crate::router_with_dist(test_state(), None);
        let res = app
            .oneshot(
                Request::get("/nothing-here").body(Body::empty()).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
