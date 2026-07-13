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
