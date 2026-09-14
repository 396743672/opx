//! 操作审计存储：每条操作一行 JSON 追加到 logs/audit-YYYY-MM-DD.jsonl。
//! 与普通运行日志分离，供「操作记录」页查询/统计/导出。

use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::utils::paths;

/// 单条操作记录。一期不含结果字段；二期新增 result/error 时须带 #[serde(default)]。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEntry {
    pub ts: String,
    pub action: String,
    pub target: String,
    #[serde(default)]
    pub detail: String,
}

/// 审计文件路径：<logs_dir>/audit-YYYY-MM-DD.jsonl
fn file_for(date: chrono::NaiveDate) -> PathBuf {
    paths::logs_dir().join(format!("audit-{}.jsonl", date.format("%Y-%m-%d")))
}

/// 追加一条操作记录（同步写盘：保证 quit/exit 等退出前调用不丢尾部）。
pub fn record(action: &str, target: &str, detail: &str) {
    // 串行化同进程内的追加：O_APPEND 不保证进程内多线程并发写的原子性，
    // 不加锁时并发调用会交错截断 JSONL 行。
    static WRITE_LOCK: std::sync::LazyLock<std::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| std::sync::Mutex::new(()));
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let entry = AuditEntry {
        ts: chrono::Local::now().to_rfc3339(),
        action: action.to_string(),
        target: target.to_string(),
        detail: detail.to_string(),
    };
    let path = file_for(chrono::Local::now().date_naive());
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(line) = serde_json::to_string(&entry) {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "{}", line);
        }
    }
}

/// 查询结果（entries 按 ts 倒序）
#[derive(Debug, Clone, Serialize)]
pub struct AuditQuery {
    pub entries: Vec<AuditEntry>,
    /// 过滤后的总条数（不受 limit/offset 影响），供分页展示
    pub total: u64,
    /// 是否还有下一页（offset + limit < total）
    pub truncated: bool,
}

/// 读取最近 days 天的全部条目（未过滤、未排序）。无法解析的行跳过，不 panic。
fn read_days(days: u64) -> Vec<AuditEntry> {
    let today = chrono::Local::now().date_naive();
    let mut out = Vec::new();
    for i in 0..days.max(1) as i64 {
        let path = file_for(today - chrono::Duration::days(i));
        let Ok(content) = std::fs::read_to_string(&path) else { continue };
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(e) = serde_json::from_str::<AuditEntry>(line) {
                out.push(e);
            }
        }
    }
    out
}

fn matches_filters(e: &AuditEntry, action: Option<&str>, keyword: Option<&str>) -> bool {
    if let Some(a) = action {
        if !a.is_empty() && e.action != a {
            return false;
        }
    }
    if let Some(k) = keyword {
        let k = k.trim().to_lowercase();
        if !k.is_empty() {
            let haystack = format!("{} {} {}", e.action, e.target, e.detail).to_lowercase();
            if !haystack.contains(&k) {
                return false;
            }
        }
    }
    true
}

