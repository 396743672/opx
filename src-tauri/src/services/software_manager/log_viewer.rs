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
use std::path::{Path, PathBuf};

use flate2::read::MultiGzDecoder;

use crate::models::software::{ArchiveLog, LogChunk, LogSource, LogSourceKind};
use crate::services::software_manager::providers::{all_providers, LogContext};
use crate::services::software_manager::SoftwareManager;

/// 归档扫描上限（防海量文件拖慢「加载更早」）
const MAX_ARCHIVES: usize = 40;

/// 判定文件名是否为日志滚动归档：含数字日期片段或 .gz / 数字结尾。
fn looks_like_archive(stem: &str) -> bool {
    let lower = stem.to_lowercase();
    lower.ends_with(".gz") || lower.ends_with(".log") || stem.chars().any(|c| c.is_ascii_digit())
}

/// 收集主文件的历史归档（同目录 + 一级日期子目录），时间倒序（靠字典序近似，最新在前）。
/// 匹配规则：同源派生文件名（`{base}.` / `{base}-` / `{base}_` 前缀）且满足滚动特征。
pub fn collect_archives(primary: &Path) -> Vec<ArchiveLog> {
    let Some(dir) = primary.parent() else { return vec![] };
    let Some(file_name) = primary.file_name().and_then(|s| s.to_str()) else { return vec![] };
    let stem = file_name.strip_suffix(".gz").unwrap_or(file_name);
    let base = stem.split_once('.').map(|(b, _)| b).unwrap_or(stem);
    let mut found: Vec<ArchiveLog> = Vec::new();

    let scan = |dir: &Path, found: &mut Vec<ArchiveLog>| {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_file() { continue; }
            let Some(name) = p.file_name().and_then(|s| s.to_str()) else { continue };
            if !looks_like_archive(name) { continue; }
            if name == file_name { continue; }
            let prefixes = [format!("{}.", base), format!("{}-", base), format!("{}_", base)];
            if !prefixes.iter().any(|pre| name.starts_with(pre)) { continue; }
            found.push(ArchiveLog {
                path: p.to_string_lossy().to_string(),
                label: name.to_string(),
            });
        }
    };
    scan(dir, &mut found);
    // 一级日期子目录（应用日志形态 logs/<YYYY-MM-DD>/type.date.n.log.gz）
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let d = e.path();
            if !d.is_dir() { continue; }
            scan(&d, &mut found);
        }
    }
    // 字典序倒排：框架上最新（日期更大）在前；同日前靠前
    found.sort_by(|a, b| b.path.cmp(&a.path));
    found.truncate(MAX_ARCHIVES);
    found
}

/// 整体解压 .gz（日志按天归档，单文件体积可控）
pub fn decompress_gzip(path: &Path) -> anyhow::Result<Vec<u8>> {
    let f = std::fs::File::open(path)?;
    let mut out = Vec::new();
    let mut dec = MultiGzDecoder::new(f);
    Read::read_to_end(&mut dec, &mut out)?;
    Ok(out)
}

/// 组合 trait：统一普通文件与 .gz 内存 buffer 的随机访问读取
trait ReadSeek: Read + Seek {}
impl<T: Read + Seek + ?Sized> ReadSeek for T {}

/// 打开可随机访问的日志后端：普通文件按需 seek；.gz 解压到内存后经 Cursor seek。
/// 返回 (reader, 解码后总字节数)。
fn open_reader(path: &Path) -> anyhow::Result<(Box<dyn ReadSeek>, u64)> {
    let lower = path.to_string_lossy().to_lowercase();
    if lower.ends_with(".gz") {
        let bytes = decompress_gzip(path)?;
        let len = bytes.len() as u64;
        Ok((Box::new(std::io::Cursor::new(bytes)), len))
    } else {
        let f = std::fs::File::open(path)?;
        let len = f.metadata()?.len();
        Ok((Box::new(f), len))
    }
}

/// 级别过滤用的内置默认正则（决策 6 / 共享知识 §6.6）
const DEFAULT_LEVEL_REGEX: &str =
    r"(?i)\b(ERROR|ERR|WARN|WARNING|INFO|DEBUG|TRACE|FATAL|CRITICAL|PANIC|NOTICE)\b";

