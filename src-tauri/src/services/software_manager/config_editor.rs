use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::models::software::{ConfigField, ConfigSchema};

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
                lines[i] = format!("{} {}", key, v_str);
                found = true;
                break;
            }
        }
    }
    if !found {
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
                lines[i] = format!("{} {};", key, v_str);
                found = true;
                break;
            }
        }
    }
    if !found {
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
    use crate::models::software::{ConfigField, ConfigFieldType, ConfigSchema};

    fn schema_with(field_key: &str, section: Option<&str>) -> ConfigSchema {
        ConfigSchema {
            fields: vec![ConfigField {
                key: field_key.to_string(),
                label_i18n: "test".to_string(),
                field_type: ConfigFieldType::Text,
                default_value: serde_json::json!(""),
                section: section.map(|s| s.to_string()),
                description_i18n: None,
            }],
        }
    }

    // —— detect_format ——

    #[test]
    fn detect_format_ini_for_my_ini() {
        assert_eq!(detect_format(&PathBuf::from("my.ini")), ConfigFormat::Ini);
    }

    #[test]
    fn detect_format_ini_for_my_cnf() {
        // MySQL Linux 下常用 my.cnf（INI 格式）
        assert_eq!(detect_format(&PathBuf::from("my.cnf")), ConfigFormat::Ini);
    }

    #[test]
    fn detect_format_keyvalue_for_redis_conf() {
        assert_eq!(
            detect_format(&PathBuf::from("redis.conf")),
            ConfigFormat::KeyValue
        );
    }

    #[test]
    fn detect_format_nginx_for_nginx_conf() {
        assert_eq!(
            detect_format(&PathBuf::from("nginx.conf")),
            ConfigFormat::NginxConf
        );
    }

    #[test]
    fn detect_format_json_for_json() {
        assert_eq!(
            detect_format(&PathBuf::from("config.json")),
            ConfigFormat::Json
        );
    }

    #[test]
    fn detect_format_plaintext_for_unknown() {
        assert_eq!(
            detect_format(&PathBuf::from("config.cfg")),
            ConfigFormat::Plaintext
        );
    }

    // —— INI 解析 ——

    #[test]
    fn ini_lookup_finds_port_in_mysqld_section() {
        let content = "[mysql]\ndefault-character-set=utf8mb4\n\n[mysqld]\nport=3306\nmax_connections=151\n";
        let schema = schema_with("port", Some("[mysqld]"));
        let form = read_form_from_string(content, &schema, ConfigFormat::Ini);
        assert_eq!(form.get("port").unwrap(), &serde_json::json!(3306));
    }

    #[test]
    fn ini_lookup_returns_null_for_missing_key() {
        let content = "[mysqld]\nport=3306\n";
        let schema = schema_with("max_connections", Some("[mysqld]"));
        let form = read_form_from_string(content, &schema, ConfigFormat::Ini);
        assert_eq!(form.get("max_connections").unwrap(), &serde_json::Value::Null);
    }

    #[test]
    fn ini_upsert_replaces_existing_key() {
        let content = "[mysqld]\nport=3306\nmax_connections=151\n";
        let schema = schema_with("port", Some("[mysqld]"));
        let mut form = FormData::new();
        form.insert("port".to_string(), serde_json::json!(3307));
        let new_content = write_form_to_string(content, &schema, &form, ConfigFormat::Ini);
        assert!(new_content.contains("port=3307"));
        assert!(new_content.contains("max_connections=151"));
    }

    #[test]
    fn ini_upsert_adds_missing_key_to_section() {
        let content = "[mysqld]\nport=3306\n";
        let schema = schema_with("max_connections", Some("[mysqld]"));
        let mut form = FormData::new();
        form.insert("max_connections".to_string(), serde_json::json!(200));
        let new_content = write_form_to_string(content, &schema, &form, ConfigFormat::Ini);
        assert!(new_content.contains("max_connections=200"));
    }

    #[test]
    fn ini_upsert_creates_section_if_not_exists() {
        let content = "[mysql]\ndefault-character-set=utf8mb4\n";
        let schema = schema_with("port", Some("[mysqld]"));
        let mut form = FormData::new();
        form.insert("port".to_string(), serde_json::json!(3306));
        let new_content = write_form_to_string(content, &schema, &form, ConfigFormat::Ini);
        assert!(new_content.contains("[mysqld]"));
        assert!(new_content.contains("port=3306"));
    }

    // —— KeyValue 解析 ——

    #[test]
    fn kv_lookup_finds_port_in_redis_conf() {
        let content = "port 6379\nbind 127.0.0.1\nmaxmemory 256mb\n";
        let schema = schema_with("port", None);
        let form = read_form_from_string(content, &schema, ConfigFormat::KeyValue);
        assert_eq!(form.get("port").unwrap(), &serde_json::json!(6379));
    }

    #[test]
    fn kv_upsert_replaces_existing_key() {
        let content = "port 6379\nbind 127.0.0.1\n";
        let schema = schema_with("port", None);
        let mut form = FormData::new();
        form.insert("port".to_string(), serde_json::json!(6380));
        let new_content = write_form_to_string(content, &schema, &form, ConfigFormat::KeyValue);
        assert!(new_content.contains("port 6380"));
        assert!(new_content.contains("bind 127.0.0.1"));
    }

    #[test]
    fn kv_upsert_appends_missing_key() {
        let content = "port 6379\n";
        let schema = schema_with("maxmemory", None);
        let mut form = FormData::new();
        form.insert("maxmemory".to_string(), serde_json::json!("512mb"));
        let new_content = write_form_to_string(content, &schema, &form, ConfigFormat::KeyValue);
        assert!(new_content.contains("maxmemory 512mb"));
    }

    // —— NginxConf 解析 ——

    #[test]
    fn nginx_lookup_finds_listen() {
        let content = "worker_processes 4;\nlisten 80;\nroot html;\n";
        let schema = schema_with("listen", None);
        let form = read_form_from_string(content, &schema, ConfigFormat::NginxConf);
        assert_eq!(form.get("listen").unwrap(), &serde_json::json!(80));
    }

    #[test]
    fn nginx_upsert_replaces_listen() {
        let content = "listen 80;\nroot html;\n";
        let schema = schema_with("listen", None);
        let mut form = FormData::new();
        form.insert("listen".to_string(), serde_json::json!(8080));
        let new_content = write_form_to_string(content, &schema, &form, ConfigFormat::NginxConf);
        assert!(new_content.contains("listen 8080;"));
        assert!(new_content.contains("root html;"));
    }

    #[test]
    fn nginx_upsert_appends_missing_key() {
        let content = "listen 80;\n";
        let schema = schema_with("root", None);
        let mut form = FormData::new();
        form.insert("root".to_string(), serde_json::json!("html"));
        let new_content = write_form_to_string(content, &schema, &form, ConfigFormat::NginxConf);
        assert!(new_content.contains("root html;"));
    }

    // —— JSON 解析 ——

    #[test]
    fn json_lookup_finds_key() {
        let content = r#"{"port": 3306, "host": "127.0.0.1"}"#;
        let schema = schema_with("port", None);
        let form = read_form_from_string(content, &schema, ConfigFormat::Json);
        assert_eq!(form.get("port").unwrap(), &serde_json::json!(3306));
    }

    #[test]
    fn json_upsert_replaces_key() {
        let content = r#"{"port": 3306}"#;
        let schema = schema_with("port", None);
        let mut form = FormData::new();
        form.insert("port".to_string(), serde_json::json!(3307));
        let new_content = write_form_to_string(content, &schema, &form, ConfigFormat::Json);
        assert!(new_content.contains("\"port\": 3307"));
    }

    #[test]
    fn json_upsert_returns_err_for_non_object_root() {
        let content = r#"[1, 2, 3]"#; // 数组根
        let result = json_upsert(content, "port", &serde_json::json!(3306));
        assert!(result.is_err());
        let err = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err.contains("不是对象"), "错误应含'不是对象'，实际：{}", err);
    }

    // —— parse_value / value_to_string ——

    #[test]
    fn parse_value_handles_integers() {
        assert_eq!(parse_value("3306"), serde_json::json!(3306));
    }

    #[test]
    fn parse_value_handles_floats() {
        assert_eq!(parse_value("3.14"), serde_json::json!(3.14));
    }

    #[test]
    fn parse_value_handles_booleans() {
        assert_eq!(parse_value("true"), serde_json::json!(true));
        assert_eq!(parse_value("false"), serde_json::json!(false));
    }

    #[test]
    fn parse_value_handles_strings() {
        assert_eq!(parse_value("utf8mb4"), serde_json::json!("utf8mb4"));
        assert_eq!(parse_value("128M"), serde_json::json!("128M"));
    }

    #[test]
    fn value_to_string_handles_all_types() {
        assert_eq!(value_to_string(&serde_json::json!("test")), "test");
        assert_eq!(value_to_string(&serde_json::json!(3306)), "3306");
        assert_eq!(value_to_string(&serde_json::json!(true)), "true");
        assert_eq!(value_to_string(&serde_json::Value::Null), "");
    }

    // —— backup_config ——

    #[test]
    fn backup_creates_backup_file_in_backups_subdir() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg_path = tmp.path().join("my.ini");
        std::fs::write(&cfg_path, "content").unwrap();

        let backup_path = backup_config(&cfg_path).unwrap();
        assert!(backup_path.exists());
        assert!(backup_path.to_string_lossy().contains("backups"));
    }

    #[test]
    fn backup_keeps_latest_5_backups() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg_path = tmp.path().join("my.ini");
        std::fs::write(&cfg_path, "v1").unwrap();

        let backup_dir = tmp.path().join("backups");
        std::fs::create_dir_all(&backup_dir).unwrap();
        // 预创建 5 个旧备份（须以 _my.ini 结尾才会被 cleanup 处理）
        for i in 0..5 {
            std::fs::write(
                backup_dir.join(format!("2026010{}_my.ini", i)),
                "old",
            )
            .unwrap();
        }

        backup_config(&cfg_path).unwrap();
        let count = std::fs::read_dir(&backup_dir).unwrap().count();
        assert!(count <= 5, "应保留最多 5 个备份，实际 {}", count);
    }

    #[test]
    fn cleanup_does_not_touch_other_files_backups() {
        // 同目录下 my.ini 与 redis.conf 的备份不应互相误删
        let tmp = tempfile::tempdir().unwrap();
        let backup_dir = tmp.path().join("backups");
        std::fs::create_dir_all(&backup_dir).unwrap();

        // 预创建 6 个 my.ini 备份 + 3 个 redis.conf 备份
        for i in 0..6 {
            std::fs::write(backup_dir.join(format!("2026010{}_my.ini", i)), "old").unwrap();
        }
        for i in 0..3 {
            std::fs::write(backup_dir.join(format!("2026010{}_redis.conf", i)), "old").unwrap();
        }

        // cleanup my.ini，保留 5 个
        cleanup_old_backups(&backup_dir, "my.ini", 5).unwrap();

        let entries: Vec<_> = std::fs::read_dir(&backup_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        let my_count = entries
            .iter()
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .map(|n| n.ends_with("_my.ini"))
                    .unwrap_or(false)
            })
            .count();
        let redis_count = entries
            .iter()
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .map(|n| n.ends_with("_redis.conf"))
                    .unwrap_or(false)
            })
            .count();

        assert_eq!(my_count, 5, "my.ini 备份应保留 5 个");
        assert_eq!(redis_count, 3, "redis.conf 备份不应被误删，应保留 3 个");
    }

    #[test]
    fn backup_handles_nonexistent_file() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg_path = tmp.path().join("nonexistent.ini");
        let result = backup_config(&cfg_path);
        // 不存在文件不应 panic，backup_config 返回 Ok（不复制，仅创建目录）
        assert!(result.is_ok());
    }

    // —— read/write source ——

    #[test]
    fn read_config_source_returns_file_content() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("my.ini");
        std::fs::write(&path, "test content").unwrap();
        let content = read_config_source(&path).unwrap();
        assert_eq!(content, "test content");
    }

    #[test]
    fn write_config_source_writes_and_backs_up() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("my.ini");
        std::fs::write(&path, "old content").unwrap();

        write_config_source(&path, "new content").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new content");
        // 备份应存在
        let backup_dir = tmp.path().join("backups");
        assert!(backup_dir.exists());
    }

    // —— 测试辅助函数 ——

    fn read_form_from_string(
        content: &str,
        schema: &ConfigSchema,
        format: ConfigFormat,
    ) -> FormData {
        let mut form = FormData::new();
        for field in &schema.fields {
            let value = match format {
                ConfigFormat::Ini => ini_lookup(content, field.section.as_deref(), &field.key),
                ConfigFormat::KeyValue => kv_lookup(content, &field.key),
                ConfigFormat::NginxConf => nginx_lookup(content, &field.key),
                ConfigFormat::Json => json_lookup(content, &field.key),
                ConfigFormat::Plaintext => serde_json::Value::Null,
            };
            form.insert(field.key.clone(), value);
        }
        form
    }

    fn write_form_to_string(
        content: &str,
        schema: &ConfigSchema,
        form: &FormData,
        format: ConfigFormat,
    ) -> String {
        let mut new_content = content.to_string();
        for (key, value) in form {
            let field = schema.fields.iter().find(|f| &f.key == key);
            if let Some(field) = field {
                new_content = match format {
                    ConfigFormat::Ini => {
                        ini_upsert(&new_content, field.section.as_deref(), key, value)
                            .unwrap()
                    }
                    ConfigFormat::KeyValue => kv_upsert(&new_content, key, value).unwrap(),
                    ConfigFormat::NginxConf => nginx_upsert(&new_content, key, value).unwrap(),
                    ConfigFormat::Json => json_upsert(&new_content, key, value).unwrap(),
                    ConfigFormat::Plaintext => new_content,
                };
            }
        }
        new_content
    }
}
