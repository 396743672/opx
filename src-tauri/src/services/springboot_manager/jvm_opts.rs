use crate::models::springboot::{GcOption, JvmOptsTemplate};
use crate::utils::process::hidden;

use super::parse_java_major;

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
    let output = hidden(&java_bin)
        .arg("-version")
        .output()
        .ok()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let first_line = stderr.lines().next()?;
    // 形如 `openjdk version "21.0.11" 2026-04-21 LTS`，取引号内的版本串。
    // 版本串→主版本的归一化统一走 parse_java_major（1.8.0 → 8），避免两套解析规则漂移。
    let inner = &first_line[first_line.find('"')? + 1..];
    parse_java_major(&inner[..inner.find('"')?])
}

/// 该 JDK 主版本**推荐**的 GC 类型（也是生成默认参数时用的值）。
///
/// - **8–20 → G1GC**：JDK 9 起 G1 就是 JVM 自身默认；JDK 8 上 G1 同样稳定（官方 8 的
///   GC 调优指南对 4G 以上堆推荐它）。而 ZGC / Shenandoah 在 8 上**根本不存在**——
///   实测传 `-XX:+UseZGC` 会打印 `Unrecognized VM option` 并**拒绝启动**。
/// - **21+ → ZGC**：JDK 21 引入分代 ZGC（JEP 439）、23 转正、24 移除非分代模式，
///   停顿可压到 1ms 内。17–20 虽然有 ZGC，但那时还没有分代模式，通用场景 G1 更划算。
pub fn recommended_gc_type(jdk_major: u32) -> &'static str {
    if jdk_major >= 21 { "ZGC" } else { "G1GC" }
}

/// 下拉框里的全部 GC 选项，顺序即展示顺序
const GC_CHOICES: [&str; 5] = ["G1GC", "ParallelGC", "ZGC", "ShenandoahGC", "SerialGC"];

/// 该 GC 在给定 JDK 上是否可用。
///
/// ⚠️ 两条实测事实：
/// 1. **按版本门禁**：JDK 8 传 `-XX:+UseZGC` / `-XX:+UseShenandoahGC` 会 `Unrecognized
///    VM option` 并拒绝启动，所以必须禁用而不是只提示。
/// 2. **厂商差异**：Shenandoah 由 Red Hat 主导，**Oracle JDK 任何版本都不包含它**
///    （Temurin / Zulu / Corretto / RHEL 的构建才有）。仅凭版本判不出厂商，
///    这里只按版本判断；Oracle JDK 用户选它仍会启动失败。
fn gc_supported(gc: &str, jdk_major: u32) -> bool {
    match gc {
        "ZGC" => jdk_major >= 11,
        "ShenandoahGC" => jdk_major >= 12,
        _ => true,
    }
}

/// 供表单渲染的 GC 目录：每个选项是否可用、是否为该 JDK 的推荐值。
///
/// 与 [`generate_opts`] 同源（都用 [`recommended_gc_type`]），避免出现
/// 「推荐的」和「能选的」两套说法。
pub fn gc_catalog(jdk_major: u32) -> Vec<GcOption> {
    let recommended = recommended_gc_type(jdk_major);
    GC_CHOICES
        .iter()
        .map(|name| GcOption {
            name: (*name).to_string(),
            supported: gc_supported(name, jdk_major),
            recommended: *name == recommended,
        })
        .collect()
}

/// 某个 GC 在给定 JDK 上还需要哪些额外开关。
fn gc_flags(gc_type: &str, jdk_version: u32) -> Vec<String> {
    match gc_type {
        // 分代 ZGC：开关只存在于 JDK 21–23（JEP 439 引入、23 起成为默认、JEP 490 在 24 移除）。
        // 24+ 再传只会打印 `Ignoring option ZGenerational; support was removed in 24.0`，
        // 且该选项已被标记为将来会过期——届时 JVM 会直接拒绝启动。
        "ZGC" if (21..=23).contains(&jdk_version) => {
            vec!["-XX:+UseZGC".to_string(), "-XX:+ZGenerational".to_string()]
        }
        "ZGC" => vec!["-XX:+UseZGC".to_string()],
        // G1 的字符串去重（JDK 8u20 起可用，11 以后更稳定）
        "G1GC" if jdk_version >= 11 => vec!["-XX:+UseStringDeduplication".to_string()],
        _ => vec![],
    }
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

    let gc_type = recommended_gc_type(jdk_version).to_string();
    let mut extra_flags = gc_flags(&gc_type, jdk_version);

    extra_flags.push("-XX:+ExitOnOutOfMemoryError".to_string());
    extra_flags.push("-XX:+HeapDumpOnOutOfMemoryError".to_string());
    extra_flags.push("-Dfile.encoding=UTF-8".to_string());

    JvmOptsTemplate {
        xms_mb,
        xmx_mb,
        metaspace_mb,
        gc_type,
        extra_flags,
        gc_options: gc_catalog(jdk_version),
    }
}

#[cfg(test)]
mod tests {
    use super::{gc_catalog, generate_opts, recommended_gc_type};

