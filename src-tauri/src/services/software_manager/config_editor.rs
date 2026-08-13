use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::models::software::ConfigSchema;

pub type FormData = std::collections::BTreeMap<String, serde_json::Value>;

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigFormat {
    Ini,
    KeyValue,
    NginxConf,
    Json,
    Plaintext,
}

pub fn detect_format(file_path: &Path) -> ConfigFormat {
    let name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.ends_with(".ini") || name == "my.cnf" {
        ConfigFormat::Ini
    } else if name == "postgresql.conf" {
        // PostgreSQL 配置是 `key = value`（等号分隔、无 section），本质即 INI 语法
        ConfigFormat::Ini
    } else if name == "redis.conf" {
        ConfigFormat::KeyValue
    } else if name.ends_with(".conf") {
        if name == "nginx.conf" {
            ConfigFormat::NginxConf
        } else {
            ConfigFormat::KeyValue
        }
    } else if name.ends_with(".json") {
        ConfigFormat::Json
    } else {
        ConfigFormat::Plaintext
    }
}

/// 从配置文件读出表单数据
pub fn read_config_as_form(file_path: &Path, schema: &ConfigSchema) -> Result<FormData> {
    let content = std::fs::read_to_string(file_path)?;
    let format = detect_format(file_path);
    let mut form = FormData::new();

    for field in &schema.fields {
        let value = match format {
            ConfigFormat::Ini => ini_lookup(&content, field.section.as_deref(), &field.key),
            ConfigFormat::KeyValue => kv_lookup(&content, &field.key),
            ConfigFormat::NginxConf => nginx_lookup(&content, &field.key),
            ConfigFormat::Json => json_lookup(&content, &field.key),
            ConfigFormat::Plaintext => serde_json::Value::Null,
        };
        form.insert(field.key.clone(), value);
    }
    Ok(form)
}

/// 把表单数据写回配置文件（保留未涉及的字段）
pub fn write_form_to_config(
    file_path: &Path,
    schema: &ConfigSchema,
    form: &FormData,
) -> Result<()> {
    let content = std::fs::read_to_string(file_path).unwrap_or_default();
    let format = detect_format(file_path);
    let mut new_content = content;

    for (key, value) in form {
        // 跳过 ephemeral 字段：敏感一次性值（如 MySQL 初始化密码）绝不写入配置文件
        if schema.ephemeral_keys.iter().any(|k| k == key) {
            continue;
        }
        let field = schema.fields.iter().find(|f| &f.key == key);
        if let Some(field) = field {
            new_content = match format {
                ConfigFormat::Ini => {
                    ini_upsert(&new_content, field.section.as_deref(), key, value)?
                }
                ConfigFormat::KeyValue => kv_upsert(&new_content, key, value)?,
                ConfigFormat::NginxConf => nginx_upsert(&new_content, key, value)?,
                ConfigFormat::Json => json_upsert(&new_content, key, value)?,
                ConfigFormat::Plaintext => new_content,
            };
        }
    }

    backup_config(file_path)?;
    // 原子写：写 .tmp + rename，避免崩溃/断电损坏配置文件
    let tmp_path = file_path.with_extension("tmp");
    std::fs::write(&tmp_path, new_content)?;
    std::fs::rename(&tmp_path, file_path)?;
    Ok(())
}

pub fn read_config_source(file_path: &Path) -> Result<String> {
    Ok(std::fs::read_to_string(file_path)?)
}

pub fn write_config_source(file_path: &Path, content: &str) -> Result<()> {
    backup_config(file_path)?;
    // 原子写
    let tmp_path = file_path.with_extension("tmp");
    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, file_path)?;
    Ok(())
}

pub fn backup_config(file_path: &Path) -> Result<PathBuf> {
    let parent = file_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("配置文件路径无父目录: {}", file_path.display()))?;
    let backup_dir = parent.join("backups");
    std::fs::create_dir_all(&backup_dir)?;

    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config");
    let backup_name = format!("{}_{}", ts, filename);
    let backup_path = backup_dir.join(&backup_name);
    if file_path.exists() {
        std::fs::copy(file_path, &backup_path)?;
    }
    // 按文件名后缀过滤，只清理本配置文件的备份，避免误删其他文件的备份
    cleanup_old_backups(&backup_dir, filename, 5)?;
    Ok(backup_path)
}

