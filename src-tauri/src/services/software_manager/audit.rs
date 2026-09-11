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
}
