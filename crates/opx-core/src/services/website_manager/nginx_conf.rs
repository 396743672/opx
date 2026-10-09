use crate::models::website::{Location, LocationKind, Site};

/// 生成站点配置文件名：`{sanitize(name)}_{listen}_{short_id}.conf`
/// - name 中非 ASCII 字母数字/连字符统一转 `_`，去首尾 `_`，为空则用 `site`
/// - short_id 取 id 前 8 位，保证同名同端口不冲突、且改名不丢失手写配置
pub fn site_conf_filename(site: &Site) -> String {
    let name: String = site
        .name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
        .collect();
    let name = name.trim_matches('_');
    let name = if name.is_empty() { "site" } else { name };
    let short_id: String = site.id.chars().take(8).collect();
    format!("{}_{}_{}.conf", name, site.listen, short_id)
}

/// 生成单个站点的 nginx server 块（保存到 conf/sites/<name_port_id>.conf）
/// 相对路径（sites-data/...）统一写成 `../sites-data/...`：nginx 把 conf 内的相对路径
/// 以 conf/ 目录为基准解析，`../` 上跳一级即 nginx 根目录，证书/静态目录都在根下，
/// 这样无需写死安装路径，也避免 `\` 被 nginx 当转义符。
pub fn generate_server_block(site: &Site) -> String {
    let server_name = site
        .server_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("_");

    let mut out = String::new();
    out.push_str("server {\n");
    // SSL 主块端口：统一用标准 443（https 默认），80 由下方跳转块占用；
    // 不依赖 site.listen，用户无需为 https 改端口
    if site.ssl.enabled {
        out.push_str("    listen 443 ssl;\n");
        if let Some(c) = &site.ssl.cert_path {
            out.push_str(&format!("    ssl_certificate {};\n", to_conf_rel(c)));
        }
        if let Some(k) = &site.ssl.key_path {
            out.push_str(&format!("    ssl_certificate_key {};\n", to_conf_rel(k)));
        }
        out.push_str("    ssl_protocols TLSv1.2 TLSv1.3;\n");
        out.push_str("    ssl_session_cache shared:SSL:10m;\n");
    } else {
        out.push_str(&format!("    listen {};\n", site.listen));
    }
    out.push_str(&format!("    server_name {};\n", server_name));

    // 收集所有 upstream 块（多后端 location）
    let short_id: String = site.id.chars().take(8).collect();
    let mut emitted_upstreams: Vec<String> = Vec::new();
    for loc in &site.locations {
        if loc.kind == LocationKind::Proxy {
            let addrs: Vec<&str> = loc
                .upstreams
                .iter()
                .map(|t| t.addr.trim())
                .filter(|a| !a.is_empty())
                .collect();
            if addrs.len() > 1 {
                let name = format!("site_{}_{}", short_id, sanitize_path(&loc.path));
                if !emitted_upstreams.contains(&name) {
                    let mut b = format!("upstream {} {{\n", name);
                    for addr in &addrs {
                        b.push_str(&format!("    server {};\n", addr));
                    }
                    b.push_str("}\n");
                    out.push_str(&b);
                    emitted_upstreams.push(name);
                }
            }
        }
    }

    for loc in &site.locations {
        out.push_str(&generate_location(loc, &short_id));
    }
    out.push_str("}\n");

    // 启用 SSL 时只保留用户配置端口的 http→https 跳转（不硬编码 80，避免与外部占用冲突）
    if site.ssl.enabled && site.listen != 443 && site.listen != 0 {
        out.push_str("server {\n");
        out.push_str(&format!("    listen {};\n", site.listen));
        out.push_str(&format!("    server_name {};\n", server_name));
        out.push_str("    return 301 https://$host$request_uri;\n");
        out.push_str("}\n");
    }

    out
}

fn sanitize_path(path: &str) -> String {
    let s: String = path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let s = s.trim_matches('_');
    if s.is_empty() { "root".to_string() } else { s.to_string() }
}

/// 证书/私钥路径写进 conf：ssl_certificate/key 的路径以 conf/ 为基准（实测），
/// 相对路径（sites-data/...）需加 `../` 上跳一级到 nginx 根。
/// 绝对路径（盘符或 `/` 开头）原样转正斜杠（nginx 会把 `\` 当转义符）。
/// NOTE: root/alias 的基准是 prefix，不能加 `../`，见 [`to_root`]。
fn to_conf_rel(p: &str) -> String {
    let p = p.replace('\\', "/");
    let is_abs = p.starts_with('/') || (p.len() > 2 && p.as_bytes()[1] == b':');
    if is_abs {
        return p;
    }
    let p = p.trim_start_matches('/');
    if p.starts_with("../") {
        p.to_string()
    } else {
        format!("../{}", p)
    }
}

/// root 指令路径：nginx 以其 prefix（nginx 根目录）为基准，保持相对即可；
/// 实测含 `..` 的 root 会导致 500（rewrite cycle）。仅统一正斜杠防转义。
fn to_root(p: &str) -> String {
    p.replace('\\', "/")
}

