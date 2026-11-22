//! 静态 SPA 托管（批次 4.5，设计 §5「`/` 服务 Vue SPA 构建产物」）。
//!
//! - 资源查找顺序：**内嵌资源优先**（宿主经 [`AssetProvider`] 注入，即
//!   Tauri 编译期嵌入 `frontendDist` 的字节）→ 文件系统 dist 回退。
//!   打包后便携 zip 只有 exe、无外部 dist，靠内嵌资源提供页面；
//!   `tauri dev` 由 Tauri 自身的回退读 `frontendDist`，无需外部 dist。
//! - dist 目录解析（回退路径）：`OPX_WEB_DIST` 环境变量 → exe 同级 `dist/`
//!   → 当前工作目录 `dist/` → 其父级 `dist/`；以 `index.html` 存在为判据。
//!   两者皆无则不下挂静态路由（纯 API 模式，headless 无前端产物也能跑）。
//! - SPA fallback：未匹配路径回落 `index.html`（vue-router history 模式）。
//!   路径先归一化并拒绝 `..`/`/`/`\` 穿越（等价 ServeDir 内建防护）。
//! - dist **不进 git**（根 .gitignore 已有 `/dist`）；桌面壳与 headless
//!   共用本 crate，零 tauri 依赖由闭包注入维持。

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::{Extension, Router};

/// 宿主注入的内嵌资源条目（值对象，opx-http 本身零 tauri 依赖）。
///
/// 由桌面壳用 Tauri `AppHandle::asset_resolver()` 读取编译期嵌入的
/// `frontendDist` 字节后构造；`mime_type` 直接沿用 Tauri 的判定结果。
#[derive(Clone, Debug)]
pub struct EmbeddedAsset {
    pub bytes: Vec<u8>,
    pub mime_type: String,
}

/// 内嵌资源提供者：输入规范化相对路径（如 `index.html`、
/// `assets/main.js`），命中返回资源字节与 MIME。
///
/// 用类型别名而非 trait，宿主只需传一个闭包——保持 opx-http 对 tauri
/// 零依赖（桌面壳与 headless 共用本 crate 的边界不被破坏）。
pub type AssetProvider = Arc<dyn Fn(&str) -> Option<EmbeddedAsset> + Send + Sync>;

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

