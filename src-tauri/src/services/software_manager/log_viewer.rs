//! 日志查看器后端服务（C 扩展 · 日志查看器）
//!
//! 负责：列出实例的日志来源、按 offset/limit 读取日志（tail / 增量 / 历史分页）、
//! 以及把日志下载到用户指定路径。读取时支持关键字 / 正则 / 级别过滤。
//!
//! 设计要点（见 design 文档 §3.3）：
//! - `offset == None` → tail 模式，返回末尾 `limit`（默认 2000）行，end_offset = total_bytes；
//! - `offset == Some(o), before == false` → 增量模式，从字节 `o` 向前（朝 EOF）读取新行；
//! - `offset == Some(o), before == true` → 历史模式，读取字节 `o` 之前（朝文件头）的 `limit` 行。

use std::io::{BufRead, Read, Seek, SeekFrom};
use std::path::Path;

use crate::models::software::{LogChunk, LogSource};
use crate::services::software_manager::providers::{all_providers, LogContext};
use crate::services::software_manager::SoftwareManager;

/// 级别过滤用的内置默认正则（决策 6 / 共享知识 §6.6）
const DEFAULT_LEVEL_REGEX: &str =
    r"(?i)\b(ERROR|ERR|WARN|WARNING|INFO|DEBUG|TRACE|FATAL|CRITICAL|PANIC|NOTICE)\b";

/// 行过滤器：关键字 / 正则 / 级别三重过滤
struct LineFilter<'a> {
    keyword: Option<&'a str>,
    regex: Option<regex::Regex>,
    level: Option<String>,
    level_regex: Option<regex::Regex>,
}

impl<'a> LineFilter<'a> {
    fn is_active(&self) -> bool {
        self.keyword.is_some() || self.regex.is_some() || self.level.is_some()
    }

    fn matches(&self, line: &str) -> bool {
        if let Some(kw) = self.keyword {
            if !line.contains(kw) {
                return false;
            }
        }
        if let Some(re) = &self.regex {
            if !re.is_match(line) {
                return false;
            }
        }
        if let Some(lv) = &self.level {
            match &self.level_regex {
                Some(lre) => match lre.captures(line) {
                    Some(c) => {
                        let captured = c.get(1).map(|m| m.as_str()).unwrap_or("");
                        if !captured.eq_ignore_ascii_case(lv) {
                            return false;
                        }
                    }
                    None => return false,
                },
                None => {
                    if !line.to_uppercase().contains(&lv.to_uppercase()) {
                        return false;
                    }
                }
            }
        }
        true
    }
}

/// 列出某实例的日志来源（取 provider.log_sources）
pub fn list_log_sources(
    manager: &SoftwareManager,
    installed_id: &str,
) -> anyhow::Result<Vec<LogSource>> {
    let sw = manager
        .find_installed(installed_id)
        .ok_or_else(|| anyhow::anyhow!("未找到安装记录: {}", installed_id))?;
    let providers = all_providers();
    let provider = providers
        .iter()
        .find(|p| p.key() == sw.key)
        .ok_or_else(|| anyhow::anyhow!("未找到 provider: {}", sw.key))?;
    let ctx = LogContext {
        installed_id: sw.id.clone(),
        install_path: sw.install_path.clone(),
        version: sw.version.clone(),
        config: sw.config.clone(),
        pid: sw.pid,
    };
    Ok(provider.log_sources(&ctx))
}

/// 读取日志（按来源索引 + offset 模式）
#[allow(clippy::too_many_arguments)]
pub fn read_log(
    manager: &SoftwareManager,
    installed_id: &str,
    source_index: usize,
    offset: Option<u64>,
    before: bool,
    limit: usize,
    keyword: Option<&str>,
    regex: bool,
    level: Option<&str>,
) -> anyhow::Result<LogChunk> {
    let sources = list_log_sources(manager, installed_id)?;
    let source = sources
        .get(source_index)
        .ok_or_else(|| anyhow::anyhow!("日志源索引越界: {}", source_index))?;
    let path = Path::new(&source.path);
    if !path.exists() {
        return Ok(LogChunk {
            lines: vec![],
            start_offset: 0,
            end_offset: 0,
            total_bytes: 0,
            has_more: false,
            truncated: false,
        });
    }
    let total = std::fs::metadata(path)?.len();

    let kw = keyword.filter(|s| !s.is_empty());
    let re_filter = if regex {
        kw.map(|k| regex::Regex::new(k).map_err(|e| anyhow::anyhow!("正则编译失败: {}", e)))
            .transpose()?
    } else {
        None
    };
    let lv = level
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let level_re = if lv.is_some() {
        let pattern = source
            .level_pattern
            .clone()
            .unwrap_or_else(|| DEFAULT_LEVEL_REGEX.to_string());
        Some(
            regex::Regex::new(&pattern)
                .map_err(|e| anyhow::anyhow!("级别正则编译失败: {}", e))?,
        )
    } else {
        None
    };
    let filter = LineFilter {
        keyword: kw,
        regex: re_filter,
        level: lv,
        level_regex: level_re,
    };

    match offset {
        None => read_backward_filtered(path, total, total, limit, &filter),
        Some(o) if before => read_backward_filtered(path, total, o, limit, &filter),
        Some(o) => read_since(path, total, o, limit, &filter),
    }
}