/// 日志文件解码：优先 UTF-8 严格解码；失败则按 GBK 转码。
/// Windows 中文系统下 PostgreSQL 等按系统码页(GBK)输出日志，UTF-8 lossy 会得到乱码。
fn decode_log_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::GBK.decode(bytes).0.into_owned(),
    }
}

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
    archive_index: usize,
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
            archive_index: 0,
        });
    }
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
        None => {
            let (mut reader, total) = open_reader(&path)?;
            read_backward_filtered(&mut reader, total, total, limit, &filter)
        }
        Some(o) if before => read_log_archive_mode(&path, &source.archives, archive_index, o, limit, &filter),
        Some(o) => {
            let (mut reader, total) = open_reader(&path)?;
            read_since(&mut reader, total, o, limit, &filter)
        }
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

/// 为单个日志源附加历史归档（provider 复用入口）
pub fn attach_archives(mut s: LogSource) -> LogSource {
    s.archives = collect_archives(Path::new(&s.path));
    s
}

/// 按 level 拆分发现 SpringBoot 日志源：logs/*.log 各为源，归档含同目录与日期子目录。
pub fn collect_springboot_sources(logs_dir: &Path) -> Vec<LogSource> {
    let mut out: Vec<LogSource> = Vec::new();
    if !logs_dir.is_dir() {
        return out;
    }
    let Ok(rd) = std::fs::read_dir(logs_dir) else { return out };
    let mut files: Vec<_> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().map_or(false, |x| x == "log"))
        .collect();
    files.sort();
    for f in files {
        let Some(stem) = f.file_stem().and_then(|s| s.to_str()) else { continue };
        // 仅识别常见 level 文件，避免把无关 .log 也当源
        if !["debug", "info", "error", "warn", "trace", "console"].iter().any(|l| *l == stem) {
            continue;
        }
        let src = LogSource {
            path: f.to_string_lossy().to_string(),
            kind: LogSourceKind::ProviderFile,
            has_levels: true,
            level_pattern: None,
            label: Some(stem.to_string()),
            archives: vec![],
        };
        out.push(attach_archives(src));
    }
    out
}

