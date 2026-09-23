//! JVM 指标采集。
//!
//! # 采集源
//!
//! | 数据 | 来源 | 说明 |
//! |---|---|---|
//! | 堆各区 / Metaspace / 分代 GC | `jstat -gc <pid>` | 一次快照，内存单位 KB、时间单位秒 |
//! | 线程数 / 类加载 | `jcmd <pid> PerfCounter.print` | 读 perfdata 共享内存 |
//! | 堆上限 Xmx | `jcmd <pid> VM.flags` | 作为堆进度条的分母 |
//!
//! # 为什么不用 `jcmd GC.heap_info` + `Thread.print`（2026-09-23 重写）
//!
//! 旧实现的三个缺陷，前两个会导致前端展示错误数据：
//!
//! 1. `parse_heap_info` 匹配到 heap 行后立即 `return`，而 `Metaspace` 行位于
//!    该行**之后**，永远扫描不到 → 非堆内存恒为 0。
//! 2. 线程数取 `Thread.print` 输出的**总行数**，其中含全部堆栈帧
//!    → 实测把 82 个线程报成 854。
//! 3. `gc_count` / `gc_time` 在结构体里被硬编码为 0，从未真正采集。
//!
//! 此外 `Thread.print` 每 5 秒触发一次 thread dump，会让目标 JVM 走到
//! safepoint（STW）。新方案的三个命令都只读取数据，不产生停顿。
//!
//! # 已知限制
//!
//! jstat / jcmd 需 attach 到目标 JVM：目标以更高权限运行而本进程不足时会被拒。
//! 命令走同步阻塞式调用，若目标完全失去响应可能长时间占用一个 tokio worker
//! （jstat/jcmd 对 attach 失败会在数秒内返回，实测未出现挂起）。
//!
//! 实测三条命令合计约 1.2 秒（JDK 21 上 jstat -gc 单独 0.25 秒），占 5 秒轮询
//! 间隔约两成。暂不做缓存：按 pid 缓存 Xmx 会在 pid 复用后读到上一个进程的
//! 堆上限，收益不抵风险。

use crate::models::springboot::JvmInfo;
use crate::utils::process::hidden;
use std::path::{Path, PathBuf};

/// 采集指定 JVM 进程的指标快照。
///
/// `jdk_path` 为 `Some` 时从该 JDK 的 `bin/` 取诊断工具，否则回退到 PATH。
/// 失败时返回带原因的 `Err`（工具缺失、非零退出、输出无法解析），
/// 原因会透传到前端展示——不要在此层吞错误，否则前端只能瞎猜。
pub fn collect_jvm_metrics(pid: u32, jdk_path: Option<String>) -> Result<JvmInfo, String> {
    let jdk = jdk_path.as_deref();

    // 主数据源：堆、Metaspace、分代 GC。拿不到即视为采集失败。
    let gc = parse_jstat_gc(&run_jdk_tool(pid, "jstat", &["-gc"], jdk)?)
        .ok_or_else(|| "jstat 输出无法解析（表头缺失、列数不符或堆容量为 0）".to_string())?;

    // 线程与类加载：辅助信息，缺失时保留 0 而不阻断整次采集
    let perf = parse_perf_counters(
        &run_jdk_tool(pid, "jcmd", &["PerfCounter.print"], jdk).unwrap_or_default(),
    );

    // 堆上限：VM.flags 未给出 MaxHeapSize 时回退为当前提交容量
    let heap_max =
        parse_max_heap_size(&run_jdk_tool(pid, "jcmd", &["VM.flags"], jdk).unwrap_or_default())
            .unwrap_or(gc.heap_committed);

    Ok(JvmInfo {
        heap_used: gc.heap_used,
        heap_max,
        heap_committed: gc.heap_committed,
        non_heap_used: gc.non_heap_used,
        non_heap_committed: gc.non_heap_committed,
        thread_count: perf.thread_count,
        thread_daemon: perf.thread_daemon,
        thread_peak: perf.thread_peak,
        thread_started: perf.thread_started,
        classes_loaded: perf.classes_loaded,
        classes_unloaded: perf.classes_unloaded,
        gc_young_count: gc.gc_young_count,
        gc_young_time_ms: gc.gc_young_time_ms,
        gc_full_count: gc.gc_full_count,
        gc_full_time_ms: gc.gc_full_time_ms,
    })
}

