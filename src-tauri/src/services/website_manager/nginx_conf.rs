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
                if !target.trim().is_empty() {
                    s.push_str(&format!("        proxy_pass {};\n", target));
                    s.push_str("        proxy_http_version 1.1;\n");
                    s.push_str("        proxy_set_header Host $host;\n");
                    s.push_str("        proxy_set_header X-Real-IP $remote_addr;\n");
                    s.push_str("        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n");
                    s.push_str("        proxy_set_header X-Forwarded-Proto $scheme;\n");
                    // WebSocket / 长连接支持（$connection_upgrade 由主配置 map 提供）
                    s.push_str("        proxy_set_header Upgrade $http_upgrade;\n");
                    s.push_str("        proxy_set_header Connection $connection_upgrade;\n");
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
            custom_conf: false,
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
        // WebSocket / 长连接支持
        assert!(block.contains("proxy_http_version 1.1;"));
        assert!(block.contains("proxy_set_header Upgrade $http_upgrade;"));
        assert!(block.contains("proxy_set_header Connection $connection_upgrade;"));
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

    #[test]
    fn ensure_map_upgrade_injects_once_and_is_idempotent() {
        let conf = "worker_processes auto;\nhttp {\n    server_tokens off;\n}\n";
        let once = ensure_map_upgrade(conf);
        assert!(once.contains("map $http_upgrade $connection_upgrade {"));
        assert!(once.contains("default upgrade;"));
        assert!(once.contains("''      close;"));
        let twice = ensure_map_upgrade(&once);
        assert_eq!(once, twice, "已有 map 时不应重复注入");
    }

    #[test]
    fn site_conf_filename_sanitizes_name_and_appends_port_and_short_id() {
        let site = site_with(vec![static_loc()], Some("www.demo.com"));
        // site_with 的 id 是 "s1"，name 是 "t"，listen 80
        let fname = site_conf_filename(&site);
        assert_eq!(fname, "t_80_s1.conf");
    }

    #[test]
    fn site_conf_filename_handles_cjk_and_special_chars() {
        let mut site = site_with(vec![static_loc()], None);
        site.name = "我的站点 demo!".to_string();
        site.id = "abcd1234efgh".to_string();
        site.listen = 8080;
        let fname = site_conf_filename(&site);
        // 中文/空格/叹号 → _，去首尾 _，短 id 取前 8 位
        assert_eq!(fname, "demo_8080_abcd1234.conf");
    }

    #[test]
    fn site_conf_filename_falls_back_to_site_when_name_empty() {
        let mut site = site_with(vec![static_loc()], None);
        site.name = "中文".to_string();
        site.id = "xyz".to_string();
        site.listen = 80;
        let fname = site_conf_filename(&site);
        assert_eq!(fname, "site_80_xyz.conf", "纯非 ASCII 名称应回退为 site");
    }
}