/// 把日志下载（拷贝）到用户指定路径
pub fn download_log(source_path: &str, dest_path: &str) -> anyhow::Result<()> {
    let src = Path::new(source_path);
    if !src.exists() {
        return Err(anyhow::anyhow!("日志文件不存在: {}", source_path));
    }
    std::fs::copy(src, Path::new(dest_path))
        .map_err(|e| anyhow::anyhow!("复制日志失败 {} -> {}: {}", source_path, dest_path, e))?;
    Ok(())
}

/// 从 `from_byte` 向前（朝文件头）回溯，收集末尾 `limit` 个「匹配」行。
/// 用于 tail（from_byte = total）与历史分页（from_byte = 当前首行偏移）。
fn read_backward_filtered(
    path: &Path,
    total: u64,
    from_byte: u64,
    limit: usize,
    filter: &LineFilter,
) -> anyhow::Result<LogChunk> {
    let mut file = std::fs::File::open(path)?;
    let mut remaining = from_byte.min(total);
    let chunk_size = 8192u64;
    // 从后往前读块，累积原始字节（块列表逆序后重组为正序）
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    let is_filtering = filter.is_active();
    while remaining > 0 {
        let take = remaining.min(chunk_size);
        let start = remaining - take;
        let mut buf = vec![0u8; take as usize];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut buf)?;
        blocks.push(buf);
        remaining = start;
        // 估算已收集原始行数（数换行）。未过滤时收够即停；过滤时多读一些以提高命中率。
        let collected_newlines: usize = blocks
            .iter()
            .map(|b| b.iter().filter(|&&x| x == b'\n').count())
            .sum();
        if !is_filtering && collected_newlines >= limit {
            break;
        }
        if is_filtering && collected_newlines >= limit * 8 {
            break;
        }
    }
    let mut all = Vec::new();
    for b in blocks.into_iter().rev() {
        all.extend_from_slice(&b);
    }
    let content_start = from_byte - all.len() as u64;
    let text = String::from_utf8_lossy(&all);
    let lines: Vec<&str> = text.split('\n').collect();

    // 收集匹配行及其在文件中的字节偏移（相对文件整体）
    let mut matched: Vec<(u64, &str)> = Vec::new();
    let mut byte_pos = content_start;
    for line in lines {
        let line_len = line.len();
        if filter.matches(line) {
            matched.push((byte_pos, line));
        }
        let has_nl = (byte_pos + line_len as u64) < total;
        byte_pos += line_len as u64 + if has_nl { 1 } else { 0 };
    }

    let truncated = matched.len() > limit;
    let selected = if matched.len() > limit {
        &matched[matched.len() - limit..]
    } else {
        &matched[..]
    };
    let start_offset = selected.first().map(|(o, _)| *o).unwrap_or(content_start);
    let lines_out: Vec<String> = selected.iter().map(|(_, l)| l.to_string()).collect();
    let has_more = content_start > 0;

    Ok(LogChunk {
        lines: lines_out,
        start_offset,
        end_offset: from_byte,
        total_bytes: total,
        has_more,
        truncated,
    })
}