/// 归一化 URL 路径为安全的相对路径片段。
///
/// - 去前导 `/`、末尾 `/`；空 → `index.html`；
/// - 百分号解码（`%20` → 空格），`\` 统一为 `/`；
/// - 拒绝 `.`/`..`/根组件（等价 ServeDir 内建路径穿越防护）。
fn normalize_asset_path(raw: &str) -> Option<String> {
    let decoded = percent_decode(raw).replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    for seg in decoded.split('/') {
        match seg {
            "" | "." => continue,
            ".." | "/" => return None,
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        Some("index.html".to_string())
    } else {
        Some(parts.join("/"))
    }
}

/// 极简百分号解码：仅处理合法 `%XX`，非法序列原样保留（零新增依赖）。
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = |b: u8| -> Option<u8> {
                match b {
                    b'0'..=b'9' => Some(b - b'0'),
                    b'a'..=b'f' => Some(b - b'a' + 10),
                    b'A'..=b'F' => Some(b - b'A' + 10),
                    _ => None,
                }
            };
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(hi << 4 | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn asset_response(bytes: Vec<u8>, mime: &str) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .body(Body::from(bytes))
        .expect("构造静态资源响应不应失败")
}

/// fallback 资源集合（Extension 注入，随 Router clone 共享，只读）。
#[derive(Clone)]
struct SpaResources {
    dist: Option<PathBuf>,
    provider: Option<AssetProvider>,
}

/// 查单个资源：内嵌优先，文件系统回退。返回 (bytes, mime)。
fn lookup(resources: &SpaResources, rel: &str) -> Option<(Vec<u8>, String)> {
    // 1. 内嵌资源（编译期打入 exe 的 frontendDist；portable 场景唯一来源）
    if let Some(asset) = resources.provider.as_ref().and_then(|p| p(rel)) {
        return Some((asset.bytes, asset.mime_type));
    }
    // 2. 文件系统 dist（tauri dev / OPX_WEB_DIST / adjacent 安装形态）
    let path = resources.dist.as_ref()?.join(rel);
    if !path.is_file() {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    let mime = mime_guess::from_path(&path)
        .first_or_octet_stream()
        .to_string();
    Some((bytes, mime))
}

/// 把静态 SPA fallback 挂到主 Router 上。
///
/// 内嵌资源（宿主经 [`AssetProvider`] 注入）优先，文件系统 dist 回退；
/// 两者皆无则纯 API 模式（未知路径 404）。未匹配路径统一回落 `index.html`
/// （vue-router history 深链接刷新可用）。
pub fn mount_fallback(
    app: Router,
    dist: Option<PathBuf>,
    provider: Option<AssetProvider>,
) -> Router {
    if dist.is_none() && provider.is_none() {
        return app;
    }
    let resources = SpaResources { dist, provider };
    // 顺序关键：先挂 fallback 再 layer。axum 的 layer 只作用于调用时已存在的
    // endpoint，若先 layer 后 fallback，新挂的 fallback handler 拿不到 Extension。
    app.fallback(fallback_handler)
        .layer(Extension(resources))
}

/// 统一 fallback handler：内嵌优先 → 文件系统 → SPA index.html。
async fn fallback_handler(
    Extension(resources): Extension<SpaResources>,
    req: Request,
) -> Response {
    let Some(rel) = normalize_asset_path(req.uri().path()) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
    };

    if let Some((bytes, mime)) = lookup(&resources, &rel) {
        return asset_response(bytes, &mime);
    }
    if rel != "index.html" {
        if let Some((bytes, mime)) = lookup(&resources, "index.html") {
            return asset_response(bytes, &mime);
        }
    }
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap()
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

    /// 构造内存资源表 provider，模拟 Tauri asset_resolver 读到的内嵌字节。
    fn map_provider(entries: &[(&str, &str, &str)]) -> AssetProvider {
        let map: std::collections::HashMap<String, (Vec<u8>, String)> = entries
            .iter()
            .map(|(k, v, mime)| {
                (k.to_string(), (v.as_bytes().to_vec(), mime.to_string()))
            })
            .collect();
        Arc::new(move |path: &str| {
            map.get(path).map(|(bytes, mime)| EmbeddedAsset {
                bytes: bytes.clone(),
                mime_type: mime.clone(),
            })
        })
    }

    /// 复现便携 zip 场景：文件系统无 dist，仅有内嵌资源 → `/` 应返回 index.html。
    #[tokio::test]
    async fn embedded_only_serves_index_without_filesystem_dist() {
        let provider = map_provider(&[("index.html", "<html>embedded-opx</html>", "text/html")]);
        let app = crate::router_with_assets(test_state(), None, Some(provider));

        let res = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/html"));
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("embedded-opx"));
    }

    /// 内嵌资源优先于文件系统 dist（同名文件内嵌版胜出）。
    #[tokio::test]
    async fn embedded_asset_takes_priority_over_dist() {
        let dist = make_tmp_dist("priority");
        let provider = map_provider(&[("index.html", "<html>EMBEDDED-WINS</html>", "text/html")]);
        let app = crate::router_with_assets(test_state(), Some(dist.clone()), Some(provider));

        let res = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("EMBEDDED-WINS"));
        let _ = std::fs::remove_dir_all(&dist);
    }

    /// 内嵌资源缺失的深链接 → SPA fallback 回落到内嵌 index.html。
    #[tokio::test]
    async fn embedded_only_spa_fallback() {
        let provider = map_provider(&[("index.html", "<html>spa-home</html>", "text/html")]);
        let app = crate::router_with_assets(test_state(), None, Some(provider));

        let res = app
            .oneshot(
                Request::get("/settings/deep-link")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("spa-home"));
    }

    /// 路径穿越被拒（无 dist、无匹配资源 → 404，且不得读到外部文件）。
    #[tokio::test]
    async fn path_traversal_rejected() {
        let provider = map_provider(&[("index.html", "<html>spa</html>", "text/html")]);
        let app = crate::router_with_assets(test_state(), None, Some(provider));

        let res = app
            .oneshot(
                Request::get("/../../etc/passwd")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
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
