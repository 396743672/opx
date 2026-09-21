# 日志归档查看 · 服务组增强 · Node.js 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 spec `2026-08-26-log-archive-ops-design.md` 的 A/B/C/D/E 五项：日志归档无缝查看（软件+应用）、栈启动报告、运行面板端口/日志入口、Nacos 置灰预留、Node.js provider。

**Architecture:** 日志侧把「日志源」抽象为主文件+有序归档序列（`LogSource.archives`），历史翻页到文件头自动切更旧归档（`LogChunk.archive_index`），`.gz` 经 `flate2` 解压到内存后共用字节 offset/过滤逻辑；服务组侧 `Stack.last_run_report` 持久化最近一次启动报告；Node 参照 jre.rs 以 Runtime 定位新增 provider。

**Tech Stack:** Rust (tauri v2)、Vue 3 + TS、vue-i18n、flate2、walkdir、chrono、@tauri-apps/plugin-opener。

## Global Constraints

- 所有 Rust 改动须保持 `cargo test --lib` 全绿；前端改动须 `npx vue-tsc --noEmit` 通过。
- 分支：dev；每个功能完成提交到 dev（可建任务分支，合并后删除）。
- 不回退既有行为：日志 tail/增量/实时只在主文件；应用日志无 level 文件时退回单源旧行为。
- 新增字段一律 `#[serde(default, skip_serializing_if = "...")]` 向后兼容旧 JSON。
- 中英文文案必须同步维护 `src/locales/zh-CN.ts` 与 `en-US.ts`。
- 每个功能完成后，向 `RELEASE_NOTES.md` 追加该功能说明条目（供后期生成发行文件）。

---

### Task 1: 模型层 —— 日志归档字段 + TS 对齐

**Files:**
- Modify: `src-tauri/src/models/software.rs:154-184`（LogSource / LogChunk）
- Modify: `src/models/software.ts:240-270`（LogSource / LogChunk）
- Test: 无新测试（编译 + 类型检查即可；字段由后续 Task 消费）

**Interfaces:**
- Produces:
  - `pub struct ArchiveLog { pub path: String, pub label: String }`
  - `LogSource.archives: Vec<ArchiveLog>`（serde default+skip）
  - `LogChunk.archive_index: usize`（serde default=0）

- [ ] **Step 1: Rust 模型加 ArchiveLog + 字段**

在 `models/software.rs` LogSource 定义之前插入：

```rust
/// 历史归档日志描述（同目录/日期目录下滚动压缩的旧日志，时间倒序）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArchiveLog {
    pub path: String,
    /// 展示标签（如 "2026-08-24"）
    pub label: String,
}
```

`LogSource` struct 尾部追加：

```rust
    /// 历史归档（时间倒序，最新在前）。空 = 无归档。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub archives: Vec<ArchiveLog>,
```

`LogChunk` struct 尾部追加：

```rust
    /// 当前实际读取的历史归档索引（0=主文件）。前端下次请求携带它实现无缝续接。
    #[serde(default)]
    pub archive_index: usize,
```

- [ ] **Step 2: Rust 编译**

Run: `cargo test --lib --no-run`
Expected: 编译通过（字段默认值保证既有构造点无需改）。

- [ ] **Step 3: TS 模型对齐**

`src/models/software.ts` 的 `LogSource` 接口追加 `archives?: ArchiveLog[]`，`LogChunk` 追加 `archive_index?: number`；新增：

```ts
export interface ArchiveLog {
  path: string
  label: string
}
```

- [ ] **Step 4: 类型检查**

Run: `npx vue-tsc --noEmit`
Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/models/software.rs src/models/software.ts
git commit -m "feat(log): LogSource 增加历史归档序列与 archive_index 字段"
```

---

### Task 2: 归档识别 + 统一文件访问后端（LogBackend）+ .gz 解压

**Files:**
- Modify: `src-tauri/src/services/software_manager/log_viewer.rs`
- Test: `src-tauri/src/services/software_manager/log_viewer.rs`（mod tests 增补）

**Interfaces:**
- Produces:
  - `fn collect_archives(primary: &Path) -> Vec<ArchiveLog>`（同目录 + 一级日期子目录）
  - `enum LogBackend { File { path: PathBuf, len: u64 }, Gz { bytes: Vec<u8> } }`
  - `impl LogBackend { fn open(path:&Path)->anyhow::Result<Self>; fn len(&self)->u64; fn read_range(&self,start:u64,size:usize)->anyhow::Result<Vec<u8>> }`
  - `fn decompress_gzip(path:&Path) -> anyhow::Result<Vec<u8>>`

- [ ] **Step 1: 写失败测试（归档识别）**

在 `log_viewer.rs` tests 模块追加：

```rust
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
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --lib collect_archives`
Expected: FAIL（函数未定义 / collect::<Vec<_>> 报错）。

- [ ] **Step 3: 写失败测试（.gz 解压读取）**

```rust
#[test]
fn test_read_backward_gz_tail() {
    let p = std::env::temp_dir().join(format!("opx_qa_gz_{}.log.gz", unique_suffix()));
    let content = b"L0\nL1\nL2\n";
    let mut enc = flate2::write::GzEncoder::new(std::fs::File::create(&p).unwrap(), flate2::Compression::default());
    std::io::Write::write_all(&mut enc, content).unwrap();
    enc.finish().unwrap();
    let backend = LogBackend::open(&p).unwrap();
    let total = backend.len();
    let filter = LineFilter { keyword: None, regex: None, level: None, level_regex: None };
    let chunk = read_backward_filtered(&backend, total, total, 100, &filter).unwrap();
    std::fs::remove_file(&p).unwrap();
    let non_empty: usize = chunk.lines.iter().filter(|l| !l.is_empty()).count();
    assert_eq!(non_empty, 3, "gz 归档应能读出行");
}
```

- [ ] **Step 4: 运行确认失败**

Run: `cargo test --lib read_backward_gz`
Expected: FAIL（LogBackend 未定义）。

- [ ] **Step 5: 实现 collect_archives / LogBackend / decompress_gzip / 改造读取函数**

在 `log_viewer.rs` 顶部插入（use 增补 `std::path::PathBuf`、`flate2::read::MultiGzDecoder`）：

```rust
/// 归档扫描上限（防海量文件拖慢）
const MAX_ARCHIVES: usize = 40;