fn cleanup_old_backups(dir: &Path, target_filename: &str, retain: usize) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            // 仅保留以 _{target_filename} 结尾的条目（本配置文件的备份）
            e.file_name()
                .to_str()
                .map(|name| name.ends_with(&format!("_{}", target_filename)))
                .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());
    if entries.len() > retain {
        for entry in entries.iter().take(entries.len() - retain) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    Ok(())
}

// —— INI 解析（[section] + key=value） ——

/// 规范化 section 名：去掉两端的方括号（schema 可存 "[mysqld]" 或 "mysqld"）
fn normalize_section(s: &str) -> &str {
    let s = s.trim();
    if s.starts_with('[') && s.ends_with(']') && s.len() >= 2 {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

fn ini_lookup(content: &str, section: Option<&str>, key: &str) -> serde_json::Value {
    let target_section = section.map(normalize_section);
    let mut current_section: Option<String> = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_section = Some(trimmed[1..trimmed.len() - 1].to_string());
            continue;
        }
        if trimmed.starts_with('#') || trimmed.starts_with(';') || trimmed.is_empty() {
            continue;
        }
        if let Some(eq) = trimmed.find('=') {
            let k = trimmed[..eq].trim();
            let v = trimmed[eq + 1..].trim();
            let section_match = match (&target_section, &current_section) {
                (Some(s), Some(cs)) => s == cs,
                (None, None) => true,
                _ => false,
            };
            if section_match && k == key {
                return parse_value(v);
            }
        }
    }
    serde_json::Value::Null
}

fn ini_upsert(
    content: &str,
    section: Option<&str>,
    key: &str,
    value: &serde_json::Value,
) -> Result<String> {
    let v_str = value_to_string(value);
    let target_section = section.map(normalize_section);
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut section_idx: Option<usize> = None;
    let mut found = false;

    if let Some(s) = target_section {
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim();
            if t.starts_with('[') && t.ends_with(']') && t.len() >= 2 {
                let inner = &t[1..t.len() - 1];
                if inner == s {
                    section_idx = Some(i);
                    break;
                }
            }
        }
        if section_idx.is_none() {
            lines.push(String::new());
            lines.push(format!("[{}]", s));
            lines.push(format!("{}={}", key, v_str));
            return Ok(lines.join("\n"));
        }
    }

    let start = section_idx.unwrap_or(0);
    let mut insert_at = start + 1;
    for i in start + 1..lines.len() {
        let t = lines[i].trim();
        // section=None 时，遇到第一个 [section] 块就停止（与 ini_lookup 语义对称）
        if t.starts_with('[') && t.ends_with(']') {
            break;
        }
        if let Some(eq) = t.find('=') {
            let k = t[..eq].trim();
            if k == key {
                lines[i] = format!("{}={}", key, v_str);
                found = true;
                break;
            }
        }
        insert_at = i + 1;
    }

    if !found {
        lines.insert(insert_at, format!("{}={}", key, v_str));
    }
    Ok(lines.join("\n"))
}

// —— KeyValue 解析（key value 空格分隔，无 section） ——

fn kv_lookup(content: &str, key: &str) -> serde_json::Value {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
            if k == key {
                return parse_value(v.trim());
            }
        }
    }
    serde_json::Value::Null
}

fn kv_upsert(
    content: &str,
    key: &str,
    value: &serde_json::Value,
) -> Result<String> {
    let v_str = value_to_string(value);
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut found = false;
    for i in 0..lines.len() {
        let t = lines[i].trim();
        if t.starts_with('#') || t.is_empty() {
            continue;
        }
        let mut parts = t.splitn(2, char::is_whitespace);
        if let Some(k) = parts.next() {
            if k == key {
                if v_str.is_empty() {
                    // 空值：注释掉该行，避免配置文件中出现 bare key 如 `requirepass` 导致启动失败
                    lines[i] = format!("#{}", lines[i]);
                } else {
                    lines[i] = format!("{} {}", key, v_str);
                }
                found = true;
                break;
            }
        }
    }
    if !found && !v_str.is_empty() {
        lines.push(format!("{} {}", key, v_str));
    }
    Ok(lines.join("\n"))
}

