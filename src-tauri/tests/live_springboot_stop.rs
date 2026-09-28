//! 实机验证：**真实 Spring Boot fat jar** 在 JRE 运行时下也能被 attach 注入优雅停止。
//!
//! 与 `live_agent_stop.rs` 的区别：那边的目标是自编译的小程序（只验证注入机制本身），
//! 这里用的是真实 Spring Boot 应用 —— 嵌套 jar 类加载器（`JarLauncher`）、嵌入式
//! Tomcat、完整的 Spring 容器。三个样本覆盖三个大版本，且 **application.properties
//! 里没有任何 Actuator 配置**（`management.endpoint.shutdown` 默认关闭），
//! 正是「删掉 Actuator 通道后仍能优雅停止」这一结论的实证。
//!
//! | jar | Spring Boot | 运行时 | 说明 |
//! |---|---|---|---|
//! | demo-jdk8.jar  | 2.7.18 | JRE 8  | 关闭日志无 GracefulShutdown（版本差异） |
//! | demo-jdk17.jar | 3.5.16 | JRE 17 | |
//! | demo-jdk21.jar | 4.0.8  | JRE 21 | 目标 stderr 会打 JEP 451 警告 |
//!
//! 判据（缺一不可）：
//! 1. 应用自行退出（无任何强杀）
//! 2. **退出码为 0** —— `System.exit(0)` 的特征；`taskkill /F` 的对照退出码是 1，
//!    这也是「不能只看进程消失，还要看怎么消失」的原因
//! 3. 日志出现 `Shutting down ExecutorService` —— Spring 容器真的执行了 destroy
//!    （注意：该类日志是 **DEBUG 级**，故启动时显式开 `--logging.level.org.springframework=DEBUG`，
//!    否则关闭过程在 INFO 级别下什么都不打，会被误判成「没优雅停止」）
//!
//! 依赖本机资源，缺失则自动跳过。可用环境变量覆盖：
//! - `OPX_TEST_JARS`：存放这三个 jar 的目录（默认 `C:\Users\Runic\Desktop\startjar`）
//! - `OPX_TEST_APPS`：opx 的已装运行时根目录（默认 `target/debug/apps`）
//! - `OPX_TEST_JDK21`：用作注入启动器的 JDK（默认 `D:\jdks\jdk-21.0.11`）
//!
//! 运行：`cargo test --test live_springboot_stop -- --ignored --nocapture`
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use opx_lib::services::springboot_manager::agent::{
    ensure_agent_jar, request_graceful_exit, InjectOutcome, Launcher,
};

fn env_path(var: &str, fallback: &str) -> PathBuf {
    std::env::var(var).map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(fallback))
}

fn jars_dir() -> PathBuf {
    env_path("OPX_TEST_JARS", r"C:\Users\Runic\Desktop\startjar")
}

fn apps_dir() -> PathBuf {
    env_path("OPX_TEST_APPS", "target/debug/apps")
}

