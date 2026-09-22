use crate::models::springboot::JvmInfo;
use crate::utils::process::hidden;

/// 通过 jcmd 采集 JVM 指标
/// `jdk_path: Some(path)` 时从 JDK 目录找 jcmd，否则走 PATH
pub fn collect_jvm_metrics(pid: u32, jdk_path: Option<String>) -> Option<JvmInfo> {
    let heap_output = run_jcmd(pid, "GC.heap_info", jdk_path.as_deref())?;
    let (heap_used, heap_max, non_heap_used) = parse_heap_info(&heap_output);

    // ponytail: jcmd Thread.print 输出中统计线程数
    let thread_output = run_jcmd(pid, "Thread.print", jdk_path.as_deref()).unwrap_or_default();
    let thread_count = thread_output.lines().count().max(1) - 1; // 粗略估算，减去首行

    Some(JvmInfo {
        heap_used: heap_used.unwrap_or(0),
        heap_max: heap_max.unwrap_or(0),
        non_heap_used: non_heap_used.unwrap_or(0),
        thread_count,
        gc_count: 0,
        gc_time: 0,
    })
}

fn run_jcmd(pid: u32, command: &str, jdk_path: Option<&str>) -> Option<String> {
    // ponytail: 优先从 JDK 目录找 jcmd，PATH 上没有 jcmd 也能用
    let jcmd = jdk_path
        .map(|p| std::path::Path::new(p).join("bin").join(if cfg!(windows) { "jcmd.exe" } else { "jcmd" }))
        .filter(|p| p.exists());
    let jcmd = jcmd.as_deref().unwrap_or(std::path::Path::new(
        if cfg!(windows) { "jcmd.exe" } else { "jcmd" }
    ));
    let output = hidden(jcmd)
        .args([&pid.to_string(), command])
        .output()
        .ok()?;
    if !output.status.success() { return None; }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

fn parse_heap_info(output: &str) -> (Option<u64>, Option<u64>, Option<u64>) {
    let mut heap_used = None;
    let mut heap_max = None;
    let mut metaspace_used = None;
    for line in output.lines() {
        let line = line.trim();
        // Metaspace       used 12345K, capacity 23456K
        if line.starts_with("Metaspace") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            for (i, part) in parts.iter().enumerate() {
                if *part == "used" && i + 1 < parts.len() {
                    metaspace_used = Some(parse_memory_value(parts[i + 1]));
                }
            }
        }
        if line.contains("used") && (line.contains("total") || line.contains("capacity")) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            for (i, part) in parts.iter().enumerate() {
                if *part == "used" && i + 1 < parts.len() {
                    let val = parse_memory_value(parts[i + 1]);
                    if val > 0 {
                        if let Some(pos) = parts.iter().position(|&p| p == "total") {
                            if pos + 1 < parts.len() {
                                heap_max = Some(parse_memory_value(parts[pos + 1]));
                            }
                        }
                        heap_used = Some(val);
                        return (heap_used, heap_max, metaspace_used);
                    }
                }
            }
        }
    }
    (heap_used, heap_max, metaspace_used)
}

/// 解析 "123456K" / "1024M" / "2G" 格式为字节数
fn parse_memory_value(s: &str) -> u64 {
    let s = s.trim_end_matches(',');
    if let Some(val) = s.strip_suffix('K') {
        val.parse::<u64>().ok().map(|v| v * 1024).unwrap_or(0)
    } else if let Some(val) = s.strip_suffix('M') {
        val.parse::<u64>().ok().map(|v| v * 1024 * 1024).unwrap_or(0)
    } else if let Some(val) = s.strip_suffix('G') {
        val.parse::<u64>().ok().map(|v| v * 1024 * 1024 * 1024).unwrap_or(0)
    } else {
        s.parse::<u64>().ok().unwrap_or(0)
    }
}
