//! 实机验证：**真实 Spring Boot fat jar** 的版本元信息（框架版本 / 官方 Java 兼容区间 / 构建 JDK）。
//!
//! 为什么必须拿真 jar 测：`Build-Jdk` 这个字段名是**错的**——Maven Archiver 3.5 起因构建
//! 不可复现弃用了它，默认只写 `Build-Jdk-Spec`。用构造的假 MANIFEST 测（自己写什么就断言什么）
//! 永远发现不了这个问题，而对真实 jar 读 `Build-Jdk` 会恒为 `None`、提示永不显示。
//! 实测三个样本（Maven JAR Plugin 3.2.2 / 3.4.2 打出）**都只有 `Build-Jdk-Spec`**。
//!
//! 同时锁住「上限按**小版本**查表」：2.7 到 21、3.5 到 25、4.0 到 25。
//! 若有人把它退回「按大版本一刀切」，2.7 的上限会漂到 25、4.0 会漂到 26，这里就红。
//!
//! | jar | Spring Boot | 官方 Java 区间 | Build-Jdk-Spec |
//! |---|---|---|---|
//! | demo-jdk8.jar  | 2.7.18 | 8–21  | 1.8 |
//! | demo-jdk17.jar | 3.5.16 | 17–25 | 17  |
//! | demo-jdk21.jar | 4.0.8  | 17–25 | 21  |
//!
//! 依赖本机资源，缺失则自动跳过。可用 `OPX_TEST_JARS` 覆盖目录
//! （默认 `C:\Users\Runic\Desktop\startjar`）。
//!
//! 运行：`cargo test --test live_jar_compat -- --ignored --nocapture`
#![cfg(windows)]

use std::path::PathBuf;

use opx_lib::services::springboot_manager::{
    jdk_range_for_spring_boot, parse_spring_boot_major, parse_spring_boot_minor, read_build_jdk,
    read_spring_boot_version,
};

fn jars_dir() -> PathBuf {
    std::env::var("OPX_TEST_JARS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"C:\Users\Runic\Desktop\startjar"))
}

/// 期望值表：(jar 文件名, Spring Boot 版本, 区间下限, 区间上限, 构建 JDK)
const CASES: [(&str, &str, u32, u32, u32); 3] = [
    ("demo-jdk8.jar", "2.7.18", 8, 21, 8),
    ("demo-jdk17.jar", "3.5.16", 17, 25, 17),
    ("demo-jdk21.jar", "4.0.8", 17, 25, 21),
];

#[test]
#[ignore]
fn real_jars_report_expected_compat_metadata() {
    let dir = jars_dir();
    let mut checked = 0;
    let mut failures: Vec<String> = Vec::new();

    for (name, sb, min, max, build) in CASES {
        let jar = dir.join(name);
        if !jar.exists() {
            println!("跳过 {name}：{} 不存在", jar.display());
            continue;
        }
        let path = jar.to_string_lossy().to_string();

        // ① 框架版本（MANIFEST 的 Spring-Boot-Version）
        let got_sb = read_spring_boot_version(&path);
        if got_sb.as_deref() != Some(sb) {
            failures.push(format!("{name}: Spring-Boot-Version 期望 {sb}，实际 {got_sb:?}"));
            continue;
        }

        // ② 官方兼容区间：下限按大版本、上限按小版本
        let major = parse_spring_boot_major(sb).expect("大版本应可解析");
        let minor = parse_spring_boot_minor(sb).expect("小版本应可解析");
        let range = jdk_range_for_spring_boot(major, minor).expect("区间应可判定");
        if range != (min, Some(max)) {
            failures.push(format!("{name}: 区间期望 ({min}, Some({max}))，实际 {range:?}"));
        }

        // ③ 构建 JDK：真实字段是 `Build-Jdk-Spec`，不是 `Build-Jdk`
        let got_build = read_build_jdk(&path);
        if got_build != Some(build) {
            failures.push(format!(
                "{name}: 构建 JDK 期望 {build}，实际 {got_build:?}（查一下 Build-Jdk-Spec 是否还能读到）"
            ));
        }

        println!(
            "{name}: Spring Boot {sb} · 官方兼容 Java {}–{} · 构建 JDK {:?}",
            range.0,
            range.1.map(|v| v.to_string()).unwrap_or_else(|| "?".into()),
            got_build
        );
        checked += 1;
    }

    if checked == 0 {
        println!("跳过：{} 下没有样本 jar", dir.display());
        return;
    }
    assert!(failures.is_empty(), "真实 jar 元信息不符：\n  {}", failures.join("\n  "));
    println!("\n{checked} 个真实 jar 的元信息全部符合预期");
}