/// 判定文件名是否为日志滚动归档：含数字日期片段或 .gz / .N 结尾。
fn looks_like_archive(stem: &str) -> bool {
    stem.ends_with(".gz")
        || stem.ends_with(".log")
        || stem.chars().any(|c| c.is_ascii_digit())
}

/// 收集主文件的历史归档（同目录 + 一级日期子目录），时间倒序（最新在前）。
pub fn collect_archives(primary: &Path) -> Vec<ArchiveLog> {
    let Some(dir) = primary.parent() else { return vec![] };
    let Some(file_name) = primary.file_name().and_then(|s| s.to_str()) else { return vec![] };
    let stem = file_name.strip_suffix(".gz").unwrap_or(file_name);
    // 匹配前缀：形如 info.log→info；access.log→access；再找 info.*.gz / access.log-* 等
    let base = stem.split_once('.').map(|(b, _)| b).unwrap_or(stem);
    let mut found: Vec<ArchiveLog> = Vec::new();
    let mut scan = |dir: &Path| {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_file() { continue; }
            let Some(name) = p.file_name().and_then(|s| s.to_str()) else { continue };
            let lower = name.to_lowercase();
            if !looks_like_archive(lower) { continue; }
            // 必须是同源派生：info→info.xxx；access→access.log-xxx / access.log.xxx
            if !(name.starts_with(&format!("{}.", base)) || name.starts_with(&format!("{}-", base)) || name.starts_with(&format!("{}_", base))) {
                continue;
            }
            if name == file_name { continue; } // 排除主文件本身
            found.push(ArchiveLog { path: p.to_string_lossy().to_string(), label: name.to_string() });
        }
    };
    scan(dir);
    // 一级日期子目录（应用日志形态 logs/<YYYY-MM-DD>/type.date.n.log.gz）
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let d = e.path();
            if !d.is_dir() { continue; }
            scan(&d);
        }
    }
    found.sort_by(|a, b| b.path.cmp(&a.path)); // 字典序倒排近似时间倒序
    found.truncate(MAX_ARCHIVES);
    found
}

/// 统一文件访问：普通文件按需 seek；.gz 归档整体解压到内存。
pub enum LogBackend {
    File { path: PathBuf, len: u64 },
    Gz { bytes: Vec<u8> },
}

impl LogBackend {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let lower = path.to_string_lossy().to_lowercase();
        if lower.ends_with(".gz") {
            Ok(LogBackend::Gz { bytes: decompress_gzip(path)? })
        } else {
            Ok(LogBackend::File { path: path.to_path_buf(), len: std::fs::metadata(path)?.len() })
        }
    }
    pub fn len(&self) -> u64 {
        match self { LogBackend::File { len, .. } => *len, LogBackend::Gz { bytes } => bytes.len() as u64 }
    }
    pub fn read_range(&self, start: u64, size: usize) -> anyhow::Result<Vec<u8>> {
        match self {
            LogBackend::File { path, .. } => {
                use std::io::{Read, Seek, SeekFrom};
                let mut f = std::fs::File::open(path)?;
                f.seek(SeekFrom::Start(start))?;
                let mut buf = vec![0u8; size];
                let n = f.read(&mut buf)?;
                buf.truncate(n);
                Ok(buf)
            }
            LogBackend::Gz { bytes } => {
                let start = start.min(bytes.len() as u64) as usize;
                let end = (start + size).min(bytes.len());
                Ok(bytes[start..end].to_vec())
            }
        }
    }
}

/// 整体解压 .gz（日志按天归档，单文件体积可控）
pub fn decompress_gzip(path: &Path) -> anyhow::Result<Vec<u8>> {
    let f = std::fs::File::open(path)?;
    let mut out = Vec::new();
    let mut dec = MultiGzDecoder::new(f);
    std::io::Read::read_to_end(&mut dec, &mut out)?;
    Ok(out)
}
```

再**改造 `read_backward_filtered` / `read_since` 首参**：`path: &Path` → `backend: &LogBackend`，文件读取段替换为 `backend.read_range(...)`。内部 `std::fs::File::open(path)` 与 `file.seek`/`read_exact` 全部改为 `backend.read_range(start, take)`；`total` 参数改由 `backend.len()` 计算（函数签名去掉 `total` 用 `backend.len()`）。

`read_since` 同样改为 `&LogBackend`（用 `read_range` 顺序读块，EOF 由 `end >= total` 判定）。

- [ ] **Step 6: 运行测试确认通过**

Run: `cargo test --lib`
Expected: 新增 2 测试 PASS，既有测试（`test_read_backward_tail_basic` 等）改造后仍 PASS。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/services/software_manager/log_viewer.rs
git commit -m "feat(log): 归档识别 + LogBackend 统一文件访问 + .gz 解压读取"
```

---

### Task 3: read_log 跨文件无缝续接 + 命令参数 + provider 归档接入

**Files:**
- Modify: `src-tauri/src/services/software_manager/log_viewer.rs`（`read_log`、`list_log_sources` 加 `attach_archives`、`default_log_sources` 关联）
- Modify: `src-tauri/src/services/software_manager/providers/mod.rs:188-203`（`default_log_sources` 附加 archives）
- Modify: `src-tauri/src/services/software_manager/providers/nginx.rs:304-324`（覆盖源附 archives）
- Modify: `src-tauri/src/commands/software.rs:1846-1871`（`read_log` 增 `archive_index` 参数）
- Test: `log_viewer.rs` tests

**Interfaces:**
- Consumes: `collect_archives`、`LogBackend`、`ArchiveLog`
- Produces:
  - `fn attach_archives(mut s: LogSource) -> LogSource`
  - `read_log(...)` 签名增 `archive_index: usize`