// —— NginxConf 解析（行级匹配 + upsert） ——

fn nginx_lookup(content: &str, key: &str) -> serde_json::Value {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        if let (Some(k), Some(rest)) = (parts.next(), parts.next()) {
            if k == key {
                let v = rest.trim_end_matches(';').trim();
                return parse_value(v);
            }
        }
    }
    serde_json::Value::Null
}

fn nginx_upsert(
    content: &str,
    key: &str,
    value: &serde_json::Value,
) -> Result<String> {
    let v_str = value_to_string(value);
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut found = false;
    for i in 0..lines.len() {
        let t = lines[i].trim();
        if t.starts_with('#') || t.is_empty() {
            continue;
        }
        let mut parts = t.splitn(2, char::is_whitespace);
        if let Some(k) = parts.next() {
            if k == key {
                if v_str.is_empty() {
                    lines[i] = format!("#{}", lines[i]);
                } else {
                    lines[i] = format!("{} {};", key, v_str);
                }
                found = true;
                break;
            }
        }
    }
    if !found && !v_str.is_empty() {
        lines.push(format!("{} {};", key, v_str));
    }
    Ok(lines.join("\n"))
}

// —— JSON 解析 ——

fn json_lookup(content: &str, key: &str) -> serde_json::Value {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(content) {
        if let Some(val) = v.get(key) {
            return val.clone();
        }
    }
    serde_json::Value::Null
}

fn json_upsert(
    content: &str,
    key: &str,
    value: &serde_json::Value,
) -> Result<String> {
    let mut v: serde_json::Value = if content.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(content)?
    };
    let obj = v
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("JSON 根不是对象，无法 upsert 字段 {}", key))?;
    obj.insert(key.to_string(), value.clone());
    Ok(serde_json::to_string_pretty(&v)?)
}

// —— 辅助 ——

fn parse_value(s: &str) -> serde_json::Value {
    if let Ok(n) = s.parse::<i64>() {
        return serde_json::json!(n);
    }
    if let Ok(f) = s.parse::<f64>() {
        return serde_json::json!(f);
    }
    if s == "true" {
        return serde_json::json!(true);
    }
    if s == "false" {
        return serde_json::json!(false);
    }
    serde_json::json!(s)
}

fn value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_postgresql_conf_as_ini() {
        // 无论传入相对文件名还是拼接 install_path 后的完整路径，都应路由到 Ini
        assert_eq!(detect_format(Path::new("postgresql.conf")), ConfigFormat::Ini);
        assert_eq!(detect_format(Path::new("D:/pg/data/postgresql.conf")), ConfigFormat::Ini);
    }

    #[test]
    fn detect_redis_conf_remains_keyvalue() {
        assert_eq!(detect_format(Path::new("redis.conf")), ConfigFormat::KeyValue);
    }

    #[test]
    fn ini_reads_pg_equals_style_and_writes_compatible() {
        // PG 配置 `key = value`（等号两侧带空格）能被 Ini 读器正确解析
        let content = "# Connection Settings\nport = 5432\nlisten_addresses = 'localhost'\n";
        assert_eq!(ini_lookup(content, None, "port"), serde_json::json!(5432));
        assert_eq!(
            ini_lookup(content, None, "listen_addresses"),
            serde_json::json!("'localhost'")
        );
        // 写回产出 `port=15432`（无空格），PG 的解析器同时接受两种写法
        let out = ini_upsert(content, None, "port", &serde_json::json!(15432)).unwrap();
        assert!(out.contains("port=15432"), "写入后应含 port=15432, got:\n{}", out);
        assert_eq!(ini_lookup(&out, None, "port"), serde_json::json!(15432));
    }
}