/// SpringBoot 日志读取：tail/增量在主文件；历史走跨归档续接。keyword 做行过滤。
pub(crate) fn read_springboot_chunk(
    path: &Path,
    archives: &[ArchiveLog],
    archive_index: usize,
    offset: Option<u64>,
    before: bool,
    limit: usize,
    keyword: Option<&str>,
    regex: bool,
    level: Option<&str>,
) -> anyhow::Result<LogChunk> {
    let kw = keyword.filter(|s| !s.is_empty());
    let re_filter = if regex {
        kw.map(|k| {
            regex::Regex::new(k).map_err(|e| anyhow::anyhow!("正则编译失败: {}", e))
        })
        .transpose()?
    } else {
        None
    };
    let lv = level.filter(|s| !s.is_empty()).map(|s| s.to_string());
    let level_re = if lv.is_some() {
        Some(
            regex::Regex::new(DEFAULT_LEVEL_REGEX)
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
        None => {
            if !path.exists() {
                return Ok(LogChunk {
                    lines: vec![],
                    start_offset: 0,
                    end_offset: 0,
                    total_bytes: 0,
                    has_more: false,
                    truncated: false,
                    archive_index: 0,
                });
            }
            let (mut reader, total) = open_reader(path)?;
            let mut c = read_backward_filtered(&mut reader, total, total, limit, &filter)?;
            c.archive_index = 0;
            Ok(c)
        }
        Some(o) if before => read_log_archive_mode(path, archives, archive_index, o, limit, &filter),
        Some(o) => {
            if !path.exists() {
                return Ok(LogChunk {
                    lines: vec![],
                    start_offset: o,
                    end_offset: o,
                    total_bytes: 0,
                    has_more: false,
                    truncated: false,
                    archive_index: 0,
                });
            }
            let (mut reader, total) = open_reader(path)?;
            let mut c = read_since(&mut reader, total, o, limit, &filter)?;
            c.archive_index = 0;
            Ok(c)
        }
    }
}

/// 历史翻页的跨文件续接：在 `archive_index` 对应文件内从 `from_byte` 向前回溯；
/// 到文件头且有更旧归档时自动切到更旧归档的尾部继续读，实现无缝续接。
fn read_log_archive_mode(
    path: &Path,
    archives: &[ArchiveLog],
    archive_index: usize,
    from_byte: u64,
    limit: usize,
    filter: &LineFilter,
) -> anyhow::Result<LogChunk> {
    let cur_path = if archive_index == 0 {
        path.to_path_buf()
    } else {
        let a = archives
            .get(archive_index - 1)
            .ok_or_else(|| anyhow::anyhow!("归档索引越界: {}", archive_index))?;
        PathBuf::from(&a.path)
    };
    let (mut reader, total) = open_reader(&cur_path)?;
    let mut chunk = read_backward_filtered(&mut reader, total, from_byte.min(total), limit, filter)?;
    chunk.archive_index = archive_index;
    // 当前文件已展露到头部（from_byte==0），且有更旧档 → 无缝切换到下一个归档
    let at_head = from_byte == 0 || chunk.start_offset == 0;
    if at_head && archive_index < archives.len() {
        let nxt = &archives[archive_index];
        let npath = PathBuf::from(&nxt.path);
        let (mut nr, ntotal) = open_reader(&npath)?;
        let mut nchunk = read_backward_filtered(&mut nr, ntotal, ntotal, limit, filter)?;
        nchunk.archive_index = archive_index + 1;
        let mut lines = nchunk.lines.clone();
        lines.extend(chunk.lines);
        return Ok(LogChunk {
            lines,
            start_offset: nchunk.start_offset,
            end_offset: nchunk.end_offset,
            total_bytes: nchunk.total_bytes,
            has_more: nchunk.has_more || (archive_index + 1 < archives.len()),
            truncated: nchunk.truncated || chunk.truncated,
            archive_index: archive_index + 1,
        });
    }
    chunk.has_more = chunk.has_more || (chunk.start_offset == 0 && archive_index < archives.len());
    chunk.archive_index = archive_index;
    Ok(chunk)
}

/// 从 `from_byte` 向前（朝文件头）回溯，收集末尾 `limit` 个「匹配」行。
/// 用于 tail（from_byte = total）与历史分页（from_byte = 当前首行偏移）。
fn read_backward_filtered<R: Read + Seek>(
    reader: &mut R,
    total: u64,
    from_byte: u64,
    limit: usize,
    filter: &LineFilter,
) -> anyhow::Result<LogChunk> {
    let mut remaining = from_byte.min(total);
    let chunk_size = 8192u64;
    // 从后往前读块，累积原始字节（块列表逆序后重组为正序）
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    let is_filtering = filter.is_active();
    while remaining > 0 {
        let take = remaining.min(chunk_size);
        let start = remaining - take;
        let mut buf = vec![0u8; take as usize];
        reader.seek(SeekFrom::Start(start))?;
        reader.read_exact(&mut buf)?;
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
    let content_start = from_byte.saturating_sub(all.len() as u64);
    let text = decode_log_bytes(&all);
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
        archive_index: 0,
    })
}