- [ ] **Step 1: 写失败测试（跨文件续接）**

```rust
#[test]
fn test_read_backward_cross_archive_switch() {
    let base = std::env::temp_dir().join(format!("opx_qa_xa_{}", unique_suffix()));
    std::fs::create_dir_all(&base).unwrap();
    let primary = base.join("info.log");
    std::fs::write(&primary, b"P0\nP1\n").unwrap();
    // 归档（旧的一天）
    let gz_path = base.join("info.2026-08-24.0.log.gz");
    {
        let mut enc = flate2::write::GzEncoder::new(std::fs::File::create(&gz_path).unwrap(), flate2::Compression::default());
        std::io::Write::write_all(&mut enc, b"O0\nO1\n").unwrap();
        enc.finish().unwrap();
    }
    let archives = collect_archives(&primary);
    assert_eq!(archives.len(), 1);
    // 从主文件末尾往前翻：翻到文件头（0）后应切到归档尾部
    let src = LogBackend::open(&primary).unwrap();
    let filter = LineFilter { keyword: None, regex: None, level: None, level_regex: None };
    let chunk = read_log_archive_mode(&primary, &archives, 0, 0, true, 40, &filter).unwrap();
    std::fs::remove_dir_all(&base).unwrap();
    assert!(chunk.lines.iter().any(|l| l.contains("O")), "切到归档后应读到旧行, got {:?}", chunk.lines);
    assert_eq!(chunk.archive_index, 1, "应切到 index=1 归档");
}
```

（`read_log_archive_mode` 为 Task 3 新增的预期内部函数：给定主文件/归档列表/当前 index/from_byte/方向，返回拼接 chunk。）

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --lib read_backward_cross_archive`
Expected: FAIL（`read_log_archive_mode` 未定义）。

- [ ] **Step 3: 实现 read_log 扩展 + attach_archives + read_log_archive_mode**

在 `log_viewer.rs`：

```rust
/// 为单个日志源附加归档（provider 复用入口）
pub fn attach_archives(mut s: LogSource) -> LogSource {
    s.archives = collect_archives(Path::new(&s.path));
    s
}

/// 供 read_log 使用：在指定文件内回溯，到文件头且有更旧档则切下一归档尾部。
/// archive_index == 0 表示主文件；否则 archives[archive_index-1]。
fn read_log_archive_mode(
    path: &Path,
    archives: &[ArchiveLog],
    archive_index: usize,
    from_byte: u64,
    limit: usize,
    filter: &LineFilter,
) -> anyhow::Result<LogChunk> {
    let (cur_path, cur_total) = if archive_index == 0 {
        (path.to_path_buf(), std::fs::metadata(path)?.len())
    } else {
        let a = archives.get(archive_index - 1).ok_or_else(|| anyhow::anyhow!("归档索引越界"))?;
        let b = LogBackend::open(Path::new(&a.path))?;
        (a.path.clone(), b.len())
    };
    let backend = LogBackend::open(&cur_path)?;
    let total = backend.len();
    let mut chunk = read_backward_filtered(&backend, total, from_byte.min(total), limit, filter)?;
    // 当前文件已展露到头部（start_offset<=0 或 from_byte<=0），且有更旧档 → 无缝切换
    let at_head = from_byte == 0 || chunk.start_offset == 0;
    if at_head && archive_index < archives.len() {
        let next_idx = archive_index + 1;
        let nxt = &archives[next_idx - 1];
        let nb = LogBackend::open(Path::new(&nxt.path))?;
        let nt = nb.len();
        let mut nchunk = read_backward_filtered(&nb, nt, nt, limit, filter)?;
        nchunk.archive_index = next_idx;
        // 合并：新返回的行（旧日志）放在前面（时间更早）
        let mut lines = nchunk.lines;
        lines.extend(chunk.lines);
        // 用当前(主/旧档)的 total_bytes 展示当前读取目标大小
        return Ok(LogChunk {
            lines,
            start_offset: nchunk.start_offset,
            end_offset: nchunk.end_offset,
            total_bytes: nchunk.total_bytes,
            has_more: nchunk.has_more || nchunk.archive_index < archives.len(),
            truncated: nchunk.truncated || chunk.truncated,
            archive_index: next_idx,
        });
    }
    chunk.has_more = chunk.has_more || (chunk.start_offset == 0 && archive_index < archives.len());
    chunk.archive_index = archive_index;
    Ok(chunk)
}
```

改造 `read_log`（增 `archive_index: usize`，历史分支改走 `read_log_archive_mode`）：

```rust
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
    archive_index: usize, // 新增
) -> anyhow::Result<LogChunk> {
    // ...（既有 source/path/filter 构造同上）...
    // tail/增量仅主文件；历史走跨文件模式
    let archives = source.archives.clone();
    match offset {
        None => {
            let b = LogBackend::open(&path)?;
            let t = b.len();
            let mut c = read_backward_filtered(&b, t, t, limit, &filter)?;
            c.archive_index = 0;
            Ok(c)
        }
        Some(o) if before => read_log_archive_mode(&path, &archives, archive_index, o, limit, &filter),
        Some(o) => {
            let b = LogBackend::open(&path)?;
            let t = b.len();
            let mut c = read_since(&b, t, o, limit, &filter)?;
            c.archive_index = 0;
            Ok(c)
        }
    }
}
```

- [ ] **Step 4: attach 到默认源与 provider**

`providers/mod.rs::default_log_sources`：对生成的 `LogSource` 调 `crate::services::software_manager::log_viewer::attach_archives(s)`。

`nginx.rs::log_sources`：对两个源（access.log / error.log）在构造后调 `log_viewer::attach_archives`。

（其余 provider 均基于 `default_log_sources` 或自有 ProviderFile 源——为保证一致，mongodb.rs / mysql.rs / redis.rs / kafka.rs / elasticsearch.rs / nacos.rs / postgresql.rs 覆盖的 `log_sources` 里 append 的源也套 `attach_archives`；每个文件改动模式相同。）

- [ ] **Step 5: read_log 命令增 archive_index 参数**

`commands/software.rs:1846`：

```rust
pub async fn read_log(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
    source_index: usize,
    archive_index: Option<usize>,
    offset: Option<u64>,
    before: Option<bool>,
    limit: Option<u64>,
    keyword: Option<String>,
    regex: bool,
    level: Option<String>,
) -> Result<LogChunk, String> {
    let archive_index = archive_index.unwrap_or(0);
    // ... 调用 log_viewer::read_log(..., archive_index) ...
}
```

`src-tauri/src/lib.rs` 无需改（命令名/参数由 invoke 关键词传参）。

- [ ] **Step 6: 运行测试**

Run: `cargo test --lib`
Expected: 全部 PASS（含跨文件续接、gz、既有回归）。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/services/software_manager/log_viewer.rs src-tauri/src/services/software_manager/providers/mod.rs src-tauri/src/services/software_manager/providers/nginx.rs src-tauri/src/services/software_manager/providers/mongodb.rs src-tauri/src/services/software_manager/providers/mysql.rs src-tauri/src/services/software_manager/providers/redis.rs src-tauri/src/services/software_manager/providers/kafka.rs src-tauri/src/services/software_manager/providers/elasticsearch.rs src-tauri/src/services/software_manager/providers/nacos.rs src-tauri/src/services/software_manager/providers/postgresql.rs src-tauri/src/commands/software.rs
git commit -m "feat(log): 历史翻页跨归档无缝续接 + read_log 支持 archive_index"
```

