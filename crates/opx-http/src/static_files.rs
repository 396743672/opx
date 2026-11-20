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

use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use include_dir::{include_dir, Dir};
use tower_http::services::{ServeDir, ServeFile};

/// 编译期内嵌的前端产物（仓库根 dist/，路径相对本 crate 的 Cargo.toml）。
///
/// 这是 Web 管理入口自包含的核心：dist 被编进二进制，解压版/安装版/Linux
/// headless 即使磁盘上没有 dist 目录，Web 页面依然可用。build.rs 保证编译期
/// 该目录始终存在（全新 clone 未跑 npm build 时建最小占位）。
static EMBEDDED_DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../dist");

/// 内嵌的 index.html 字节（SPA 入口 + 深链接回落目标）。
pub fn embedded_index() -> Option<&'static [u8]> {
    EMBEDDED_DIST.get_file("index.html").map(|f| f.contents())
}

/// 按路径取内嵌资源字节；路径为相对 dist 根（如 `assets/foo.js`）。
pub fn embedded_asset(path: &str) -> Option<&'static [u8]> {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        return embedded_index();
    }
    EMBEDDED_DIST
        .get_file(path)
        .map(|f| f.contents())
}

/// 递归列出某个子目录下的所有文件路径（相对 dist 根），供诊断/测试。
pub fn embedded_file(prefix: &str) -> Option<Vec<String>> {
    let prefix = prefix.trim_end_matches('/');
    let dir = if prefix.is_empty() {
        Some(&EMBEDDED_DIST)
    } else {
        EMBEDDED_DIST.get_dir(prefix)
    }?;
    let mut out = Vec::new();
    fn walk(dir: &Dir<'_>, out: &mut Vec<String>) {
        for f in dir.files() {
            out.push(f.path().to_string_lossy().replace('\\', "/"));
        }
        for d in dir.dirs() {
            walk(d, out);
        }
    }
    walk(dir, &mut out);
    Some(out)
}

/// 按扩展名推断 Content-Type；未知类型回落 `application/octet-stream`。
pub fn embedded_mime(path: &str) -> String {
    mime_guess::from_path(path)
        .first()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string())
}

/// 内嵌 dist 的 SPA fallback handler：命中真实文件则返回（带正确 MIME），
/// 其余路径（含 SPA 深链接）回落 index.html。
///
/// 与磁盘版 [`spa_service`] 行为一致，但资源来自二进制而非文件系统——
/// 这就是「Web 入口不依赖外部 dist 目录」的实现。
pub async fn embedded_fallback(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if let Some(bytes) = embedded_asset(path) {
        let mime = embedded_mime(path);
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, mime.as_str())],
            bytes.to_vec(),
        )
            .into_response();
    }
    // SPA 回落：未知路径一律给 index.html（200），让前端路由自行处理
    match embedded_index() {
        Some(html) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            html.to_vec(),
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

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
        // 开发态 exe 由 `cargo run` 从 src-tauri/ 拉起（cwd=src-tauri），
        // tauri dev watcher 重启后的进程实测会丢失 OPX_WEB_DIST 环境变量
        // → 静态路由消失（404）。向上找一层仓库根的 dist 兜底。
        if let Some(parent) = cwd.parent() {
            candidates.push(parent.join("dist"));
        }
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

    // ===== 内嵌 dist（include_dir!）回归 =====

    /// 内嵌产物必须含 index.html（build.rs 保证全新 clone 也有最小占位）。
    /// 这是「Web 入口自包含、不依赖外部 dist 目录」的核心不变量。
    #[test]
    fn embedded_dist_has_index_html() {
        let idx = super::embedded_index().expect("内嵌 dist 必须含 index.html");
        let html = String::from_utf8_lossy(idx);
        assert!(
            html.to_lowercase().contains("<html") || html.to_lowercase().contains("<!doctype"),
            "index.html 应是 HTML，实际前 80 字节: {}",
            &html[..html.len().min(80)]
        );
    }

    /// 内嵌资源可被路径检索（取一个真实存在的 assets 文件）。
    #[test]
    fn embedded_dist_serves_asset_by_path() {
        let asset = super::embedded_file("assets/").expect("内嵌 dist 应含 assets/");
        assert!(
            asset.iter().any(|p| p.ends_with(".js") || p.ends_with(".css")),
            "assets/ 下应有 js/css 产物，实际: {:?}",
            asset.iter().take(5).collect::<Vec<_>>()
        );
    }

    /// MIME 推断：.js -> javascript，.html -> text/html，未知 -> octet-stream。
    #[test]
    fn embedded_mime_guess() {
        assert!(super::embedded_mime("main.js").starts_with("text/javascript")
            || super::embedded_mime("main.js").starts_with("application/javascript"));
        assert!(super::embedded_mime("index.html").starts_with("text/html"));
        assert_eq!(super::embedded_mime("file.unknownext"), "application/octet-stream");
    }

    /// 内嵌 fallback 路由：请求页面路径返回内嵌 index.html（200），
    /// 证明无外部 dist 目录时 Web 入口仍可用——这正是解压版/安装版/Linux 所需。
    #[tokio::test]
    async fn embedded_fallback_serves_spa_without_disk_dist() {
        let app = crate::router_embedded(test_state());
        let res = app
            .clone()
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "内嵌 index 应 200");
        let body = axum::body::to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
        let html = String::from_utf8_lossy(&body);
        assert!(html.to_lowercase().contains("<html") || html.to_lowercase().contains("<!doctype"));

        // SPA 深链接也回落到内嵌 index.html
        let res = app
            .oneshot(Request::get("/settings/web").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "内嵌 SPA 深链接应回落 index");
    }

    /// API 路由优先级：内嵌 fallback 不能吞掉 /api/*。
    #[tokio::test]
    async fn embedded_router_keeps_api_routes() {
        let app = crate::router_embedded(test_state());
        // /api/health 无 token → 401（说明命中了 API 路由而非静态 fallback）
        let res = app
            .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "/api/* 应优先于静态 fallback");
    }
}
