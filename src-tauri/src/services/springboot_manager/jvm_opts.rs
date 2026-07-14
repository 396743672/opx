use crate::models::springboot::JvmOptsTemplate;

/// 根据 JDK 安装路径检测版本并生成优化参数
pub fn detect_jdk_version(jdk_path: &str) -> Option<u32> {
    let java_bin = std::path::Path::new(jdk_path).join("bin").join("java");
    let java_bin = if cfg!(windows) {
        let mut p = java_bin.to_path_buf();
        p.set_extension("exe");
        if p.exists() { p } else { java_bin.to_path_buf() }
    } else {
        java_bin
    };
    if !java_bin.exists() { return None; }
    let output = std::process::Command::new(&java_bin)
        .arg("-version")
        .output()
        .ok()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let first_line = stderr.lines().next()?;
    // "1.8.0" -> 8
    let full = first_line.trim();
    // Try "version \"1.8.0\"" -> 8
    if full.contains("\"1.") {
        if let Some(start) = full.find('"') {
            let inner = &full[start+1..];
            if let Some(end) = inner.find('"') {
                let ver = &inner[..end];
                if ver.starts_with("1.") {
                    if let Some(dot) = ver[2..].find('.') {
                        if let Ok(v) = ver[2..2+dot].parse::<u32>() {
                            // workaround: "1.8.0" -> v=8
                            return Some(v);
                        }
                    }
                }
            }
        }
    }
    // "11.0.1" -> 11
    if let Some(start) = full.find('"') {
        let inner = &full[start+1..];
        if let Some(end) = inner.find('"') {
            let ver = &inner[..end];
            if let Some(dot) = ver.find('.') {
                if let Ok(v) = ver[..dot].parse::<u32>() {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// 获取本机物理内存 MB
fn total_ram_mb() -> u64 {
    let system = sysinfo::System::new_all();
    system.total_memory() / 1024 / 1024
}

/// 根据 JDK 版本和本机内存生成默认优化参数
///
/// 为 Spring Boot 应用设计的保守默认值：
/// - Xmx = 本机 RAM 的 25%，上限 2GB，下限 256MB（Spring Boot 典型场景不需要更大）
/// - Xms = Xmx（相等防止 GC 调整堆大小导致停顿）
/// - Metaspace = 128MB（JDK17+ 类元数据更多，给 256MB）
pub fn generate_opts(jdk_version: u32) -> JvmOptsTemplate {
    let total_mb = total_ram_mb();
    let xmx_mb = (total_mb / 4).min(2048).max(256);
    let xms_mb = xmx_mb; // 相等避免 GC resize 停顿
    let metaspace_mb = if jdk_version >= 17 { 256 } else { 128 };

    let (gc_type, mut extra_flags) = match jdk_version {
        8 => ("G1GC".to_string(), vec![]),
        11 | 12 | 13 | 14 | 15 | 16 => {
            ("G1GC".to_string(), vec!["-XX:+UseStringDeduplication".to_string()])
        }
        v if v >= 21 => {
            ("ZGC".to_string(), vec![
                "-XX:+UseZGC".to_string(),
                "-XX:+ZGenerational".to_string(),
            ])
        }
        _ => {
            ("ZGC".to_string(), vec!["-XX:+UseZGC".to_string()])
        }
    };

    extra_flags.push("-XX:+ExitOnOutOfMemoryError".to_string());
    extra_flags.push("-XX:+HeapDumpOnOutOfMemoryError".to_string());
    extra_flags.push("-Dfile.encoding=UTF-8".to_string());

    JvmOptsTemplate {
        xms_mb,
        xmx_mb,
        metaspace_mb,
        gc_type,
        extra_flags,
    }
}