---

### Task 4: LogViewerDialog 前端归档续接

**Files:**
- Modify: `src/modules/software-manager/components/LogViewerDialog.vue`

- [ ] **Step 1: 前端维护 archiveIndex**

Script：新增 `const archiveIndex = ref(0)`；`loadHistory` 的 invoke 参数加 `archiveIndex: archiveIndex.value`；返回后 `archiveIndex.value = chunk.archive_index ?? 0`。`loadTail`/实时轮询不携带 archive_index（主文件）。

- [ ] **Step 2: 手工走查**

Run: `npm run tauri:dev`；软件管理 → 选 nginx/MySQL 实例日志 → 连续点「加载更早」跨过归档。
Expected: 无断裂、.gz 内容可读、到达最早档后按钮停用。

- [ ] **Step 3: 类型检查 + Commit**

Run: `npx vue-tsc --noEmit`
Commit: `feat(log): 软件日志查看器归档无缝续接`

---

### Task 5: 应用日志多源 + 归档（后端）

**Files:**
- Modify: `src-tauri/src/commands/springboot.rs`（`LogChunk` 收敛、`read_springboot_log` 重构、新增 `list_springboot_log_sources`）
- Modify: `src-tauri/src/services/software_manager/log_viewer.rs`（将 `read_backward_filtered`/`read_since` 设为 `pub(crate)`，`read_log_archive_mode` 复用；新增 `fn collect_logs_dir_sources(logs_dir: &Path) -> Vec<LogSource>` 供级联）
- Modify: `src-tauri/src/lib.rs:275`（注册 `list_springboot_log_sources`）
- Test: `commands/springboot.rs` 或 `log_viewer.rs` tests

**Interfaces:**
- Consumes: `LogSource`、`LogBackend`、`read_log_archive_mode`、`collect_archives`
- Produces:
  - `fn collect_springboot_sources(logs_dir: &Path) -> Vec<LogSource>`（按 level 多源 + 每源 archives，含日期子目录）
  - 命令 `list_springboot_log_sources(app_id: String) -> Result<Vec<LogSource>, String>`
  - 命令 `read_springboot_log(app_id: String, source_index: usize, archive_index: usize, offset: u64) -> Result<LogChunk, String>`

- [ ] **Step 1: 写失败测试（springboot 源发现）**

在 `log_viewer.rs` tests 追加：

```rust
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
    assert!(names.contains(&"debug".to_string()));
    assert!(names.contains(&"info".to_string()));
    assert!(names.iter().any(|l| l.contains("error")), "含 error 源, got {:?}", names);
    assert!(!names.iter().any(|l| l.contains("random")), "非 *.log 忽略");
    for s in &sources {
        if s.label.as_deref() == Some("info") { assert!(!s.archives.is_empty(), "info 应有归档"); }
        if s.label.as_deref() == Some("error") { assert!(!s.archives.is_empty(), "error 应有日期目录归档"); }
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --lib collect_springboot_sources`
Expected: FAIL（函数未定义）。

- [ ] **Step 3: 实现 collect_springboot_sources + 命令**

`log_viewer.rs` 新增：

```rust
/// 按 level 拆分发现 SpringBoot 日志源：logs/*.log 各为源，归档含同目录与日期子目录。
pub fn collect_springboot_sources(logs_dir: &Path) -> Vec<LogSource> {
    let mut out: Vec<LogSource> = Vec::new();
    if !logs_dir.is_dir() { return out; }
    let Ok(rd) = std::fs::read_dir(logs_dir) else { return out };
    let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && p.extension().map_or(false, |x| x == "log")).collect();
    files.sort();
    for f in files {
        let Some(stem) = f.file_stem().and_then(|s| s.to_str()) else { continue };
        // stem 即 level 名：debug/info/error/warn/...；过滤非 level（如 console）
        if !["debug","info","error","warn","trace","console"].iter().any(|l| *l == stem) { continue; }
        let mut src = LogSource {
            path: f.to_string_lossy().to_string(),
            kind: LogSourceKind::ProviderFile,
            has_levels: true,
            level_pattern: None,
            label: Some(stem.to_string()),
            archives: vec![],
        };
        src = attach_archives(src);
        out.push(src);
    }
    out
}
```

将 `read_backward_filtered` / `read_since` / `read_log_archive_mode` 改为 `pub(crate)`（供 commands/springboot.rs 复用）。

`commands/springboot.rs` 新增命令（替换旧 `read_springboot_log`）：