/// 从字节 `from_byte` 向前（朝 EOF）读取匹配行，直到 EOF 或收满 `limit` 行。
/// 用于实时增量轮询（前端携带上次的 end_offset）。
fn read_since<R: Read + Seek>(
    reader: &mut R,
    total: u64,
    from_byte: u64,
    limit: usize,
    filter: &LineFilter,
) -> anyhow::Result<LogChunk> {
    reader.seek(SeekFrom::Start(from_byte))?;
    let mut reader = std::io::BufReader::new(&mut *reader);
    let mut lines: Vec<String> = Vec::new();
    let mut end_offset = from_byte;
    let mut truncated = false;
    loop {
        let _start = reader.stream_position()?;
        let mut raw = Vec::new();
        let n = reader.read_until(b'\n', &mut raw)?;
        if n == 0 {
            break; // EOF
        }
        let line = decode_log_bytes(&raw);
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
        archive_index: 0,
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
    fn test_decode_log_bytes_gbk_and_utf8() {
        // 真实 PG 日志字节（GBK 编码的「日志： 正在」，Windows 中文系统码页输出）
        let gbk: &[u8] = &[
            0xc8, 0xd5, 0xd6, 0xbe, 0x3a, 0x20, 0x20, 0xd5, 0xfd, 0xd4, 0xda, 0xc6, 0xf4,
            0xb6, 0xaf,
        ];
        let s = decode_log_bytes(gbk);
        assert!(s.contains("日志"), "GBK 解码应得中文: got {:?}", s);
        assert!(s.contains("正在"));

        let utf8 = "2026-08-18 INFO starting\n".as_bytes();
        assert_eq!(decode_log_bytes(utf8), "2026-08-18 INFO starting\n");

        // 混合：整体非 UTF-8 时回退 GBK（含 ASCII 前缀行）
        let mixed: &[u8] = b"2026-08-18 INFO ok\n\xc8\xd5\xd6\xbe\x3a\x20\xd5\xfd\xd4\xda\n";
        let sm = decode_log_bytes(mixed);
        assert!(sm.contains("2026-08-18 INFO ok"));
        assert!(sm.contains("日志"));
    }

    #[test]
    fn test_collect_archives_finds_gz_and_date_subdirs() {
        let base = std::env::temp_dir().join(format!("opx_qa_arch_{}", unique_suffix()));
        let logs = base.join("logs");
        std::fs::create_dir_all(&logs.join("2026-08-24")).unwrap();
        let write = |rel: &str, body: &[u8]| {
            let p = logs.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, body).unwrap();
        };
        write("info.log", b"now");
        write("info.2026-08-26.0.log.gz", b"x"); // 同目录 gz
        write("info.2026-08-25.0.log.gz", b"x");
        write("2026-08-24/info.2026-08-24.0.log.gz", b"x"); // 日期子目录
        write("access.log-20260824.gz", b"x"); // logrotate 形态（不同 stem，应忽略）
        write("readme.txt", b"x"); // 非日志，忽略

        let primary = logs.join("info.log");
        let archives = collect_archives(&primary);
        std::fs::remove_dir_all(&base).unwrap();
        let labels: Vec<String> = archives.iter().map(|a| a.label.clone()).collect();
        assert!(!labels.is_empty(), "应识别到归档");
        assert!(labels[0].contains("2026-08-26"), "最新在前, got {:?}", labels);
        assert!(labels.iter().any(|l| l.contains("2026-08-25")));
        assert!(labels.iter().any(|l| l.contains("2026-08-24")));
        assert!(!labels.iter().any(|l| l.contains("access")), "非同 stem 滚动应忽略");
    }

    #[test]
    fn test_read_backward_gz_tail() {
        let p = std::env::temp_dir().join(format!("opx_qa_gz_{}.log.gz", unique_suffix()));
        let content = b"L0\nL1\nL2\n";
        let file = std::fs::File::create(&p).unwrap();
        let enc = {
            let mut e = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            std::io::Write::write_all(&mut e, content).unwrap();
            e
        };
        enc.finish().unwrap();
        let (mut reader, total) = open_reader(&p).unwrap();
        let filter = LineFilter {
            keyword: None,
            regex: None,
            level: None,
            level_regex: None,
        };
        let chunk = read_backward_filtered(&mut reader, total, total, 100, &filter).unwrap();
        std::fs::remove_file(&p).unwrap();
        let non_empty: usize = chunk.lines.iter().filter(|l| !l.is_empty()).count();
        assert_eq!(non_empty, 3, "gz 归档应能读出行");
    }

    #[test]
    fn test_read_backward_cross_archive_switch() {
        let base = std::env::temp_dir().join(format!("opx_qa_xa_{}", unique_suffix()));
        std::fs::create_dir_all(&base).unwrap();
        let primary = base.join("info.log");
        std::fs::write(&primary, b"P0\nP1\n").unwrap();
        let gz_path = base.join("info.2026-08-24.0.log.gz");
        {
            let file = std::fs::File::create(&gz_path).unwrap();
            let enc = {
                let mut e = flate2::write::GzEncoder::new(file, flate2::Compression::default());
                std::io::Write::write_all(&mut e, b"O0\nO1\n").unwrap();
                e
            };
            enc.finish().unwrap();
        }
        let archives = collect_archives(&primary);
        assert_eq!(archives.len(), 1, "应有 1 个归档");
        let filter = LineFilter {
            keyword: None,
            regex: None,
            level: None,
            level_regex: None,
        };
        // 从主文件头（from_byte=0）继续往前翻 → 应切到归档尾部
        let chunk = read_log_archive_mode(&primary, &archives, 0, 0, 40, &filter).unwrap();
        std::fs::remove_dir_all(&base).unwrap();
        assert_eq!(chunk.archive_index, 1, "应切到 index=1 归档");
        let joint: String = chunk.lines.concat();
        assert!(joint.contains("O0"), "应读到归档旧行, got {:?}", chunk.lines);
        assert!(joint.contains("O1"), "应读到归档旧行");
    }

    #[test]
    fn test_collect_springboot_sources_level_files() {
        let base = std::env::temp_dir().join(format!("opx_qa_sb_{}", unique_suffix()));
        let logs = base.join("logs");
        std::fs::create_dir_all(&logs.join("2026-08-24")).unwrap();
        std::fs::write(logs.join("debug.log"), b"d").unwrap();
        std::fs::write(logs.join("info.log"), b"i").unwrap();
        std::fs::write(logs.join("error.log"), b"e").unwrap();
        std::fs::write(logs.join("info.2026-08-26.0.log.gz"), b"x").unwrap();
        std::fs::write(logs.join("2026-08-24/error.2026-08-24.0.log.gz"), b"x").unwrap();
        std::fs::write(logs.join("random.txt"), b"x").unwrap();

        let sources = collect_springboot_sources(&logs);
        std::fs::remove_dir_all(&base).unwrap();
        let names: Vec<String> = sources.iter().map(|s| s.label.clone().unwrap_or_default()).collect();
        assert!(names.contains(&"debug".to_string()), "含 debug 源, got {:?}", names);
        assert!(names.contains(&"info".to_string()), "含 info 源");
        assert!(names.contains(&"error".to_string()), "含 error 源");
        assert!(!names.iter().any(|l| l.contains("random")), "非 *.log 忽略");
        for s in &sources {
            if s.label.as_deref() == Some("info") { assert!(!s.archives.is_empty(), "info 应有同目录归档"); }
            if s.label.as_deref() == Some("error") { assert!(!s.archives.is_empty(), "error 应有日期子目录归档"); }
        }
    }

    #[test]
    fn test_read_since_basic() {
        let p = tmp_file("since", b"a\nb\nc\n");
        let filter = LineFilter {
            keyword: None,
            regex: None,
            level: None,
            level_regex: None,
        };
        let (mut reader, total) = open_reader(&p).unwrap();
        let chunk = read_since(&mut reader, total, 0, 100, &filter).unwrap();
        assert_eq!(chunk.lines.len(), 3);
        assert_eq!(chunk.start_offset, 0);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn test_read_backward_tail_basic() {
        let content = b"L0\nL1\nL2\nL3\nL4\n";
        let p = tmp_file("tail", content);
        let filter = LineFilter {
            keyword: None,
            regex: None,
            level: None,
            level_regex: None,
        };
        let (mut reader, total) = open_reader(&p).unwrap();
        let chunk = read_backward_filtered(&mut reader, total, total, 100, &filter).unwrap();
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
        let filter = LineFilter {
            keyword: None,
            regex: None,
            level: None,
            level_regex: None,
        };
        // from_byte = 6（L2 起始），应返回 [0,6) 的 "L0","L1"，start_offset 正确应为 0
        let (mut reader, total) = open_reader(&p).unwrap();
        let chunk = read_backward_filtered(&mut reader, total, 6, 100, &filter).unwrap();
        assert_eq!(chunk.lines.iter().filter(|l| !l.is_empty()).count(), 2);
        assert_eq!(
            chunk.start_offset, 0,
            "before 模式 start_offset 计算错误（应为真实首字节偏移 0，当前实现会偏大）"
        );
        let _ = fs::remove_file(&p);
    }
}
