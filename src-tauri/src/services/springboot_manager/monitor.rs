use crate::models::springboot::JvmInfo;

/// 通过 jcmd 采集 JVM 指标
pub fn collect_jvm_metrics(pid: u32) -> Option<JvmInfo> {
    let heap_output = run_jcmd(pid, "GC.heap_info")?;
    let (heap_used, heap_max) = parse_heap_info(&heap_output);

    Some(JvmInfo {
        heap_used: heap_used.unwrap_or(0),
        heap_max: heap_max.unwrap_or(0),
        non_heap_used: 0,
        thread_count: 0,
        gc_count: 0,
        gc_time: 0,
    })
}

fn run_jcmd(pid: u32, command: &str) -> Option<String> {
    let jcmd = if cfg!(windows) { "jcmd.exe" } else { "jcmd" };
    let pid_str = pid.to_string();
    let output = std::process::Command::new(jcmd)
        .args([pid_str.as_str(), command])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

fn parse_heap_info(output: &str) -> (Option<u64>, Option<u64>) {
    let mut heap_used = None;
    let mut heap_max = None;
    for line in output.lines() {
        let line = line.trim();
        if line.contains("used") && (line.contains("total") || line.contains("capacity")) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            for (i, part) in parts.iter().enumerate() {
                if **part == "used" && i + 1 < parts.len() {
                    let val = parse_memory_value(parts[i + 1]);
                    if val > 0 {
                        if let Some(pos) = parts.iter().position(|&p| p == "total") {
                            if pos + 1 < parts.len() {
                                heap_max = parse_memory_value(parts[pos + 1]);
                            }
                        }
                        heap_used = Some(val);
                        return (heap_used, heap_max);
                    }
                }
            }
        }
        if line.starts_with("Metaspace") && line.contains("used") {
            // ponytail: 非堆内存采集，需要时添加
        }
    }
    (heap_used, heap_max)
}

/// 解析 "123456K" / "1024M" / "2G" 格式为字节数
fn parse_memory_value(s: &str) -> u64 {
    let s = s.trim_end_matches(',');
    if let Some(val) = s.strip_suffix('K') {
        val.parse::<u64>().ok().map(|v| v * 1024).unwrap_or(0)
    } else if let Some(val) = s.strip_suffix('M') {
        val.parse::<u64>().ok().map(|v| v * 1024 * 1024).unwrap_or(0)
    } else if let Some(val) = s.strip_suffix('G') {
        val.parse::<u64>()
            .ok()
            .map(|v| v * 1024 * 1024 * 1024)
            .unwrap_or(0)
    } else {
        s.parse::<u64>().ok().unwrap_or(0)
    }
}