/// 调用 JDK 自带的诊断工具（jstat / jcmd）。
///
/// `jdk_path` 存在时从其 `bin/` 取可执行文件（PATH 上没有也能用），否则回退 PATH。
/// 失败返回 `Err`，错误信息包含 exe 路径、退出码与子进程 stderr 摘要，
/// 供前端直接展示与日志排查——此前失败被静默吞掉，用户只能看到
/// 「需要完整 JDK」这种猜测性文案（2026-09-23 排查 online-srm-biz 误报时确定）。
fn run_jdk_tool(pid: u32, tool: &str, args: &[&str], jdk_path: Option<&str>) -> Result<String, String> {
    let exe_name = if cfg!(windows) {
        format!("{tool}.exe")
    } else {
        tool.to_string()
    };
    let exe = jdk_path
        .map(|p| Path::new(p).join("bin").join(&exe_name))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from(&exe_name));

    let mut cmd = hidden(&exe);
    cmd.arg(pid.to_string()).args(args);
    let output = cmd
        .output()
        .map_err(|e| format!("{} 启动失败: {e}", exe.display()))?;
    if !output.status.success() {
        let detail = summarize_tool_error(&output.stdout, &output.stderr);
        let code = output.status.code().map(|c| c.to_string()).unwrap_or_else(|| "?".into());
        let msg = format!("{} {} 退出码 {code}：{detail}", exe.display(), args.join(" "));
        tracing::warn!("JVM 诊断工具调用失败: {msg}");
        return Err(msg);
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// 汇总子进程的错误输出：优先 stderr，为空则回退 stdout。
/// 压成单行并截断到 200 字符（会进前端错误提示与日志）。
fn summarize_tool_error(stdout: &[u8], stderr: &[u8]) -> String {
    let flat = |raw: &[u8]| String::from_utf8_lossy(raw).split_whitespace().collect::<Vec<_>>().join(" ");
    let msg = {
        let e = flat(stderr);
        if e.is_empty() { flat(stdout) } else { e }
    };
    if msg.is_empty() {
        "(无输出)".to_string()
    } else if msg.chars().count() > 200 {
        format!("{}…", msg.chars().take(200).collect::<String>())
    } else {
        msg
    }
}

/// `jstat -gc` 一次快照的解析结果（内存已换算为字节，时间已换算为毫秒）。
#[derive(Debug, Default, PartialEq)]
struct GcSnapshot {
    heap_used: u64,
    heap_committed: u64,
    non_heap_used: u64,
    non_heap_committed: u64,
    gc_young_count: u64,
    gc_young_time_ms: u64,
    gc_full_count: u64,
    gc_full_time_ms: u64,
}

/// 带 `key=value` 计数的解析结果。
#[derive(Debug, Default, PartialEq)]
struct PerfCounters {
    thread_count: usize,
    thread_daemon: usize,
    thread_peak: usize,
    thread_started: u64,
    classes_loaded: u64,
    classes_unloaded: u64,
}

/// 解析 `jstat -gc <pid>` 的两行输出（表头 + 一次快照）。
///
/// 列集合随 JDK 版本变化：JDK 8 为 `… YGC YGCT FGC FGCT GCT`，
/// JDK 17+ 在 `FGCT` 与 `GCT` 之间插入 `CGC CGCT`。故此处**按表头名建立
/// 列索引**再取值，而非按固定下标，避免版本差异导致错列。
///
/// 无法解析（输出为空、被截断、表头与数据列数不符、堆容量为 0）时返回 `None`。
fn parse_jstat_gc(output: &str) -> Option<GcSnapshot> {
    let mut lines = output.lines().filter(|l| !l.trim().is_empty());
    let header: Vec<&str> = lines.next()?.split_whitespace().collect();
    let values: Vec<&str> = lines.next()?.split_whitespace().collect();
    if header.is_empty() || header.len() != values.len() {
        return None;
    }

    let at = |name: &str| -> Option<f64> {
        header
            .iter()
            .position(|h| *h == name)
            .and_then(|i| values[i].parse::<f64>().ok())
    };
    // 内存列单位为 KB；`as` 是截断，先 round 避免 1.386*1000 这类浮点误差丢精度
    let kb = |name: &str| -> u64 { at(name).map(|v| (v * 1024.0).round() as u64).unwrap_or(0) };
    let ms = |name: &str| -> u64 { at(name).map(|v| (v * 1000.0).round() as u64).unwrap_or(0) };
    let count = |name: &str| -> u64 { at(name).map(|v| v.round() as u64).unwrap_or(0) };

    // 堆 = survivor0 + survivor1 + eden + old（实测该式与 GC.heap_info 的 total 一致）
    let heap_used = kb("S0U") + kb("S1U") + kb("EU") + kb("OU");
    let heap_committed = kb("S0C") + kb("S1C") + kb("EC") + kb("OC");
    // 堆容量为 0 说明列名不匹配或该 GC 未按代划分（如 ZGC 全为 `-`），不具备展示价值
    if heap_committed == 0 {
        return None;
    }

    Some(GcSnapshot {
        heap_used,
        heap_committed,
        non_heap_used: kb("MU"),
        non_heap_committed: kb("MC"),
        gc_young_count: count("YGC"),
        gc_young_time_ms: ms("YGCT"),
        gc_full_count: count("FGC"),
        gc_full_time_ms: ms("FGCT"),
    })
}

/// 从 `jcmd PerfCounter.print` 输出提取所需计数器。
///
/// 每行形如 `key=value`；字符串型计数器（如 `sun.gc.collector.0.name="…"`）
/// 会因 `parse::<f64>` 失败被跳过。
fn parse_perf_counters(output: &str) -> PerfCounters {
    let mut c = PerfCounters::default();
    for line in output.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let Ok(n) = value.trim().parse::<f64>() else {
            continue;
        };
        let n = n.max(0.0).round() as u64;
        match key.trim() {
            "java.threads.live" => c.thread_count = n as usize,
            "java.threads.daemon" => c.thread_daemon = n as usize,
            "java.threads.livePeak" => c.thread_peak = n as usize,
            "java.threads.started" => c.thread_started = n,
            "java.cls.loadedClasses" => c.classes_loaded = n,
            "java.cls.unloadedClasses" => c.classes_unloaded = n,
            _ => {}
        }
    }
    c
}

