use std::path::{Path, PathBuf};

use crate::models::springboot::{GcOption, GcUnsupportedReason, JvmOptsTemplate};
use crate::utils::process::hidden;

use super::parse_java_major;

/// `<home>/bin/java`（Windows 下优先 `java.exe`）
fn java_bin(home: &Path) -> PathBuf {
    let base = home.join("bin").join("java");
    if cfg!(windows) {
        let exe = base.with_extension("exe");
        if exe.exists() {
            return exe;
        }
    }
    base
}

/// 跑一次 `java -version` 并返回它写到 stderr 的全文（JVM 一贯写 stderr）。
fn java_version_output(home: &Path) -> Option<String> {
    let java = java_bin(home);
    if !java.exists() {
        return None;
    }
    let output = hidden(&java).arg("-version").output().ok()?;
    Some(String::from_utf8_lossy(&output.stderr).into_owned())
}

/// 根据 JDK 安装路径检测版本并生成优化参数
pub fn detect_jdk_version(jdk_path: &str) -> Option<u32> {
    let stderr = java_version_output(Path::new(jdk_path))?;
    let first_line = stderr.lines().next()?;
    // 形如 `openjdk version "21.0.11" 2026-04-21 LTS`，取引号内的版本串。
    // 版本串→主版本的归一化统一走 parse_java_major（1.8.0 → 8），避免两套解析规则漂移。
    let inner = &first_line[first_line.find('"')? + 1..];
    parse_java_major(&inner[..inner.find('"')?])
}

/// 该运行时是不是 **Oracle 构建**（Oracle JDK 与 Oracle 的 OpenJDK 构建都算）。
///
/// 只为一件事服务：**Oracle 的任何版本都不带 Shenandoah GC**。
/// OpenJDK 官方 wiki（Shenandoah GC → Releases）原文：
///
/// > Oracle Does not ship Shenandoah in any release, both OpenJDK builds and proprietary builds
///
/// 这一条纯版本判断看不出来：同一份 JDK 21，Corretto / Zulu / Temurin / Liberica 能选
/// Shenandoah，Oracle 选了必定 `Unrecognized VM option 'UseShenandoahGC'` **拒绝启动**。
///
/// 两条判据任一命中即认定，且**只在有明确证据时才判 Oracle**——判错会把本来能用的
/// 选项禁掉，比漏判更糟（漏判顶多是用户选了之后启动失败，日志里原因很清楚）：
/// 1. `release` 文件的 `IMPLEMENTOR` / `JAVA_VENDOR` 含 `oracle`（JDK 都有该文件）
/// 2. 兜底跑一次 `java -version`，输出含 `Java(TM) SE Runtime Environment`
///    （Oracle 私有构建的标志串；各家 OpenJDK 构建一律是 `OpenJDK Runtime Environment`）。
///    没有 `release` 文件的运行时（如 JRE 8）只能走这条。
pub fn is_oracle_runtime(home: &Path) -> bool {
    if let Ok(text) = std::fs::read_to_string(home.join("release")) {
        if release_is_oracle(&text) {
            return true;
        }
    }
    java_version_output(home)
        .map(|out| banner_is_oracle(&out))
        .unwrap_or(false)
}

/// 判据一：`release` 文件里的 `IMPLEMENTOR` / `JAVA_VENDOR` 是否指向 Oracle。
///
/// `release` 是 `KEY="value"` 逐行的 properties 格式（JRE 8 没有这个文件）。
fn release_is_oracle(text: &str) -> bool {
    text.lines().any(|line| {
        let Some((key, value)) = line.split_once('=') else {
            return false;
        };
        matches!(key.trim(), "IMPLEMENTOR" | "JAVA_VENDOR")
            && value.to_ascii_lowercase().contains("oracle")
    })
}