```rust
#[tauri::command]
pub async fn list_springboot_log_sources(
    manager: State<'_, Arc<SpringBootManager>>,
    app_id: String,
) -> Result<Vec<LogSource>, String> {
    let app = manager.find_app(&app_id).map_err(|e| e.to_string())?;
    let abs = crate::utils::paths::resolve_data_path(&app.log_path);
    let dir = if abs.is_file() { abs.parent().map(|p| p.to_path_buf()).unwrap_or(abs) } else { abs };
    Ok(crate::services::software_manager::log_viewer::collect_springboot_sources(&dir))
}

#[tauri::command]
pub async fn read_springboot_log(
    manager: State<'_, Arc<SpringBootManager>>,
    app_id: String,
    source_index: usize,
    archive_index: Option<usize>,
    offset: Option<u64>,
    before: Option<bool>,
    limit: Option<u64>,
    keyword: Option<String>,
) -> Result<LogChunk, String> {
    let app = manager.find_app(&app_id).map_err(|e| e.to_string())?;
    let abs = crate::utils::paths::resolve_data_path(&app.log_path);
    let dir = if abs.is_file() { abs.parent().map(|p| p.to_path_buf()).unwrap_or(abs) } else { abs };
    let sources = crate::services::software_manager::log_viewer::collect_springboot_sources(&dir);
    let source = sources.get(source_index).ok_or_else(|| format!("日志源索引越界: {}", source_index))?;
    let archive_index = archive_index.unwrap_or(0);
    let limit = limit.map(|l| l as usize).unwrap_or(2000);
    let before = before.unwrap_or(false);
    let path = std::path::Path::new(&source.path);
    let archives = source.archives.clone();
    let kw = keyword.filter(|s| !s.is_empty());
    let filter = crate::services::software_manager::log_viewer::make_basic_filter(kw.as_deref());
    crate::services::software_manager::log_viewer::read_springboot_chunk(&path, &archives, archive_index, offset, before, limit, &filter)
        .map_err(|e| e.to_string())
}
```

需要在 `log_viewer.rs` 暴露两个辅助：
- `pub fn make_basic_filter(keyword: Option<&str>) -> LineFilter`（LineFilter 目前是私有 struct → 加 `pub(crate)` 与构造；或提供 `read_springboot_chunk` 直接把 keyword 传入，内部构造 filter）。
- `pub fn read_springboot_chunk(path, archives, archive_index, offset, before, limit, filter) -> Result<LogChunk>`：
  - offset None → tail 主文件；
  - before → `read_log_archive_mode`；
  - 否则增量子文件。

删除旧 `commands/springboot.rs` 的 `LogChunk { lines, offset }` 定义与旧 `read_springboot_log`；`use crate::models::software::LogChunk`。

- [ ] **Step 4: 注册命令**

`src-tauri/src/lib.rs`：把 `commands::springboot::read_springboot_log` 换成 `list_springboot_log_sources` + `read_springboot_log`（新签名 invoke 名称不变）。

- [ ] **Step 5: 运行测试 + 类型检查**