/// 从字节 `from_byte` 向前（朝 EOF）读取匹配行，直到 EOF 或收满 `limit` 行。
/// 用于实时增量轮询（前端携带上次的 end_offset）。
fn read_since(
    path: &Path,
    total: u64,
    from_byte: u64,
    limit: usize,
    filter: &LineFilter,
) -> anyhow::Result<LogChunk> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(from_byte))?;
    let mut reader = std::io::BufReader::new(file);
    let mut lines: Vec<String> = Vec::new();
    let mut end_offset = from_byte;
    let mut truncated = false;
    loop {
        let _start = reader.stream_position()?;
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break; // EOF
        }
        let end = reader.stream_position()?;
        if filter.matches(&line) {
            if lines.len() >= limit {
                truncated = true;
                end_offset = end;
                break;
            }
            lines.push(line);
            end_offset = end;
        }
        if end >= total {
            break;
        }
    }
    Ok(LogChunk {
        lines,
        start_offset: from_byte,
        end_offset,
        total_bytes: total,
        has_more: from_byte > 0,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn unique_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    fn tmp_file(prefix: &str, content: &[u8]) -> std::path::PathBuf {
        let p = std::env::temp_dir()
            .join(format!("opx_qa_lv_{}_{}.log", prefix, unique_suffix()));
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(content).unwrap();
        p
    }

    #[test]
    fn test_line_filter_keyword() {
        let f = LineFilter {
            keyword: Some("err"),
            regex: None,
            level: None,
            level_regex: None,
        };
        assert!(f.matches("this is an error"));
        assert!(!f.matches("all good"));
    }

    #[test]
    fn test_line_filter_regex() {
        let re = regex::Regex::new("ERR|WARN").unwrap();
        let f = LineFilter {
            keyword: None,
            regex: Some(re),
            level: None,
            level_regex: None,
        };
        assert!(f.matches("line WARN here"));
        assert!(!f.matches("line INFO here"));
    }

    #[test]
    fn test_line_filter_level_default_regex() {
        let re = regex::Regex::new(DEFAULT_LEVEL_REGEX).unwrap();
        let f = LineFilter {
            keyword: None,
            regex: None,
            level: Some("ERROR".to_string()),
            level_regex: Some(re),
        };
        assert!(f.matches("2024-01-01 ERROR boom"));
        assert!(!f.matches("2024-01-01 INFO ok"));
        // 默认级别正则可不区分大小写匹配
        assert!(f.matches("2024-01-01 error lowercase"));
    }

    #[test]
    fn test_read_since_basic() {
        let p = tmp_file("since", b"a\nb\nc\n");
        let total = fs::metadata(&p).unwrap().len();
        let filter = LineFilter {
            keyword: None,
            regex: None,
            level: None,
            level_regex: None,
        };
        let chunk = read_since(&p, total, 0, 100, &filter).unwrap();
        assert_eq!(chunk.lines.len(), 3);
        assert_eq!(chunk.start_offset, 0);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn test_read_backward_tail_basic() {
        let content = b"L0\nL1\nL2\nL3\nL4\n";
        let p = tmp_file("tail", content);
        let total = fs::metadata(&p).unwrap().len();
        let filter = LineFilter {
            keyword: None,
            regex: None,
            level: None,
            level_regex: None,
        };
        let chunk = read_backward_filtered(&p, total, total, 100, &filter).unwrap();
        assert_eq!(chunk.lines.iter().filter(|l| !l.is_empty()).count(), 5);
        assert_eq!(chunk.start_offset, 0);
        let _ = fs::remove_file(&p);
    }

    /// 历史模式（before=true）：from_byte 之前的内容，start_offset 应等于该内容首字节真实偏移。
    /// 当前实现 content_start = total - all.len()，在 before 模式（from_byte < total）下
    /// 会偏大 (total - from_byte)，导致分页偏移错位 => 该断言在修复前会失败。
    #[test]
    fn test_read_backward_before_mode_offset() {
        let content = b"L0\nL1\nL2\nL3\nL4\n"; // 每行 3 字节，total = 15
        let p = tmp_file("before", content);
        let total = fs::metadata(&p).unwrap().len();
        let filter = LineFilter {
            keyword: None,
            regex: None,
            level: None,
            level_regex: None,
        };
        // from_byte = 6（L2 起始），应返回 [0,6) 的 "L0","L1"，start_offset 正确应为 0
        let chunk = read_backward_filtered(&p, total, 6, 100, &filter).unwrap();
        assert_eq!(chunk.lines.iter().filter(|l| !l.is_empty()).count(), 2);
        assert_eq!(
            chunk.start_offset, 0,
            "before 模式 start_offset 计算错误（应为真实首字节偏移 0，当前实现会偏大）"
        );
        let _ = fs::remove_file(&p);
    }
}