/// 判据二：`java -version` 输出是否来自 Oracle 私有构建。
///
/// Oracle 固定打印 `Java(TM) SE Runtime Environment`，各家 OpenJDK 构建
/// （Temurin / Zulu / Corretto / Liberica…）一律打印 `OpenJDK Runtime Environment`——
/// 这个 `(TM)` 就是二者的分水岭。
fn banner_is_oracle(text: &str) -> bool {
    text.contains("Java(TM) SE Runtime Environment")
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

/// 该 GC 在给定 JDK 上不可用的原因（`None` = 可用）。
///
/// ⚠️ 两条实测/权威依据：
/// 1. **按版本门禁**：JDK 8 传 `-XX:+UseZGC` / `-XX:+UseShenandoahGC` 会 `Unrecognized
///    VM option` 并拒绝启动，所以必须禁用而不是只提示。
/// 2. **按厂商门禁**：Shenandoah 由 Red Hat 主导，**Oracle 的任何构建都不包含它**
///    （见 [`is_oracle_runtime`] 引用的 OpenJDK wiki 原文）。这里之前只按版本判断，
///    结果 Oracle JDK 21 的用户可以选中 Shenandoah，一启动就失败。
fn gc_unsupported_reason(
    gc: &str,
    jdk_major: u32,
    oracle_runtime: bool,
) -> Option<GcUnsupportedReason> {
    match gc {
        "ZGC" if jdk_major < 11 => Some(GcUnsupportedReason::Version),
        "ShenandoahGC" if jdk_major < 12 => Some(GcUnsupportedReason::Version),
        "ShenandoahGC" if oracle_runtime => Some(GcUnsupportedReason::Vendor),
        _ => None,
    }
}

/// 供表单渲染的 GC 目录：每个选项是否可用、不可用的原因、是否为该 JDK 的推荐值。
///
/// 与 [`generate_opts`] 同源（都用 [`recommended_gc_type`]），避免出现
/// 「推荐的」和「能选的」两套说法。
pub fn gc_catalog(jdk_major: u32, oracle_runtime: bool) -> Vec<GcOption> {
    let recommended = recommended_gc_type(jdk_major);
    GC_CHOICES
        .iter()
        .map(|name| {
            let reason = gc_unsupported_reason(name, jdk_major, oracle_runtime);
            GcOption {
                name: (*name).to_string(),
                supported: reason.is_none(),
                recommended: *name == recommended,
                unsupported_reason: reason,
            }
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
/// `oracle_runtime` 只影响 GC 目录里的可用性标注（Oracle 不含 Shenandoah），
/// 推荐值本身只由版本决定。
///
/// 为 Spring Boot 应用设计的保守默认值：
/// - Xmx = 本机 RAM 的 25%，上限 2GB，下限 256MB（Spring Boot 典型场景不需要更大）
/// - Xms = Xmx（相等防止 GC 调整堆大小导致停顿）
/// - Metaspace = 128MB（JDK17+ 类元数据更多，给 256MB）
pub fn generate_opts(jdk_version: u32, oracle_runtime: bool) -> JvmOptsTemplate {
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
        gc_options: gc_catalog(jdk_version, oracle_runtime),
    }
}

#[cfg(test)]
mod tests {
    use super::{gc_catalog, generate_opts, is_oracle_runtime, recommended_gc_type};
    use crate::models::springboot::GcUnsupportedReason;

    /// 各测试默认用「非 Oracle 运行时」——推荐值与可用性都不受厂商影响，
    /// 厂商差异由 `catalog_blocks_shenandoah_on_oracle_runtime` 单独覆盖。
    const OTHER: bool = false;

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
            let t = generate_opts(v, OTHER);
            assert_eq!(t.gc_type, "ZGC", "JDK {v}");
            assert!(
                t.extra_flags.contains(&"-XX:+ZGenerational".to_string()),
                "JDK {v} 需要分代开关，否则 ZGC 走非分代模式"
            );
        }
        for v in [24, 25, 26] {
            let t = generate_opts(v, OTHER);
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
            let t = generate_opts(v, OTHER);
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
        let cat = gc_catalog(8, OTHER);
        let find = |n: &str| cat.iter().find(|o| o.name == n).expect("选项应存在");
        assert!(!find("ZGC").supported, "JDK 8 不支持 ZGC");
        assert!(!find("ShenandoahGC").supported, "JDK 8 不支持 Shenandoah");
        assert_eq!(
            find("ZGC").unsupported_reason,
            Some(GcUnsupportedReason::Version),
            "JDK 8 的 ZGC 是版本原因"
        );
        for name in ["G1GC", "ParallelGC", "SerialGC"] {
            assert!(find(name).supported, "JDK 8 应支持 {name}");
            assert_eq!(find(name).unsupported_reason, None, "{name} 不该带原因");
        }
        assert!(find("G1GC").recommended, "JDK 8 的推荐值应是 G1GC");
    }

    /// 同一份 JDK 21：非 Oracle 构建五个 GC 全可用，**Oracle 构建必须禁掉 Shenandoah**。
    ///
    /// 依据 OpenJDK wiki 原文：*Oracle Does not ship Shenandoah in any release,
    /// both OpenJDK builds and proprietary builds*。之前只按版本判断，
    /// 于是 Oracle JDK 21 的用户能选中它，一启动就 `Unrecognized VM option`。
    #[test]
    fn catalog_blocks_shenandoah_on_oracle_runtime() {
        let oracle = true;
        for v in [12, 17, 21, 25] {
            let cat = gc_catalog(v, oracle);
            let sh = cat
                .iter()
                .find(|o| o.name == "ShenandoahGC")
                .expect("选项应存在");
            assert!(!sh.supported, "Oracle JDK {v} 不含 Shenandoah");
            assert_eq!(
                sh.unsupported_reason,
                Some(GcUnsupportedReason::Vendor),
                "Oracle JDK {v} 的 Shenandoah 是厂商原因，不是版本原因"
            );
            assert!(cat.iter().any(|o| o.name == "G1GC" && o.supported));
            assert!(cat.iter().any(|o| o.name == "ZGC" && o.supported));
        }

        // 版本不够时，原因应以版本为准（更根本，用户换发行版也没用）
        let old = gc_catalog(8, oracle);
        let sh = old.iter().find(|o| o.name == "ShenandoahGC").unwrap();
        assert_eq!(sh.unsupported_reason, Some(GcUnsupportedReason::Version));
    }

    /// JDK 17 起所有选项都可用（实测 Temurin/Adoptium 17/21 五个 GC 全部接受）
    #[test]
    fn catalog_all_supported_from_jdk_17() {
        for v in [17, 21, 25] {
            let cat = gc_catalog(v, OTHER);
            assert!(
                cat.iter().all(|o| o.supported),
                "JDK {v} 应支持全部 GC 选项"
            );
            assert!(
                cat.iter().all(|o| o.unsupported_reason.is_none()),
                "JDK {v} 全部可用时不该带不支持原因"
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
            let t = generate_opts(v, OTHER);
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

    /// 本机实测：`D:\jdks` 下都是 BellSoft Liberica（非 Oracle），
    /// 而本机任一 JDK 的 `release` 文件里都没有 `Oracle`。
    /// 这条同时确认 `release` 快路径不会把普通 OpenJDK 构建误判成 Oracle——
    /// **误判的代价是把能用的 Shenandoah 禁掉**。
    #[test]
    fn detects_oracle_runtime_without_false_positives() {
        let candidates = [
            std::env::var("OPX_TEST_JDK").unwrap_or_else(|_| r"D:\jdks\jdk8u492-full".into()),
            std::env::var("OPX_TEST_JDK21").unwrap_or_else(|_| r"D:\jdks\jdk-21.0.11".into()),
        ];
        let mut checked = 0;
        for dir in candidates.iter().map(std::path::Path::new) {
            if !dir.join("bin").exists() {
                continue;
            }
            checked += 1;
            assert!(
                !is_oracle_runtime(dir),
                "{} 不是 Oracle 构建，不该被判成 Oracle",
                dir.display()
            );
        }
        // 本机没有样本时不算失败（保持测试可在任意机器上跑）
        if checked == 0 {
            eprintln!("跳过：未找到测试用 JDK（可用 OPX_TEST_JDK 指定）");
        }
    }

    /// `release` 文件缺失时退回解析 `java -version` 输出，
    /// 所以两条判据都要能独立生效、且都不误伤普通 OpenJDK 构建。
    #[test]
    fn oracle_is_detected_from_release_and_version_banner() {
        use super::{banner_is_oracle, release_is_oracle};

        // 判据一：release 文件。Oracle JDK 8 的 IMPLEMENTOR 是 "Oracle Corporation"
        assert!(release_is_oracle("JAVA_VERSION=\"1.8.0_504\"\nIMPLEMENTOR=\"Oracle Corporation\"\n"));
        assert!(release_is_oracle("JAVA_VENDOR=\"Oracle Corporation\"\n"));
        // 真实样本（本机 D:\jdks 全是 BellSoft Liberica）
        assert!(!release_is_oracle("JAVA_VERSION=\"21.0.11\"\nIMPLEMENTOR=\"BellSoft\"\n"));
        assert!(!release_is_oracle("JAVA_VERSION=\"21.0.11\"\nIMPLEMENTOR=\"Eclipse Adoptium\"\n"));
        assert!(!release_is_oracle("JAVA_VERSION=\"21.0.11\"\nIMPLEMENTOR=\"Amazon.com Inc.\"\n"));
        // 没有 release 文件内容 / 无关键都不该命中
        assert!(!release_is_oracle(""));
        assert!(!release_is_oracle("JAVA_VERSION=\"21.0.11\"\n"));

        // 判据二：java -version 输出
        assert!(banner_is_oracle(
            "java version \"21.0.11\" 2026-04-21 LTS\n\
             Java(TM) SE Runtime Environment (build 21.0.11+10-LTS-92)\n\
             Java HotSpot(TM) 64-Bit Server VM (build 21.0.11+10-LTS-92, mixed mode)"
        ));
        for banner in [
            "openjdk version \"21.0.11\" 2026-04-21 LTS\nOpenJDK Runtime Environment (build 21.0.11+10-LTS)",
            // Temurin
            "openjdk version \"17.0.20\" 2026-07-15\nOpenJDK Runtime Environment Temurin-17.0.20+8 (build 17.0.20+8)",
            // Zulu
            "openjdk version \"21.0.11\" 2026-04-21 LTS\nOpenJDK Runtime Environment Zulu21.46+19-CA (build 21.0.11+10-LTS)",
            // Liberica（本机实际使用的发行版）
            "openjdk version \"21.0.11\" 2026-04-21 LTS\nOpenJDK Runtime Environment (build 21.0.11+10-LTS)\nOpenJDK 64-Bit Server VM (build 21.0.11+10-LTS, mixed mode, sharing)",
        ] {
            assert!(
                !banner_is_oracle(banner),
                "OpenJDK 构建不该被判成 Oracle: {banner}"
            );
        }
    }

    /// `is_oracle_runtime` 的快路径：`release` 文件命中即返回，
    /// 连 `java -version` 都不用跑（所以这个临时目录**不需要真的放 java**）。
    #[test]
    fn is_oracle_runtime_reads_implementor_from_release_file() {
        let dir = std::env::temp_dir().join(format!("opx-oracle-probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建临时目录");
        std::fs::write(
            dir.join("release"),
            "JAVA_VERSION=\"1.8.0_504\"\nIMPLEMENTOR=\"Oracle Corporation\"\n",
        )
        .expect("写 release");

        assert!(
            is_oracle_runtime(&dir),
            "release 里写 Oracle 就该判成 Oracle（这条覆盖 Oracle JDK 8 的实际形状）"
        );

        // 换成普通发行版 → 必须回落
        std::fs::write(
            dir.join("release"),
            "JAVA_VERSION=\"21.0.11\"\nIMPLEMENTOR=\"BellSoft\"\n",
        )
        .expect("改写 release");
        assert!(!is_oracle_runtime(&dir), "BellSoft 构建不该被判成 Oracle");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