Run: `cargo test --lib` 且 `npx vue-tsc --noEmit`
Expected: 全绿。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/services/software_manager/log_viewer.rs src-tauri/src/commands/springboot.rs src-tauri/src/lib.rs
git commit -m "feat(log): SpringBoot 日志按 level 多源 + 日期目录归档"
```

---

### Task 6: 应用日志查看器前端多源 + 归档

**Files:**
- Modify: `src/modules/springboot-manager/components/LogViewer.vue`（重构为多源）
- Modify: `src/modules/springboot-manager/pages/SpringBootPage.vue`（调用处改传 appId 而非 path；或复用回调）

**Interfaces:**
- Consumes: 命令 `list_springboot_log_sources`、新 `read_springboot_log(appId, sourceIndex, archiveIndex, offset, before, limit, keyword)`

- [ ] **Step 1: 重构 LogViewer.vue**

- 挂载时 `invoke('list_springboot_log_sources', { appId: props.appId })` 拿 `sources`；生成源 tab。
- 维护 `activeSource` / `archiveIndex` / `offset`（主文件增量 offset）。
- tail/轮询：`read_springboot_log(appId, activeSource, 0, null, false, 2000, keyword)`；offset==0 首次。
- 「加载更早」按钮：`read_springboot_log(appId, activeSource, archiveIndex, startOffset, true, 2000, keyword)`，同步返回 `archive_index`。
- 关键字过滤：前后端组合（keyword 传给后端 + 前端已有 highlight 保留为可选；为最小改动，keyword 传后端做行过滤）。

- [ ] **Step 2: 类型检查 + 走查**

Run: `npx vue-tsc --noEmit`；`npm run tauri:dev` 应用带 logback level 日志显示 4 个源 tab，各源可翻归档。

- [ ] **Step 3: Commit**

```bash
git add src/modules/springboot-manager/components/LogViewer.vue src/modules/springboot-manager/pages/SpringBootPage.vue
git commit -m "feat(log): 应用日志查看器多源 tab + 日期目录归档续接"
```

---

### Task 7: 栈启动报告（后端持久化）

**Files:**
- Modify: `src-tauri/src/models/stack.rs`（`StackRunReport`、`StackMemberReport`、`Stack.last_run_report`）
- Modify: `src-tauri/src/services/stack_manager.rs`（`start()` 记录耗时并写 `last_run_report` + save）
- Test: `stack_manager.rs` 或新 `stack_report.rs` tests

**Interfaces:**
- Produces:
  - `pub struct StackRunReport { pub started_at: String, pub total_elapsed_ms: u64, pub members: Vec<StackMemberReport> }`
  - `pub struct StackMemberReport { pub ref_id: String, pub status: StackMemberStatus, pub elapsed_ms: u64, pub message: String }`
  - `Stack.last_run_report: Option<StackRunReport>`

- [ ] **Step 1: 写失败测试（聚合纯函数）**

在 `stack_manager.rs` tests 追加：

```rust
#[test]
fn test_build_run_report_aggregates_elapsed() {
    // 模拟成员耗时记录：ref_id → (终态, elapsed_ms, message)
    let members = vec![
        ("mysql".to_string(), "Running".to_string(), 1200u64, String::new()),
        ("redis".to_string(), "Running".to_string(), 800u64, String::new()),
        ("app".to_string(), "Failed".to_string(), 3000u64, "port busy".to_string()),
    ];
    let report = build_run_report("2026-08-26T00:00:00Z", 5000u64, members);
    assert_eq!(report.total_elapsed_ms, 5000);
    assert_eq!(report.members.len(), 3);
    assert_eq!(report.members[2].status, StackMemberStatus::Failed);
    assert_eq!(report.members[2].elapsed_ms, 3000);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --lib build_run_report`
Expected: FAIL（`build_run_report` 未定义）。

- [ ] **Step 3: 实现模型与聚合**

`models/stack.rs`：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackRunReport {
    pub started_at: String,
    pub total_elapsed_ms: u64,
    pub members: Vec<StackMemberReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackMemberReport {
    pub ref_id: String,
    pub status: StackMemberStatus,
    pub elapsed_ms: u64,
    pub message: String,
}
```

`Stack` 结构体追加：

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub last_run_report: Option<StackRunReport>,
```

`stack_manager.rs`：

```rust
/// 聚合启动报告（纯函数，便于单测）
pub fn build_run_report(
    started_at: String,
    total_elapsed_ms: u64,
    members: Vec<(String, StackMemberStatus, u64, String)>, // (ref_id, status, elapsed_ms, message)
) -> StackRunReport {
    StackRunReport {
        started_at,
        total_elapsed_ms,
        members: members.into_iter().map(|(ref_id, status, elapsed_ms, message)| {
            StackMemberReport { ref_id, status, elapsed_ms, message }
        }).collect(),
    }
}
```

- [ ] **Step 4: start() 埋点写报告**

在 `start()` 开头记录 `let started_at = chrono::Local::now().to_rfc3339(); let started_instant = std::time::Instant::now();`。
引入 `HashMap<String, std::time::Instant>` 记录每成员 `t0`（组内成员在 `do_start_member` 前后、外部依赖在 `start_external` 前后）。到达终态（Running/Failed/Stopped）时计算 `elapsed_ms = t0.elapsed().as_millis() as u64`。
在函数末尾（成功与失败回滚路径统一）构造 `Vec<StackMemberReport>` 并赋 `stack.last_run_report = Some(report)`，随后 `self.inner.lock().unwrap().save()`。若 `get(id)` 已 clone 则改用 `self.get(id)` 再写回（`save` 是 `StackManagerInner` 方法，需 `let mut inner = self.inner.lock().unwrap(); inner.stacks.iter_mut().find(...)`）。

- [ ] **Step 5: 运行测试 + 类型检查**

Run: `cargo test --lib`
Expected: 全绿。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/models/stack.rs src-tauri/src/services/stack_manager.rs
git commit -m "feat(stack): 启动报告持久化最近一次（成员耗时/结果）"
```

---

### Task 8: StackRunPanel 报告区 + 端口/日志入口

**Files:**
- Modify: `src/modules/stack/StackRunPanel.vue`
- Modify: `src/models/stack.ts`（对齐 `StackRunReport` / `last_run_report`）

**Interfaces:**
- Consumes: stack store 的 `stack.last_run_report`、`installedSoftware`、`springbootApps`

- [ ] **Step 1: TS 类型对齐**

`src/models/stack.ts`：`Stack` 增 `last_run_report?: StackRunReport | null`；新增 `StackRunReport`/`StackMemberReport` 接口（字段同后端）。

- [ ] **Step 2: 报告区渲染**

`StackRunPanel.vue` 成员状态区下方加区块：`v-if="stack.last_run_report"`：

```html
<div class="report-block" v-if="stack.last_run_report">
  <div class="section-title">
    <Icon icon="mdi:chart-timeline-variant" /> {{ $t('lastRunReport') }}
    <span class="report-total">{{ $t('totalElapsed') }}: {{ fmtMs(report.total_elapsed_ms) }}</span>
  </div>
  <div v-for="m in report.members" :key="m.ref_id" class="report-row" :class="'r-' + m.status.toLowerCase()">
    <span class="r-name">{{ resolveName(m.ref_id) }}</span>
    <span class="r-bar"><i :style="{ width: pct(m) }"></i></span>
    <span class="r-time">{{ fmtMs(m.elapsed_ms) }}</span>
    <span class="r-msg" v-if="m.message">{{ m.message }}</span>
  </div>
</div>
```

helpers：`report` computed（取 `stack.last_run_report`）、`fmtMs`、`pct(m)`（`m.elapsed_ms / max(elapsed)*100`）。

- [ ] **Step 3: 端口按钮**

成员卡 `mc-bottom` 后加：

```html
<div class="mc-actions" v-if="portOf(item)">
  <button class="mini-btn" @click="openPort(item)"><Icon icon="mdi:open-in-new" /> {{ portOf(item) }}</button>
  <button class="mini-btn" @click="openLogs(item)"><Icon icon="mdi:file-document-outline" /> {{ $t('viewLogs') }}</button>
</div>
```

`portOf(item)`：`ref_type==='software'` → stack store `installedSoftware.find(s.id===item.ref_id)?.port`；springboot → `springbootApps.find(a.id===item.ref_id)?.port`。`openPort` 用 `openUrl('http://127.0.0.1:'+port)`（`import { openUrl } from '@tauri-apps/plugin-opener'`）。

- [ ] **Step 4: 日志弹窗**

RunPanel 加 `logTarget` ref（`{ kind:'software', id } | { kind:'springboot', id, name, logPath } | null`）；`openLogs` 设置它；模板 `v-if` 渲染：

```html
<LogViewerDialog v-if="logTarget?.kind==='software'" :software="softwareById(logTarget.id)" @close="logTarget=null" />
<LogViewer v-else-if="logTarget?.kind==='springboot'" :appId="logTarget.id" :appName="logTarget.name" :logPath="logTarget.logPath" @close="logTarget=null" />
```

引入 import `LogViewerDialog`、`logViewer as LogViewer`（springboot）。

- [ ] **Step 5: 文案 + 类型检查 + 走查**

`src/locales/zh-CN.ts` / `en-US.ts`：`lastRunReport`、`totalElapsed`（en: `Last run report` / `Total elapsed`）。
Run: `npx vue-tsc --noEmit`；`npm run tauri:dev` 启动一个栈，RunPanel 显示报告、端口可开、日志弹窗。

- [ ] **Step 6: Commit**

```bash
git add src/modules/stack/StackRunPanel.vue src/models/stack.ts src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat(stack): 启动报告展示 + 成员端口链接与日志入口"
```

---

### Task 9: Node.js provider

**Files:**
- Create: `src-tauri/src/services/software_manager/providers/node.rs`
- Modify: `src-tauri/src/services/software_manager/providers/mod.rs`（`all_providers()` 注册）
- Modify: `src-tauri/src/services/software_manager/providers/mod.rs`（`pub mod node;`）
- Modify: `src/locales/zh-CN.ts` / `en-US.ts`（`catalogDesc.node`）

**Interfaces:**
- Produces:
  - `pub struct NodeProvider;` 实现 `SoftwareProvider`（key="node"，category Runtime，`catalog_entry()`，`fetch_remote_versions()`）
  - `fn parse_node_index(json: &str) -> Vec<String>`（纯函数）

- [ ] **Step 1: 写失败测试（index.json 解析）**

同文件 tests：

```rust
#[test]
fn parse_node_index_keeps_lts_pure() {
    let json = r#"[
        {"version":"v22.14.0","lts":"Jod"},
        {"version":"v20.11.1","lts":"Iron"},
        {"version":"v23.0.0","lts":false},
        {"version":"v22.14.0-rc.1","lts":false}
    ]"#;
    let vs = parse_node_index(json);
    assert_eq!(vs, vec!["20.11.1".to_string(), "22.14.0".to_string()]);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --lib parse_node_index`
Expected: FAIL（未定义）。

- [ ] **Step 3: 实现 node.rs**

```rust
use anyhow::Result;
use crate::models::software::{ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, MirrorSource, SoftwareCategory};
use super::{InstallContext, SoftwareProvider};

pub struct NodeProvider;
impl NodeProvider { pub fn new() -> Self { Self } }
impl Default for NodeProvider { fn default() -> Self { Self::new() } }

/// 解析 nodejs.org/dist/index.json：仅保留 LTS 且纯 X.Y.Z（去 v 前缀），升序。
pub fn parse_node_index(json: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(json) else { return out };
    for item in arr {
        let is_lts = item.get("lts").and_then(|v| v.as_bool()).unwrap_or(false) || item.get("lts").map(|v| v.is_string()).unwrap_or(false);
        if !is_lts { continue; }
        let Some(v) = item.get("version").and_then(|v| v.as_str()) else { continue };
        let clean = v.strip_prefix('v').unwrap_or(v);
        if !clean.chars().all(|c| c.is_ascii_digit() || c == '.') { continue; }
        if clean.split('.').count() == 3 && !out.contains(&clean.to_string()) { out.push(clean.to_string()); }
    }
    out.sort_by(|a, b| compare_versions(a, b)); // 数字分段升序
    out
}

use crate::services::software_manager::upgrade::compare_versions; // 若存在；否则写内联数字比较
```

（`compare_versions` 位置需实现时确认，来自 R2 升级检测；若模块路径不同，inline 实现数字分段比较。）

`SoftwareProvider` impl：

```rust
impl SoftwareProvider for NodeProvider {
    fn key(&self) -> &str { "node" }
    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];
        #[cfg(windows)]
        for ver in ["20.11.1", "22.14.0"] {
            versions.push(CatalogVersion {
                version: ver.to_string(),
                mirrors: vec![MirrorSource {
                    name: "nodejs.org".into(),
                    url: format!("https://nodejs.org/dist/v{ver}/node-v{ver}-win-x64.zip"),
                    builtin: None,
                }],
                archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
            });
        }
        CatalogEntry {
            key: "node".into(),
            name: "Node.js".into(),
            description: "JavaScript 运行时（Node.js LTS）".into(),
            description_i18n: Some("catalogDesc.node".into()),
            category: SoftwareCategory::Runtime,
            icon: "mdi:language-nodejs".into(),
            versions,
            default_version: "22.14.0".into(),
        }
    }
    fn fetch_remote_versions(&self) -> Result<Vec<String>> {
        let resp = minreq::get("https://nodejs.org/dist/index.json").send().map_err(|e| anyhow::anyhow!("{e}"))?;
        if resp.status_code != 200 { return Ok(vec![]); }
        Ok(parse_node_index(&resp.as_str().unwrap_or_default()))
    }
}
```

（`SoftwareProvider::fetch_remote_versions` 的准确签名与返回见 kafka.rs 既有实现，照抄其错误处理风格。）

- [ ] **Step 4: 注册**

`providers/mod.rs`：`pub mod node;`；`all_providers()` 里 `vec![...]` 尾部加 `Box::new(node::NodeProvider::new()),`。

- [ ] **Step 5: 文案 + 测试 + 类型检查**

`catalogDesc.node`：中 `'JavaScript 运行时（Node.js LTS）'`；en 对应。判断 `compare_versions` 路径正确后 `cargo test --lib` 全绿。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/services/software_manager/providers/node.rs src-tauri/src/services/software_manager/providers/mod.rs src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat(software): 新增 Node.js Runtime provider（LTS + 动态版本）"
```

---

### Task 10: Nacos 集群选项置灰

**Files:**
- Modify: `src-tauri/src/models/software.rs:320-324`（`ConfigFieldType::Select` 增 `disabled_options` / `disabled_hint_i18n`）
- Modify: `src-tauri/src/services/software_manager/providers/nacos.rs`（mode 字段 schema + 注释）
- Modify: `src/modules/software-manager/components/ConfigFormTab.vue`（select 渲染 disabled）
- Modify: `src/locales/zh-CN.ts` / `en-US.ts`（`configField.nacosModeClusterHint`）
- Modify: `src/models/software.ts`（Select 类型对齐）

**Interfaces:**
- Produces: `ConfigFieldType::Select { options, labels, #[serde(default)] disabled_options: Vec<String>, #[serde(default)] disabled_hint_i18n: Option<String> }`

- [ ] **Step 1: Rust 模型扩展**

`models/software.rs` `Select` variant 追加字段：

```rust
Select {
    options: Vec<String>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    disabled_options: Vec<String>,
    #[serde(default)]
    disabled_hint_i18n: Option<String>,
},
```

既有构造点（各地 `Select { options, labels }`）——`serde(default)` 保证反序列化兼容，但**字面量构造**需补字段。逐个 provider `ConfigFieldType::Select { ... }` 构造处追加 `disabled_options: vec![], disabled_hint_i18n: None`。用 `cargo test --lib` 编译暴露所有漏改点。

- [ ] **Step 2: nacos mode 字段置灰**

`nacos.rs` config schema 中 mode 字段：

```rust
ConfigField {
    key: "mode".into(),
    label_i18n: "configField.nacosMode".into(),
    field_type: ConfigFieldType::Select {
        options: vec!["standalone".into(), "cluster".into()],
        labels: vec![], // 显示原值
        disabled_options: vec!["cluster".into()],
        disabled_hint_i18n: Some("configField.nacosModeClusterHint".into()),
    },
    // ...既有 default_value ...
},
```

`start_command` 处 cluster 报错保留；加注释 `// ponytail: cluster 扩展点 —— 后续按 nacos 官方多节点 RAFT 接入，禁用为前置提示，不建实现`。

- [ ] **Step 3: TS 对齐 + 前端渲染**

`src/models/software.ts` `Select` union 增 `disabled_options?: string[]`、`disabled_hint_i18n?: string`。

`ConfigFormTab.vue` select 分支：

```html
<select v-model="formData[field.key]" class="input">
  <option v-for="(opt, i) in selectOptions(field)" :key="opt" :value="opt"
    :disabled="isDisabledOption(field, opt)"
    :title="disabledHint(field, opt)">
    {{ selectLabels(field)?.[i] ?? opt }}
  </option>
</select>
```

```ts
function isDisabledOption(f: ConfigField, opt: string): boolean {
  return f.field_type.type === 'Select' && (f.field_type.disabled_options ?? []).includes(opt)
}
function disabledHint(f: ConfigField, opt: string): string | undefined {
  if (!isDisabledOption(f, opt)) return undefined
  const key = f.field_type.type === 'Select' ? f.field_type.disabled_hint_i18n : undefined
  return key ? t(key) : undefined
}
```

- [ ] **Step 4: 文案 + 测试 + 类型检查**

`configField.nacosModeClusterHint`：中「集群模式将在后续版本支持，当前请使用单机模式」。`cargo test --lib` 全绿；`npx vue-tsc --noEmit` PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/models/software.rs src-tauri/src/services/software_manager/providers/nacos.rs src/modules/software-manager/components/ConfigFormTab.vue src/models/software.ts src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat(software): Nacos 集群选项前置置灰（保留扩展点）"
```

---

### Task 11: 发行说明维护 + 全量验证

**Files:**
- Modify: `RELEASE_NOTES.md`（或新建 `docs/changelog-0.4.md`，视项目习惯）

- [ ] **Step 1: RELEASE_NOTES 汇总本期功能**

在 `RELEASE_NOTES.md` 追加本期区块（若文件为 0.3.0 单一发行说明，则在顶部新增「0.4.0」章节）：

```markdown
## 0.4.0（本期新增）

### 日志
- 日志查看支持历史归档无缝续接（.gz 滚动 / 按天归档自动翻转）。
- 应用日志按 level 多源展示（debug/info/error/warn）+ 日期目录归档。
### 服务组
- 启动报告：最近一次启动的成员耗时与结果展示。
- 成员端口链接一键打开、日志查看快捷入口。
### 软件仓库
- 新增 Node.js Runtime（LTS + 在线版本）。
### 其他
- Nacos 集群选项前置置灰（保留后续扩展点）。
```

- [ ] **Step 2: 全量验证**

Run: `cargo test --lib`；`npx vue-tsc --noEmit`；`npm run build`
Expected: 全绿。

- [ ] **Step 3: Commit**

```bash
git add RELEASE_NOTES.md
git commit -m "docs: 汇总 0.4.0 本期功能到发行说明"
```

---

## Self-Review

**1. Spec 覆盖**
- C-1（软件归档续接）→ Task 1-4 ✓
- C-2（应用按 level 多源 + 日期归档）→ Task 5-6 ✓
- A（启动报告持久化）→ Task 7-8 ✓
- B（端口链接 + 日志入口）→ Task 8 ✓（同一 RunPanel 改造）
- E（Node provider）→ Task 9 ✓
- D（Nacos 置灰）→ Task 10 ✓
- 发行说明 → Task 11 ✓
- 快照保留清理（spec 标记已实现 / Out）→ 无任务 ✓

**2. 占位符扫描**：无 TBD/TODO；三处实现时确认点（`compare_versions` 模块路径、`LineFilter`/`read_*` 可见性、provider Select 字面量构造点数量）已在对应 Step 标注为「实现时确认」，非计划缺口。

**3. 类型一致性**：`ArchiveLog`/`LogChunk.archive_index`/`LogBackend`/`read_log_archive_mode`/`collect_springboot_sources`/`StackRunReport`/`build_run_report`/`parse_node_index`/`Select.disabled_options` 在跨 Task 引用处签名一致。

**风险注记**：`stack_manager::start()` 为异步复杂编排，埋点需谨慎（推荐集中在一个收尾 helper 聚合 `member_status` 现状 + `t0` map，避免侵入每分支）；`read_backward_filtered` 改首参为 `&LogBackend` 会触碰既有测试与主文件读取路径，Task 2/3 已含对应测试兜底。