fn generate_location(loc: &Location, short_id: &str) -> String {
    let mut s = format!("    location {} {{\n", loc.path);
    match loc.kind {
        LocationKind::Static => {
            if let Some(root) = &loc.root {
                // root 基准是 nginx prefix，保持相对路径即可（不加 ../，nginx 对含 .. 的 root 会 500）
                // ponytail: 存相对路径（sites-data/{name}/{path}），生成时仅统一正斜杠
                s.push_str(&format!("        root \"{}\";\n", to_root(root)));
            }
            s.push_str("        index index.html;\n");
            if loc.spa_fallback {
                s.push_str("        try_files $uri $uri/ /index.html;\n");
            }
        }
        LocationKind::Proxy => {
            let subpath = loc
                .proxy_subpath
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let upstream_count = loc
                .upstreams
                .iter()
                .filter(|t| !t.addr.trim().is_empty())
                .count();
            let proxy_target: Option<String> = if upstream_count > 1 {
                Some(format!("http://site_{}_{}", short_id, sanitize_path(&loc.path)))
            } else {
                loc.target.clone()
            };
            if let Some(base) = proxy_target {
                let base = match subpath {
                    Some(sp) => format!("{}{}", base.trim_end_matches('/'), sp),
                    None => base,
                };
                s.push_str(&format!("        proxy_pass {};\n", base));
                s.push_str("        proxy_http_version 1.1;\n");
                s.push_str("        proxy_set_header Host $host;\n");
                s.push_str("        proxy_set_header X-Real-IP $remote_addr;\n");
                s.push_str("        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n");
                s.push_str("        proxy_set_header X-Forwarded-Proto $scheme;\n");
                s.push_str("        proxy_set_header Upgrade $http_upgrade;\n");
                s.push_str("        proxy_set_header Connection $connection_upgrade;\n");
                for h in &loc.proxy_headers {
                    if !h.name.trim().is_empty() {
                        s.push_str(&format!("        proxy_set_header {} {};\n", h.name, h.value));
                    }
                }
            }
        }
    }
    s.push_str("    }\n");
    s
}

/// 确保主配置 http {} 块内含 `include sites/*.conf;`，缺失则注入一次（幂等）
pub fn ensure_include(nginx_conf: &str) -> String {
    if nginx_conf.contains("sites/*.conf") {
        return nginx_conf.to_string();
    }
    // 在第一个 http { 的左花括号之后插入
    let http_idx = nginx_conf.find("http {").or_else(|| nginx_conf.find("http{"));
    if let Some(idx) = http_idx {
        if let Some(brace_off) = nginx_conf[idx..].find('{') {
            let pos = idx + brace_off + 1;
            let mut out = String::with_capacity(nginx_conf.len() + 32);
            out.push_str(&nginx_conf[..pos]);
            out.push_str("\n    include sites/*.conf;\n");
            out.push_str(&nginx_conf[pos..]);
            return out;
        }
    }
    nginx_conf.to_string()
}

/// 启用 nginx JSON 访问日志（幂等）：在 http {} 块顶部注入
/// `log_format opx_json escape=json '...'` 与 `access_log logs/access.log opx_json;`。
/// JSON 结构化行便于后续日志分析/防护功能程序化消费。
pub fn ensure_access_log(nginx_conf: &str) -> String {
    if nginx_conf.contains("log_format opx_json") {
        return nginx_conf.to_string();
    }
    let http_idx = nginx_conf.find("http {").or_else(|| nginx_conf.find("http{"));
    let Some(idx) = http_idx else {
        return nginx_conf.to_string();
    };
    let Some(brace_off) = nginx_conf[idx..].find('{') else {
        return nginx_conf.to_string();
    };
    let pos = idx + brace_off + 1;
    let block = {
        "    log_format opx_json escape=json '{\"time\":\"$time_iso8601\",\"remote_addr\":\"$remote_addr\",\"method\":\"$request_method\",\"uri\":\"$request_uri\",\"status\":$status,\"body_bytes\":$body_bytes_sent,\"request_time\":$request_time,\"host\":\"$host\",\"referer\":\"$http_referer\",\"user_agent\":\"$http_user_agent\"}';\n    access_log logs/access.log opx_json;\n"
    };
    let mut out = String::with_capacity(nginx_conf.len() + block.len());
    out.push_str(&nginx_conf[..pos]);
    out.push_str(block);
    out.push_str(&nginx_conf[pos..]);
    out
}

/// 确保主配置 http {} 块内含 WebSocket 反代所需的
/// `map $http_upgrade $connection_upgrade { ... }`（缺失则注入一次，幂等）。
/// map 必须在 http{} 内、server{} 外，故与 ensure_include 一样注入到 http { 之后。
pub fn ensure_map_upgrade(nginx_conf: &str) -> String {
    if nginx_conf.contains("$connection_upgrade") {
        return nginx_conf.to_string();
    }
    let http_idx = nginx_conf.find("http {").or_else(|| nginx_conf.find("http{"));
    if let Some(idx) = http_idx {
        if let Some(brace_off) = nginx_conf[idx..].find('{') {
            let pos = idx + brace_off + 1;
            let block = "\n    map $http_upgrade $connection_upgrade {\n        default upgrade;\n        ''      close;\n    }\n";
            let mut out = String::with_capacity(nginx_conf.len() + block.len());
            out.push_str(&nginx_conf[..pos]);
            out.push_str(block);
            out.push_str(&nginx_conf[pos..]);
            return out;
        }
    }
    nginx_conf.to_string()
}