    /// 推荐值：8–20 用 G1（JVM 自身默认、通用最稳），21+ 才上分代 ZGC。
    /// 17–20 虽然有 ZGC，但那时还没分代模式，不该被推荐。
    #[test]
    fn recommended_gc_is_g1_until_jdk_20_then_zgc() {
        for v in [8, 9, 10, 11, 17, 20] {
            assert_eq!(recommended_gc_type(v), "G1GC", "JDK {v} 应推荐 G1GC");
        }
        for v in [21, 23, 24, 25, 26] {
            assert_eq!(recommended_gc_type(v), "ZGC", "JDK {v} 应推荐 ZGC");
        }
    }

    /// 分代 ZGC 的开关只该给 JDK 21–23：JDK 23 起分代成为默认，**JDK 24 移除了该开关**
    /// （JEP 490）。实测 JDK 25.0.3 传它只打印 `Ignoring option ZGenerational;
    /// support was removed in 24.0`（进程仍起），但该选项已被标记为将来过期——
    /// 届时 HotSpot 将不再识别它并**拒绝启动**。
    #[test]
    fn zgc_generational_flag_only_for_jdk_21_to_23() {
        for v in [21, 22, 23] {
            let t = generate_opts(v);
            assert_eq!(t.gc_type, "ZGC", "JDK {v}");
            assert!(
                t.extra_flags.contains(&"-XX:+ZGenerational".to_string()),
                "JDK {v} 需要分代开关，否则 ZGC 走非分代模式"
            );
        }
        for v in [24, 25, 26] {
            let t = generate_opts(v);
            assert_eq!(t.gc_type, "ZGC", "JDK {v}");
            assert!(
                !t.extra_flags.contains(&"-XX:+ZGenerational".to_string()),
                "JDK {v} 已无此开关，传了只会告警（且将来会导致 JVM 拒绝启动）"
            );
            assert!(
                t.extra_flags.contains(&"-XX:+UseZGC".to_string()),
                "JDK {v} 仍需 UseZGC"
            );
        }
    }

    /// JDK 9/10 尚不存在 ZGC（JDK 11 才引入）→ 必须回落 G1，
    /// 否则 JVM 直接以 `Unrecognized VM option 'UseZGC'` 拒绝启动。
    #[test]
    fn no_zgc_option_before_jdk_11() {
        for v in [8, 9, 10] {
            let t = generate_opts(v);
            assert_eq!(t.gc_type, "G1GC", "JDK {v}");
            assert!(
                t.extra_flags.iter().all(|f| !f.contains("ZGC")),
                "JDK {v} 不应出现任何 ZGC 相关参数"
            );
        }
    }

    /// 实测：JDK 8 传 `-XX:+UseZGC` / `-XX:+UseShenandoahGC` 会打印
    /// `Unrecognized VM option` 并**拒绝启动**（连 Spring 的 banner 都打不出来）。
    /// 所以这两个选项在 JDK 8 上必须被标成不可用，不能只给文字提示。
    #[test]
    fn catalog_disables_zgc_and_shenandoah_on_jdk_8() {
        let cat = gc_catalog(8);
        let find = |n: &str| cat.iter().find(|o| o.name == n).expect("选项应存在");
        assert!(!find("ZGC").supported, "JDK 8 不支持 ZGC");
        assert!(!find("ShenandoahGC").supported, "JDK 8 不支持 Shenandoah");
        for name in ["G1GC", "ParallelGC", "SerialGC"] {
            assert!(find(name).supported, "JDK 8 应支持 {name}");
        }
        assert!(find("G1GC").recommended, "JDK 8 的推荐值应是 G1GC");
    }

    /// JDK 17 起所有选项都可用（实测 Temurin/Adoptium 17/21 五个 GC 全部接受）
    #[test]
    fn catalog_all_supported_from_jdk_17() {
        for v in [17, 21, 25] {
            let cat = gc_catalog(v);
            assert!(
                cat.iter().all(|o| o.supported),
                "JDK {v} 应支持全部 GC 选项"
            );
            assert_eq!(
                cat.iter().filter(|o| o.recommended).count(),
                1,
                "JDK {v} 的推荐项必须唯一"
            );
            assert_eq!(
                cat.iter().find(|o| o.recommended).unwrap().name,
                recommended_gc_type(v),
            );
        }
    }

    /// 前端下拉框直接吃 `generate_opts` 里的目录，两者必须同源，
    /// 否则会出现「推荐 ZGC 但 ZGC 被禁用」这种自相矛盾的界面。
    #[test]
    fn generate_opts_ships_a_catalog_matching_the_recommendation() {
        for v in [8, 17, 21, 25] {
            let t = generate_opts(v);
            assert_eq!(
                t.gc_options.len(),
                5,
                "JDK {v} 的目录应含全部 5 个选项"
            );
            let rec = t
                .gc_options
                .iter()
                .find(|o| o.recommended)
                .expect("应有推荐项");
            assert_eq!(rec.name, t.gc_type, "JDK {v} 推荐项与 gc_type 必须一致");
            assert!(rec.supported, "JDK {v} 的推荐项必须可用");
        }
    }
}

