//! 实机验证 attach + agent 注入能让目标 JVM 自行退出。
//!
//! 覆盖两条路径：
//! 1. 同主版本：JDK 8 启动器 → JDK 8 目标
//! 2. **跨主版本**：JDK 8 启动器 → JDK 21 目标
//!
//! 第 2 条是 2026-09-24 实测推翻了「启动器必须与目标同主版本」之后新增的回归：
//! 跨版本时启动器会解析响应失败（`Non-numeric value found`），但命令**已送达并被
//! 执行**——所以判据只能是「目标执行了 shutdown hook 且进程自行退出」，
//! 绝不能看启动器退出码。若有人按旧结论把版本约束加回去，这个测试会失败。
//!
//! 运行：`cargo test --test live_agent_stop -- --ignored --nocapture`
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use opx_lib::services::springboot_manager::agent::{
    ensure_agent_jar, request_graceful_exit, InjectOutcome, Launcher,
};

fn env_home(var: &str, fallback: &str) -> PathBuf {
    std::env::var(var).map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(fallback))
}

fn jdk8_home() -> PathBuf {
    env_home("OPX_TEST_JDK", r"D:\jdks\jdk8u492-full")
}

fn jdk21_home() -> PathBuf {
    env_home("OPX_TEST_JDK21", r"D:\jdks\jdk-21.0.11")
}

const TARGET_SRC: &str = r#"
import java.io.FileWriter;

public class Target {
    public static void main(String[] a) throws Exception {
        Runtime.getRuntime().addShutdownHook(new Thread(() -> {
            try (FileWriter w = new FileWriter(a[0])) { w.write("hook-ran"); } catch (Exception ignored) {}
        }));
        try (FileWriter w = new FileWriter(a[1])) { w.write("started"); }
        Thread.sleep(300000);
    }
}
"#;

/// 用 `javac_home` 编译目标程序，起 JVM，注入，然后校验 hook 执行且进程自行退出。
///
/// `launcher` 是执行注入的一方，`target_home` 是被注入的 JVM——两者故意分开，
/// 以便覆盖跨主版本。
fn assert_injection_stops_jvm(label: &str, javac_home: &Path, launcher: Launcher, target_home: &Path) {
    let work = std::env::temp_dir().join(format!("opx_agent_live_{label}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();

    // 1. 编译一个带 shutdown hook 的目标程序
    std::fs::write(work.join("Target.java"), TARGET_SRC).unwrap();
    let javac = javac_home.join("bin").join("javac.exe");
    let tools = javac_home.join("lib").join("tools.jar");
    let mut cmd = std::process::Command::new(&javac);
    cmd.arg("-encoding").arg("UTF-8");
    if tools.exists() {
        // JDK 8 的 javac 需要 tools.jar 才看得到 com.sun.tools.attach；JDK 9+ 不需要
        cmd.arg("-cp").arg(&tools);
    }
    let out = cmd
        .arg("-d")
        .arg(&work)
        .arg(work.join("Target.java"))
        .output()
        .unwrap_or_else(|e| panic!("{label}: javac 无法执行（{}）: {e}", javac.display()));
    assert!(out.status.success(), "{label}: 目标编译失败: {}", String::from_utf8_lossy(&out.stderr));

    // 2. 起目标 JVM —— stdio 重定向 + CREATE_NO_WINDOW，与 opx 启动应用的方式一致
    let hook = work.join("hook.txt");
    let started = work.join("started.txt");
    let console = std::fs::File::create(work.join("console.log")).unwrap();
    let console_err = console.try_clone().unwrap();
    let mut cmd = std::process::Command::new(target_home.join("bin").join("java.exe"));
    cmd.arg("-cp")
        .arg(&work)
        .arg("Target")
        .arg(&hook)
        .arg(&started)
        .stdout(console)
        .stderr(console_err);
    opx_lib::utils::process::hide_console(&mut cmd);
    let mut child = cmd.spawn().expect("应能启动目标 JVM");
    let pid = child.id();

    let deadline = Instant::now() + Duration::from_secs(30);
    while !started.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(started.exists(), "{label}: 目标 JVM 未就绪");

    // 3. 注入（先把内嵌的 agent jar 落到磁盘）
    ensure_agent_jar().expect("agent jar 应落盘");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    match runtime.block_on(request_graceful_exit(pid, launcher)) {
        Ok(InjectOutcome::Accepted) => println!("{label}: 注入被接受（启动器退出码 0）"),
        // 跨主版本时客户端解析响应会失败，但命令已送达 —— 不视为失败
        Ok(InjectOutcome::Failed(why)) => println!("{label}: 启动器报错但命令可能已送达 → {why}"),
        Err(e) => panic!("{label}: 注入根本不可用: {e}"),
    }

    // 4. shutdown hook 必须执行 —— 这才是「优雅」的判据
    let deadline = Instant::now() + Duration::from_secs(20);
    while !hook.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let hook_ran = hook.exists();

    // 5. 执行完 hook 后进程应自行消失（没有任何强杀）
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut exited = false;
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            exited = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if !exited {
        let _ = child.kill();
    }
    let _ = std::fs::remove_dir_all(&work);

    assert!(hook_ran, "{label}: shutdown hook 未执行 → 不是优雅停止");
    assert!(exited, "{label}: 执行 hook 后目标 JVM 仍未自行退出");
    println!("✅ {label}: shutdown hook 已执行，进程自行退出（未经强杀）");
}

fn jdk8_launcher() -> Launcher {
    let home = jdk8_home();
    Launcher {
        java: home.join("bin").join("java.exe"),
        // JDK 8 的 attach API 在 lib/tools.jar 里
        classpath_prefix: Some(home.join("lib").join("tools.jar")),
    }
}

#[test]
#[ignore = "需要本机 JDK，属实机验证"]
fn agent_injection_makes_jvm_exit_by_itself() {
    let jdk = jdk8_home();
    assert_injection_stops_jvm("同主版本(JDK8→JDK8)", &jdk, jdk8_launcher(), &jdk);
}

/// 跨主版本必须同样生效：这条路径是「JRE 应用也能优雅停止」的全部依据。
#[test]
#[ignore = "需要本机 JDK 8 与 JDK 21，属实机验证"]
fn agent_injection_works_across_major_versions() {
    let jdk21 = jdk21_home();
    if !jdk21.join("bin").join("java.exe").exists() {
        println!("跳过：未找到 JDK 21（{}），可用 OPX_TEST_JDK21 指定", jdk21.display());
        return;
    }
    // 用 JDK 8 编译（产出 v52，JDK 21 也能跑），再用 JDK 8 启动器注入 JDK 21 目标
    assert_injection_stops_jvm("跨主版本(JDK8→JDK21)", &jdk8_home(), jdk8_launcher(), &jdk21);
}