/// 从 `jcmd VM.flags` 输出提取 `-XX:MaxHeapSize=<字节>`。
fn parse_max_heap_size(output: &str) -> Option<u64> {
    output
        .split_whitespace()
        .find_map(|flag| flag.strip_prefix("-XX:MaxHeapSize="))
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机实测输出（JDK 8u492 / G1，运行中的 Spring Boot 应用）
    const JSTAT_GC_JDK8: &str = " S0C    S1C    S0U    S1U      EC       EU        OC         OU       MC     MU    CCSC   CCSU   YGC     YGCT    FGC    FGCT     GCT   \n 0.0   97280.0  0.0   97280.0 1223680.0 410624.0  776192.0   148992.0  156928.0 150072.8 16640.0 15439.0     23    1.386   0      0.000    1.386";

    #[test]
    fn jstat_jdk8_columns_are_parsed_to_bytes_and_ms() {
        let s = parse_jstat_gc(JSTAT_GC_JDK8).expect("JDK 8 输出应解析成功");
        assert_eq!(s.heap_used, 656_896 * 1024);
        assert_eq!(s.heap_committed, 2_097_152 * 1024);
        assert_eq!(s.non_heap_used, (150_072.8_f64 * 1024.0).round() as u64);
        assert_eq!(s.non_heap_committed, 156_928 * 1024);
        assert_eq!(s.gc_young_count, 23);
        assert_eq!(s.gc_young_time_ms, 1386);
        assert_eq!(s.gc_full_count, 0);
        assert_eq!(s.gc_full_time_ms, 0);
    }

    /// JDK 17+ 在 FGC/FGCT 之后插入 CGC/CGCT，按列名取值不应错位。
    #[test]
    fn jstat_jdk17_extra_columns_do_not_shift_values() {
        const OUT: &str = " S0C S1C S0U S1U EC EU OC OU MC MU CCSC CCSU YGC YGCT FGC FGCT CGC CGCT GCT\n 0.0 1024.0 0.0 1024.0 2048.0 1024.0 4096.0 2048.0 25600.0 24576.0 1024.0 900.0 5 0.120 1 0.500 2 0.050 0.670";
        let s = parse_jstat_gc(OUT).expect("JDK 17 输出应解析成功");
        assert_eq!(s.gc_young_count, 5);
        assert_eq!(s.gc_full_count, 1);
        assert_eq!(s.gc_full_time_ms, 500);
        assert_eq!(s.heap_used, 4096 * 1024);
        assert_eq!(s.non_heap_used, 24_576 * 1024);
    }

    /// 拿不到有效快照时必须返回 None，而不是给出一堆 0 让前端展示成真实数据。
    #[test]
    fn jstat_rejects_invalid_output() {
        assert!(parse_jstat_gc("").is_none(), "空输出");
        assert!(
            parse_jstat_gc("jstat: couldn't find pid 4424").is_none(),
            "错误信息只有一行，取不到数据行"
        );
        assert!(
            parse_jstat_gc("A B C\n1 2 3 4").is_none(),
            "表头与数据列数不符"
        );
        assert!(
            parse_jstat_gc("S0C S1C EC OC\n0 0 0 0").is_none(),
            "堆容量为 0（列名不匹配或 ZGC 全为短横线）"
        );
    }

    #[test]
    fn perf_counters_extract_threads_and_classes() {
        const OUT: &str = "java.cls.loadedClasses=22113\njava.threads.daemon=67\njava.threads.live=82\njava.threads.livePeak=82\njava.threads.started=96\nsun.gc.collector.0.name=\"G1 incremental collections\"\njava.cls.unloadedClasses=8";
        let c = parse_perf_counters(OUT);
        assert_eq!(c.thread_count, 82);
        assert_eq!(c.thread_daemon, 67);
        assert_eq!(c.thread_peak, 82);
        assert_eq!(c.thread_started, 96);
        assert_eq!(c.classes_loaded, 22113);
        assert_eq!(c.classes_unloaded, 8);
    }

    #[test]
    fn perf_counters_tolerate_empty_output() {
        assert_eq!(parse_perf_counters(""), PerfCounters::default());
    }

    /// 错误摘要：单行化、优先 stderr、200 字符截断——会直达前端与日志，
    /// 不能是多行原文或超长堆栈。
    #[test]
    fn tool_error_summary_is_single_line_and_truncated() {
        assert_eq!(summarize_tool_error(b"", b"35364 not found\n"), "35364 not found");
        // stderr 为空时回退 stdout
        assert_eq!(summarize_tool_error(b"Error attaching\n", b""), "Error attaching");
        // 两者皆空
        assert_eq!(summarize_tool_error(b"", b""), "(无输出)");
        // 多行压成单行 + 截断到 200
        let long = "x".repeat(500);
        let s = summarize_tool_error(b"", format!("{long}\nsecond line\n").as_bytes());
        assert_eq!(s.chars().count(), 201, "200 字符 + 省略号");
        assert!(!s.contains('\n'));
        assert!(s.ends_with('…'));
    }

    #[test]
    fn max_heap_size_extracted_from_vm_flags() {
        const OUT: &str = "-XX:CICompilerCount=4 -XX:InitialHeapSize=2147483648 -XX:MaxHeapSize=2147483648 -XX:MetaspaceSize=134217728";
        assert_eq!(parse_max_heap_size(OUT), Some(2_147_483_648));
        assert_eq!(parse_max_heap_size(""), None);
        assert_eq!(parse_max_heap_size("-XX:MaxHeapSize=0"), None);
    }
}