/// 注入启动器：只用 JDK 21，以便覆盖「JRE 目标没有同版本 JDK」这一真实场景。
fn launcher() -> Launcher {
    let home = env_path("OPX_TEST_JDK21", r"D:\jdks\jdk-21.0.11");
    Launcher { java: home.join("bin").join("java.exe"), classpath_prefix: None }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// 起真实 jar、注入、校验它优雅退出。资源缺失时打印跳过说明并返回 `false`。
fn assert_real_app_stops_gracefully(jar_name: &str, sb_ver: &str, runtime_rel: &str) -> bool {
    let jar_path = jars_dir().join(jar_name);
    let java = apps_dir().join(runtime_rel).join("bin").join("java.exe");
    if !jar_path.exists() || !java.exists() {
        println!("跳过 {jar_name}：缺 jar（{}）或运行时（{}）", jar_path.display(), java.display());
        return false;
    }

    let work = std::env::temp_dir().join(format!("opx_sb_live_{}_{}", jar_name, std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let log_path = work.join("app.log");

    // 用应用自己的运行时启动（模拟 opx：JRE + 重定向日志 + 隐藏控制台）。
    // `--server.port=0` 取随机端口：命令行参数优先级高于环境变量，可免疫外部环境
    // 注入的 `SERVER_PORT` 之类配置（实测会把 server.port 顶掉，导致端口冲突启动失败）。
    // DEBUG 级别是为了让 Spring 的 destroy 日志可见——那才是「容器真的关了」的证据。
    let log = std::fs::File::create(&log_path).unwrap();
    let log_err = log.try_clone().unwrap();
    let mut cmd = Command::new(&java);
    cmd.arg("-jar")
        .arg(&jar_path)
        .arg("--server.port=0")
        .arg("--logging.level.org.springframework=DEBUG")
        .arg("--logging.level.org.apache=DEBUG")
        .current_dir(&work)
        .stdout(log)
        .stderr(log_err);
    cmd.env_remove("SERVER_PORT");
    cmd.env_remove("SERVER__PORT");
    cmd.env_remove("SERVER__HOST");
    opx_lib::utils::process::hide_console(&mut cmd);
    let mut child = cmd.spawn().unwrap_or_else(|e| panic!("{jar_name}: 无法启动: {e}"));
    let pid = child.id();

    // 等 Spring Boot 就绪
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut ready = false;
    while Instant::now() < deadline {
        if read(&log_path).contains("Started DemoApplication") {
            ready = true;
            break;
        }
        if matches!(child.try_wait(), Ok(Some(_))) {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    if !ready {
        let tail: Vec<String> = read(&log_path).lines().rev().take(12).map(str::to_string).collect();
        let _ = child.kill();
        let _ = std::fs::remove_dir_all(&work);
        panic!("{jar_name}: 120s 内未就绪，日志尾部:\n{}", tail.join("\n"));
    }

    // 注入
    ensure_agent_jar().expect("agent jar 应落盘");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    match runtime.block_on(request_graceful_exit(pid, launcher())) {
        Ok(InjectOutcome::Accepted) => println!("{jar_name}: 注入被接受（启动器退出码 0）"),
        // 跨版本时启动器客户端会解析响应失败（`Failed to load agent library`），
        // 但命令已送达并被目标执行 —— 判据只能是目标的退出码与关闭日志
        Ok(InjectOutcome::Failed(why)) => println!("{jar_name}: 启动器报错但命令已送达 → {why}"),
        Err(e) => panic!("{jar_name}: 注入不可用: {e}"),
    }

    // 等它自己退（不做任何强杀）
    let deadline = Instant::now() + Duration::from_secs(40);
    let mut status = None;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(s)) => {
                status = Some(s);
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => panic!("{jar_name}: 等待进程失败: {e}"),
        }
    }
    if status.is_none() {
        let _ = child.kill();
    }

    let log_text = read(&log_path);
    let _ = std::fs::remove_dir_all(&work);

    let status = status.unwrap_or_else(|| panic!("{jar_name}: 注入后 40s 内未自行退出（可能被强杀）"));
    assert_eq!(
        status.code(),
        Some(0),
        "{jar_name}: 退出码非 0 —— 说明不是 System.exit(0) 路径（taskkill /F 的退出码是 1）"
    );
    assert!(
        log_text.contains("Shutting down ExecutorService"),
        "{jar_name}: 日志没有 Spring 容器 destroy 的痕迹，未真正优雅停止"
    );
    println!("✅ {jar_name}（Spring Boot {sb_ver}）：退出码 0 + 容器 destroy，优雅停止");
    true
}

#[test]
#[ignore = "需要本机真实 Spring Boot jar 与 JRE 运行时，属实机验证"]
fn real_springboot_apps_stop_gracefully_without_actuator() {
    let cases = [
        ("demo-jdk8.jar", "2.7.18", r"jre\8.0.504+1"),
        ("demo-jdk17.jar", "3.5.16", r"jre\17.0.20+101"),
        ("demo-jdk21.jar", "4.0.8", r"jre\21.0.12+101.0.LTS"),
    ];
    let mut ran = 0;
    for (jar, ver, runtime) in cases {
        if assert_real_app_stops_gracefully(jar, ver, runtime) {
            ran += 1;
        }
    }
    if ran == 0 {
        println!("三个样本都未运行——请检查 OPX_TEST_JARS / OPX_TEST_APPS 是否指向真实路径");
    }
}