/// 确保主配置 http {} 块内含常用推荐设置（幂等）：
/// - client_max_body_size 200m
/// - underscores_in_headers on
/// - gzip on（原生配置里该行被注释，改为启用）
pub fn ensure_common_settings(nginx_conf: &str) -> String {
    let mut out = nginx_conf.to_string();

    if !out.contains("client_max_body_size") {
        out = out.replace("keepalive_timeout  65;", "keepalive_timeout  65;\n    client_max_body_size 200m;");
    }
    if !out.contains("underscores_in_headers") {
        out = out.replace("client_max_body_size 200m;", "client_max_body_size 200m;\n    underscores_in_headers on;");
    }
    if out.contains("#gzip  on;") {
        out = out.replace("#gzip  on;", "gzip  on;");
    }

    out
}

#[cfg(test)]
mod tests {
    use crate::models::website::{
        Location, LocationKind, ProxyHeader, Site, SslConfig, StaticSource, UpstreamTarget,
    };

    fn proxy_loc() -> Location {
        Location {
            path: "/api".into(),
            kind: LocationKind::Proxy,
            source: None,
            root: None,
            spa_fallback: false,
            target: Some("http://127.0.0.1:8080".into()),
            upstreams: vec![],
            proxy_headers: vec![ProxyHeader { name: "X-Auth".into(), value: "token".into() }],
            proxy_subpath: Some("/upstream/api".into()),
        }
    }

    fn base_site() -> Site {
        Site {
            id: "abcdef1234567890".into(),
            name: "demo".into(),
            server_name: Some("demo.local".into()),
            listen: 8080,
            ssl: SslConfig::default(),
            enabled: true,
            locations: vec![],
            custom_conf: false,
        }
    }

    #[test]
    fn multiple_upstream_generates_upstream_block() {
        let mut s = base_site();
        s.locations = vec![Location {
            upstreams: vec![
                UpstreamTarget { addr: "127.0.0.1:8081".into() },
                UpstreamTarget { addr: "127.0.0.1:8082".into() },
            ],
            target: None,
            proxy_subpath: None,
            ..proxy_loc()
        }];
        let out = super::generate_server_block(&s);
        assert!(out.contains("upstream site_abcdef12_api {"), "{}", out);
        assert!(out.contains("server 127.0.0.1:8081;"));
        assert!(out.contains("server 127.0.0.1:8082;"));
        assert!(out.contains("proxy_pass http://site_abcdef12_api;"));
        assert!(!out.contains("/upstream/api"));
    }

    #[test]
    fn subpath_and_headers_applied() {
        let mut s = base_site();
        s.locations = vec![proxy_loc()];
        let out = super::generate_server_block(&s);
        assert!(out.contains("proxy_pass http://127.0.0.1:8080/upstream/api;"), "{}", out);
        assert!(out.contains("proxy_set_header X-Auth token;"));
    }

    #[test]
    fn ssl_enabled_adds_ssl_listen_and_redirect() {
        let mut s = base_site();
        s.listen = 81; // SSL 主块固定 443，不依赖该端口
        s.ssl = SslConfig {
            enabled: true,
            cert_path: Some("sites-data/certs/demo.crt".into()),
            key_path: Some("sites-data/certs/demo.key".into()),
            ..Default::default()
        };
        let out = super::generate_server_block(&s);
        assert!(out.contains("listen 443 ssl;"), "{}", out);
        assert!(out.contains("ssl_certificate ../sites-data/certs/demo.crt;"));
        assert!(out.contains("ssl_certificate_key ../sites-data/certs/demo.key;"));
        assert!(!out.contains("listen 80;")); // 不硬编码 80，避免外部占用冲突
        assert!(out.contains("listen 81;")); // 仅保留用户配置端口的 http→https 跳转
        assert!(out.contains("return 301 https://$host$request_uri;"));
    }

    #[test]
    fn ssl_disabled_no_redirect_block() {
        let s = base_site();
        let out = super::generate_server_block(&s);
        assert!(!out.contains(" ssl;"), "{}", out);
        assert!(!out.contains("return 301"));
    }

    #[test]
    fn root_kept_relative_without_dotdot() {
        // root 基准是 nginx prefix，须保持相对路径；nginx 对含 ../ 的 root 会 500
        let mut s = base_site();
        s.locations = vec![Location {
            path: "/".into(),
            kind: LocationKind::Static,
            source: Some(StaticSource::Dir),
            root: Some("sites-data/demo/root".into()),
            spa_fallback: true,
            target: None,
            upstreams: vec![],
            proxy_headers: vec![],
            proxy_subpath: None,
        }];
        let out = super::generate_server_block(&s);
        assert!(out.contains("root \"sites-data/demo/root\";"), "{}", out);
        assert!(!out.contains("../"), "{}", out);
    }
}