/// 查询：天范围 + action 精确 + keyword 子串（大小写不敏感），ts 倒序，最多 limit 条。
pub fn query(
    days: u64,
    action: Option<&str>,
    keyword: Option<&str>,
    limit: usize,
    offset: usize,
) -> AuditQuery {
    let mut entries: Vec<AuditEntry> = read_days(days)
        .into_iter()
        .filter(|e| matches_filters(e, action, keyword))
        .collect();
    // ts 为本地时区 RFC3339，同偏移下字符串序即时间序
    entries.sort_by(|a, b| b.ts.cmp(&a.ts));
    let total = entries.len();
    let truncated = offset.saturating_add(limit) < total;
    let entries = entries.into_iter().skip(offset).take(limit).collect();
    AuditQuery {
        entries,
        total: total as u64,
        truncated,
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ActionCount {
    pub action: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditStats {
    pub today: u64,
    pub total: u64,
    pub by_action: Vec<ActionCount>,
    pub last_ts: Option<String>,
}

/// 统计最近 days 天：今日条数、总数、按 action 计数（降序，同数按 action 升序）、最近时间。
pub fn stats(days: u64) -> AuditStats {
    let all = read_days(days);
    let today_prefix = chrono::Local::now().date_naive().format("%Y-%m-%d").to_string();
    let today = all.iter().filter(|e| e.ts.starts_with(&today_prefix)).count() as u64;

    let mut counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for e in &all {
        *counts.entry(e.action.clone()).or_insert(0) += 1;
    }
    let mut by_action: Vec<ActionCount> = counts
        .into_iter()
        .map(|(action, count)| ActionCount { action, count })
        .collect();
    by_action.sort_by(|a, b| b.count.cmp(&a.count).then(a.action.cmp(&b.action)));

    let last_ts = all.iter().map(|e| e.ts.clone()).max();
    AuditStats {
        today,
        total: all.len() as u64,
        by_action,
        last_ts,
    }
}

/// CSV 字段转义：含逗号/引号/换行时用双引号包裹，内部引号翻倍。
fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// 导出用时间格式：RFC3339 → `YYYY-MM-DD HH:MM:SS`（保留原始时区偏移，不做换算）。
/// 无法解析时原样返回。
fn fmt_ts(ts: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(ts)
        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|_| ts.to_string())
}

/// 按过滤条件导出 CSV（UTF-8 BOM，便于 Excel 正确识别中文）到 dest_path。
pub fn export_csv(
    days: u64,
    action: Option<&str>,
    keyword: Option<&str>,
    dest_path: &str,
) -> Result<(), String> {
    let q = query(days, action, keyword, usize::MAX, 0);
    let mut out = String::from("\u{feff}时间,操作,目标,详情\n");
    for e in q.entries {
        out.push_str(&format!(
            "{},{},{},{}\n",
            csv_field(&fmt_ts(&e.ts)),
            csv_field(&e.action),
            csv_field(&e.target),
            csv_field(&e.detail)
        ));
    }
    std::fs::write(dest_path, out).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 唯一标记，避免测试与真实记录/其他测试互相干扰。
    fn uniq() -> String {
        format!("__qa_{}", chrono::Local::now().timestamp_nanos_opt().unwrap_or(0))
    }

    #[test]
    fn record_appends_parseable_entry() {
        let u = uniq();
        record("__qa_record", &format!("{u}_target"), &format!("{u}_detail"));
        let path = file_for(chrono::Local::now().date_naive());
        let content = std::fs::read_to_string(&path).expect("audit file exists");
        let hit = content
            .lines()
            .filter_map(|l| serde_json::from_str::<AuditEntry>(l).ok())
            .find(|e| e.action == "__qa_record" && e.target == format!("{u}_target"));
        let hit = hit.expect("entry appended");
        assert_eq!(hit.detail, format!("{u}_detail"));
        assert!(!hit.ts.is_empty());
    }

    #[test]
    fn query_filters_by_action_and_keyword() {
        let u = uniq();
        record("__qa_q_action", &format!("{u}_target"), "");
        record("__qa_q_other", &format!("{u}_target"), "");
        let q = query(1, Some("__qa_q_action"), Some(&u), 100, 0);
        assert_eq!(q.entries.len(), 1);
        assert_eq!(q.entries[0].action, "__qa_q_action");
        assert!(!q.truncated);
    }

    #[test]
    fn query_is_case_insensitive_on_keyword() {
        let u = uniq();
        record("__qa_q_case", &format!("{u}_MixedCase"), "");
        assert_eq!(query(1, Some("__qa_q_case"), Some(&u.to_lowercase()), 100, 0).entries.len(), 1);
        // 用带唯一标记的大写形式验证大小写不敏感，避免匹配历史遗留行
        let upper = format!("{u}_MIXEDCASE");
        assert_eq!(query(1, Some("__qa_q_case"), Some(&upper), 100, 0).entries.len(), 1);
    }

    #[test]
    fn query_marks_truncated_when_over_limit() {
        let u = uniq();
        for i in 0..3 {
            record("__qa_q_trunc", &format!("{u}_{i}"), "");
        }
        let q = query(1, Some("__qa_q_trunc"), Some(&u), 2, 0);
        assert_eq!(q.entries.len(), 2);
        assert!(q.truncated);
    }

    #[test]
    fn query_pages_with_total_and_next_flag() {
        let u = uniq();
        for i in 0..5 {
            record("__qa_page", &format!("{u}_{i}"), "");
        }
        let p1 = query(1, Some("__qa_page"), Some(&u), 2, 0);
        assert_eq!(p1.entries.len(), 2, "首页取 2 条");
        assert_eq!(p1.total, 5, "total 为过滤后总数，与分页无关");
        assert!(p1.truncated, "还有下一页");

        let p3 = query(1, Some("__qa_page"), Some(&u), 2, 4);
        assert_eq!(p3.entries.len(), 1, "末页只剩 1 条");
        assert!(!p3.truncated, "末页不应标记还有下一页");

        // 越界 offset 返回空且无下一页
        let p4 = query(1, Some("__qa_page"), Some(&u), 2, 99);
        assert!(p4.entries.is_empty());
        assert!(!p4.truncated);
    }

    #[test]
    fn stats_counts_today_total_and_by_action() {
        let u = uniq();
        record("__qa_stats", &u, "");
        record("__qa_stats", &u, "");

        // 断言本次运行写入的两条可被统计到（用唯一 target 隔离历史行）
        let mine = read_days(1)
            .into_iter()
            .filter(|e| e.action == "__qa_stats" && e.target == u)
            .count();
        assert_eq!(mine, 2, "本次写入的两条应可被 read_days 统计到");

        let s = stats(1);
        assert!(s.total >= 2);
        assert!(s.today >= 2);
        let c = s
            .by_action
            .iter()
            .find(|c| c.action == "__qa_stats")
            .expect("action counted");
        assert!(c.count >= 2);
        assert!(s.last_ts.is_some());
    }

    #[test]
    fn csv_field_escapes_special_chars() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("he said \"hi\""), "\"he said \"\"hi\"\"\"");
        assert_eq!(csv_field("l1\nl2"), "\"l1\nl2\"");
    }

    #[test]
    fn fmt_ts_renders_standard_format_and_falls_back() {
        // 保留原始时区偏移，不做换算
        assert_eq!(fmt_ts("2026-09-11T12:39:28.558596100+08:00"), "2026-09-11 12:39:28");
        assert_eq!(fmt_ts("2026-09-11T04:00:00Z"), "2026-09-11 04:00:00");
        // 不可解析时原样返回
        assert_eq!(fmt_ts("not-a-timestamp"), "not-a-timestamp");
    }

    #[test]
    fn export_csv_writes_header_and_rows() {
        let u = uniq();
        record("__qa_csv", &format!("{u},comma"), "");
        let dest = paths::logs_dir().join(format!("{u}.csv"));
        export_csv(1, Some("__qa_csv"), Some(&u), dest.to_str().unwrap()).unwrap();
        let content = std::fs::read_to_string(&dest).unwrap();
        assert!(content.starts_with('\u{feff}'), "BOM for Excel");
        assert!(content.contains("时间,操作,目标,详情"));
        assert!(content.contains("__qa_csv"));
        let _ = std::fs::remove_file(&dest);
    }
}
