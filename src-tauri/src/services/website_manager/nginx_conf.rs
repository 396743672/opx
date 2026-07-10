use crate::models::website::{Location, LocationKind, Site};

/// 生成单个站点的 nginx server 块（保存到 conf/sites/<id>.conf）
pub fn generate_server_block(site: &Site) -> String {
    let server_name = site
        .server_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("_");

    let mut out = String::new();
    out.push_str("server {\n");
    out.push_str(&format!("    listen {};\n", site.listen));
    out.push_str(&format!("    server_name {};\n", server_name));
    for loc in &site.locations {
        out.push_str(&generate_location(loc));
    }
    out.push_str("}\n");
    out
}

fn generate_location(loc: &Location) -> String {
    let mut s = format!("    location {} {{\n", loc.path);
    match loc.kind {
        LocationKind::Static => {
            if let Some(root) = &loc.root {
                // nginx 用正斜杠；含空格加引号，避免 Windows 路径转义问题
                s.push_str(&format!("        root \"{}\";\n", root.replace('\\', "/")));
            }
            s.push_str("        index index.html;\n");
            if loc.spa_fallback {
                s.push_str("        try_files $uri $uri/ /index.html;\n");
            }
        }
        LocationKind::Proxy => {
            if let Some(target) = &loc.target {
                s.push_str(&format!("        proxy_pass {};\n", target));
                s.push_str("        proxy_set_header Host $host;\n");
                s.push_str("        proxy_set_header X-Real-IP $remote_addr;\n");
                s.push_str("        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n");
                s.push_str("        proxy_set_header X-Forwarded-Proto $scheme;\n");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::website::{Location, LocationKind, Site, SslConfig, StaticSource};

    fn site_with(locations: Vec<Location>, server_name: Option<&str>) -> Site {
        Site {
            id: "s1".to_string(),
            name: "t".to_string(),
            server_name: server_name.map(|s| s.to_string()),
            listen: 80,
            ssl: SslConfig::default(),
            enabled: true,
            locations,
        }
    }

    fn static_loc() -> Location {
        Location {
            path: "/".to_string(),
            kind: LocationKind::Static,
            source: Some(StaticSource::Upload),
            root: Some("C:\\nginx\\sites-data\\s1\\root".to_string()),
            spa_fallback: true,
            target: None,
        }
    }

    fn proxy_loc() -> Location {
        Location {
            path: "/api".to_string(),
            kind: LocationKind::Proxy,
            source: None,
            root: None,
            spa_fallback: false,
            target: Some("http://127.0.0.1:8080".to_string()),
        }
    }

    #[test]
    fn static_block_has_root_and_spa_fallback_with_forward_slashes() {
        let block = generate_server_block(&site_with(vec![static_loc()], Some("www.demo.com")));
        assert!(block.contains("listen 80;"));
        assert!(block.contains("server_name www.demo.com;"));
        assert!(block.contains("root \"C:/nginx/sites-data/s1/root\";"));
        assert!(block.contains("try_files $uri $uri/ /index.html;"));
        assert!(!block.contains('\\'), "路径应已转为正斜杠");
    }

    #[test]
    fn proxy_block_has_proxy_pass_and_headers() {
        let block = generate_server_block(&site_with(vec![proxy_loc()], None));
        assert!(block.contains("server_name _;"), "空域名应生成 _");
        assert!(block.contains("location /api {"));
        assert!(block.contains("proxy_pass http://127.0.0.1:8080;"));
        assert!(block.contains("proxy_set_header Host $host;"));
    }

    #[test]
    fn mixed_block_has_both_locations() {
        let block = generate_server_block(&site_with(vec![static_loc(), proxy_loc()], Some("admin.demo.com")));
        assert!(block.contains("location / {"));
        assert!(block.contains("location /api {"));
    }

    #[test]
    fn ensure_include_injects_once_and_is_idempotent() {
        let conf = "worker_processes auto;\nhttp {\n    server_tokens off;\n}\n";
        let once = ensure_include(conf);
        assert!(once.contains("include sites/*.conf;"));
        let twice = ensure_include(&once);
        assert_eq!(once, twice, "已有 include 时不应重复注入");
    }
}
