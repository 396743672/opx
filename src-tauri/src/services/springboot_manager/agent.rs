//! Windows 优雅停止通道：attach + agent 注入。
//!
//! Windows 上没有 SIGTERM，应用零配置时可选的手段全部实测无效：
//! `jcmd <pid> Shutdown`（命令不存在）、`taskkill /PID` 不带 `/F`（WM_CLOSE
//! 只对进程自拥窗口有效）、`AttachConsole` + `CTRL_BREAK`（API 返回成功但
//! 信号不送达）。剩下的唯一「不需要应用改任何配置」的通道，是用 JDK 自带的
//! attach API 把一个极小的 agent 注入目标 JVM，让 JVM **自己** `System.exit(0)`
//! ——这一步会走完整的 JVM 关闭流程，shutdown hook、Spring 优雅停机、
//! `@PreDestroy`、连接池关闭全部生效，等价于 Linux 上的 SIGTERM。
//!
//! agent 与启动器共用一个 1.8 KB 的 jar（[`STOP_AGENT_JAR`] 内嵌在二进制里），
//! 停止时落到 `data_dir/opx-stop.jar`，用应用自己的 JDK 执行
//! `java -cp <jar> AttachMain <pid> <jar>`。
//!
//! 运行时约束（均已实测，详见 `resources/opx-stop-src/README.md`）：
//! - 启动器必须是 **JDK**（attach API 不在 JRE 里），目标可以是 JRE
//! - 启动器与目标**不必同主版本**：实测 JDK 8 / 21 / 25 注入 JRE 8 / 17 / 21
//!   全部 9/9 生效（shutdown hook 执行、进程自行退出）。跨版本时**客户端**会
//!   解析响应失败并打印 `Non-numeric value found` / `AgentLoadException`，
//!   但命令已送达目标并被执行——所以**不能按启动器退出码判定成败**。
//!   同主版本只是让客户端能干净地拿到响应，故仅作「优先」，不是必要条件。
//! - 目标带 `-XX:+DisableAttachMechanism` 时无法注入
//! - JDK 21+ 注入时目标 stderr 会打 4 行 JEP 451 警告（属预期）

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::models::software::InstalledSoftware;
use crate::models::springboot::SpringBootApp;
use crate::services::software_manager::SoftwareManager;
use crate::utils::{paths, process::hidden};

/// 内嵌的停止 agent（源码与重建方式见 `resources/opx-stop-src/README.md`）。
/// 同一条命令既启动它、又把它自己当 agent jar 交给 `loadAgent`。
const STOP_AGENT_JAR: &[u8] = include_bytes!("../../../resources/opx-stop.jar");

/// attach 启动器的执行上限：正常几百毫秒内返回，卡住时不该拖住整个停止流程。
const ATTACH_TIMEOUT_SECS: u64 = 15;

/// 可用于注入的启动器 JDK。
#[derive(Debug, Clone)]
pub struct Launcher {
    pub java: PathBuf,
    /// JDK 8 的 attach API 在 `lib/tools.jar` 里，需要额外放进 classpath；
    /// JDK 9+ 已模块化，为 `None`。
    pub classpath_prefix: Option<PathBuf>,
}

/// 注入结果。
///
/// 之所以区分「完全没跑起来」与「跑了但报错」：attach 是客户端先发命令、再读响应，
/// 跨版本时客户端解析响应会失败，但**命令已经送达并被执行**——实测 JDK 8 启动器
/// 打 JRE 21 目标时客户端退出码为 1，而目标的 shutdown hook 确实执行了。
/// 因此「跑了但报错」仍应算作已发出停止请求、值得等待，不能直接判死。
pub enum InjectOutcome {
    Accepted,
    Failed(String),
}

/// 把内嵌的 agent jar 落到 `data_dir`，返回路径。内容有变（升级后）会自动覆盖。
pub fn ensure_agent_jar() -> Result<PathBuf, String> {
    let path = paths::data_dir().join("opx-stop.jar");
    let up_to_date = std::fs::read(&path)
        .map(|on_disk| on_disk.as_slice() == STOP_AGENT_JAR)
        .unwrap_or(false);
    if !up_to_date {
        std::fs::write(&path, STOP_AGENT_JAR)
            .map_err(|e| format!("写出停止 agent 失败（{}）: {}", path.display(), e))?;
    }
    Ok(path)
}

/// 版本主号：`21.0.12+101.0.LTS` → 21；旧命名 `1.8.0_492` → 8。
pub fn version_major(version: &str) -> Option<u32> {
    let mut parts = version
        .trim()
        .split(|c: char| c == '.' || c == '_' || c == '+' || c == '-');
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        // 1.x 是 JDK 8 及更早的旧命名（1.8.0_492 → 8）
        parts.next().and_then(|s| s.parse().ok())
    } else {
        Some(first)
    }
}

fn jdk_binary(home: &Path, name: &str) -> PathBuf {
    home.join("bin")
        .join(if cfg!(windows) { format!("{}.exe", name) } else { name.to_string() })
}

/// 判断是不是**带 attach API 的 JDK**（而非 JRE）。
///
/// 判据取 `bin/jcmd`：JDK 8 与 JDK 9+ 都有，JRE 任何版本都没有
/// （已实测本机 JRE 8 / 17 / 21 的 `bin` 下均无 `jcmd`）。
fn is_jdk_home(home: &Path) -> bool {
    jdk_binary(home, "jcmd").exists()
}

fn build_launcher(home: &Path) -> Launcher {
    let tools_jar = home.join("lib").join("tools.jar");
    Launcher {
        java: jdk_binary(home, "java"),
        classpath_prefix: if tools_jar.exists() { Some(tools_jar) } else { None },
    }
}

/// 拼 attach 启动器的 classpath：JDK 8 需要把 tools.jar 排在 agent jar 前面。
fn build_classpath(prefix: Option<&Path>, jar: &Path) -> String {
    let sep = if cfg!(windows) { ';' } else { ':' };
    match prefix {
        Some(p) => format!("{}{}{}", p.display(), sep, jar.display()),
        None => jar.display().to_string(),
    }
}

/// 选出用于注入的启动器。
///
/// attach 要求启动器是 JDK（JRE 里没有 attach API），所以：
/// 1. 应用绑定的运行时本身就是 JDK —— 直接用它
/// 2. 绑定的是 JRE —— 借一个已安装的 JDK 顶替，**同主版本优先**
/// 3. 本机一个 JDK 都没有 —— 返回原因，由调用方降级为强杀（能力边界）
///
/// 同主版本只是优先项而非硬约束：实测跨主版本同样能让目标执行 shutdown hook
/// 并自行退出（JDK 8 / 21 / 25 × JRE 8 / 17 / 21 = 9/9 生效），差别仅在于
/// 客户端能否干净地解析响应。所以本机只有 JDK 8 也足以停止 JRE 17/21 的应用。
pub fn resolve_launcher(
    app: &SpringBootApp,
    software_mgr: &SoftwareManager,
) -> Result<Launcher, String> {
    resolve_from(app, &software_mgr.get_installed())
}

/// [`resolve_launcher`] 的实现体：不依赖配置目录，便于测试。
///
/// 🚨 `installed` 各条目的 `install_path` 是**相对 `apps_dir()` 的路径**
/// （`get_installed()` 的契约如此，返回值供前端展示用），直接拿去 `exists()` **恒为 false**。
/// 必须先解析成绝对路径再做文件系统检查——忘了这一步的后果是：用 **JRE** 启动的应用
/// 拿不到任何候选，被误判成「本机未安装 JDK」而直接强杀
/// （2026-09-24 实机踩到：demo-jdk17 绑 JRE 17，停止时提示无通道）。
///
/// 之所以让本函数收「列表」而不是 `&SoftwareManager`：路径解析必须和选择逻辑
/// 待在一起，否则每个调用方都得自己记住「哪个访问器会解析路径」，漏一个就静默失效。
fn resolve_from(app: &SpringBootApp, installed: &[InstalledSoftware]) -> Result<Launcher, String> {
    let own = installed
        .iter()
        .find(|s| s.id == app.jdk_installed_id)
        .ok_or("所选运行时未找到")?;
    let own_home = paths::resolve_install_path(&own.install_path);
    if is_jdk_home(&own_home) {
        return Ok(build_launcher(&own_home));
    }

    // 应用用的是 JRE：找一个已装 JDK 当启动器。
    // 同主版本优先（客户端能干净读到响应）；无从匹配时取版本最高者，
    // 免得按安装顺序挑到最老的那个 JDK。
    let want = version_major(&own.version);
    let mut candidates: Vec<(PathBuf, Option<u32>)> = installed
        .iter()
        .filter(|s| s.key == "jdk")
        .map(|s| {
            (
                paths::resolve_install_path(&s.install_path),
                version_major(&s.version),
            )
        })
        .filter(|(home, _)| is_jdk_home(home))
        .collect();
    candidates.sort_by_key(|(_, major)| (*major != want, std::cmp::Reverse(*major)));

    match candidates.first() {
        Some((home, _)) => Ok(build_launcher(home)),
        None => Err("本机未安装 JDK，无法通过 attach 注入停止（装任意一个 JDK 即可）".to_string()),
    }
}

fn run_launcher(pid: u32, launcher: &Launcher, jar: &Path) -> Result<InjectOutcome, String> {
    let classpath = build_classpath(launcher.classpath_prefix.as_deref(), jar);
    let out = hidden(&launcher.java)
        .arg("-cp")
        .arg(classpath)
        .arg("AttachMain")
        .arg(pid.to_string())
        .arg(jar)
        .output()
        .map_err(|e| format!("无法启动 attach 启动器（{}）: {}", launcher.java.display(), e))?;

    if out.status.success() {
        return Ok(InjectOutcome::Accepted);
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let brief: String = stderr.trim().chars().take(200).collect();
    Ok(InjectOutcome::Failed(format!(
        "attach 启动器退出码 {:?}: {}",
        out.status.code(),
        if brief.is_empty() { "(无错误输出)" } else { &brief }
    )))
}

/// 请求目标 JVM 自行退出。
///
/// `Err` 只表示**注入这件事根本没做起来**（找不到启动器 / 进程起不来 / 卡超时）；
/// 「做了但报错」走 [`InjectOutcome::Failed`]，调用方仍应等待——命令可能已送达。
pub async fn request_graceful_exit(
    pid: u32,
    launcher: Launcher,
) -> Result<InjectOutcome, String> {
    let jar = ensure_agent_jar()?;
    let task = tokio::task::spawn_blocking(move || run_launcher(pid, &launcher, &jar));
    match tokio::time::timeout(Duration::from_secs(ATTACH_TIMEOUT_SECS), task).await {
        Ok(Ok(result)) => result,
        Ok(Err(join_err)) => Err(format!("注入任务异常: {}", join_err)),
        Err(_) => Err(format!(
            "attach 启动器 {}s 未返回，已放弃注入",
            ATTACH_TIMEOUT_SECS
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_major_handles_modern_and_legacy_naming() {
        assert_eq!(version_major("21.0.12+101.0.LTS"), Some(21));
        assert_eq!(version_major("17.0.20+101"), Some(17));
        assert_eq!(version_major("25.0.3"), Some(25));
        // 1.x 是 JDK 8 及更早的旧命名
        assert_eq!(version_major("1.8.0_492"), Some(8));
        assert_eq!(version_major("1.7.0_80"), Some(7));
        assert_eq!(version_major(""), None);
        assert_eq!(version_major("unknown"), None);
    }

    #[test]
    fn classpath_prefixes_tools_jar_only_when_present() {
        let jar = Path::new("/tmp/opx-stop.jar");
        let no_prefix = build_classpath(None, jar);
        assert_eq!(no_prefix, jar.display().to_string());

        let with_prefix = build_classpath(Some(Path::new("/jdk/lib/tools.jar")), jar);
        assert!(with_prefix.contains("tools.jar"), "JDK 8 需 tools.jar: {with_prefix}");
        assert!(with_prefix.ends_with("opx-stop.jar"));
    }

    /// 判据回归：`bin/jcmd` 存在即视为 JDK。JRE 目录下没有 jcmd。
    #[test]
    fn jdk_home_detection_uses_jcmd() {
        let dir = std::env::temp_dir().join(format!("opx_jdkhome_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();

        assert!(!is_jdk_home(&dir), "空目录不应判为 JDK");

        std::fs::write(jdk_binary(&dir, "jcmd"), b"").unwrap();
        assert!(is_jdk_home(&dir), "有 bin/jcmd 应判为 JDK");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 内嵌的 jar 必须真的是一个可用的 zip（含 agent 与启动器的清单）。
    #[test]
    fn embedded_agent_jar_is_a_valid_archive() {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(STOP_AGENT_JAR.to_vec()))
            .expect("内嵌 jar 应可解析");
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.iter().any(|n| n == "AttachMain.class"), "缺启动器: {names:?}");
        assert!(names.iter().any(|n| n == "StopAgent.class"), "缺 agent: {names:?}");

        let mut manifest = String::new();
        use std::io::Read;
        archive
            .by_name("META-INF/MANIFEST.MF")
            .expect("缺 MANIFEST")
            .read_to_string(&mut manifest)
            .unwrap();
        assert!(manifest.contains("Main-Class: AttachMain"), "清单: {manifest}");
        assert!(manifest.contains("Agent-Class: StopAgent"), "清单: {manifest}");
    }

    /// 落盘后内容必须与内嵌字节一致，且重复调用不会重复写。
    #[test]
    fn agent_jar_lands_on_disk_intact() {
        let path = ensure_agent_jar().expect("应写出 agent jar");
        let on_disk = std::fs::read(&path).expect("应能读回");
        assert_eq!(on_disk.as_slice(), STOP_AGENT_JAR);

        let again = ensure_agent_jar().expect("第二次调用应成功");
        assert_eq!(again, path);
        assert_eq!(std::fs::read(&again).unwrap().as_slice(), STOP_AGENT_JAR);
    }

    // ===== 启动器选择 =====
    // 下面几个测试全部走 [`resolve_from`] 而不是 [`resolve_launcher`]：后者要读真实配置
    // 目录，前者只要一个列表，能把「相对路径」这个真实形态原样喂进来。

    /// 造一条已装运行时记录。`path` 就按 `installed.json` 里的真实写法给
    /// （**相对 apps_dir**，不是绝对路径）——这正是出过 bug 的形态。
    fn runtime(id: &str, key: &str, version: &str, path: &str) -> InstalledSoftware {
        serde_json::from_value(serde_json::json!({
            "id": id, "key": key, "version": version, "name": key,
            "install_path": path,
            "install_time": "2026-01-01T00:00:00",
            "status": "Unknown", "port": 0, "config": {},
            "is_custom": false, "auto_start_on_app_start": false, "startup_order": 0,
            "source": {"Mirror": {"mirror_name": "m", "url": "https://example.invalid"}},
        }))
        .expect("测试记录应可解析")
    }

    /// 只关心绑定的运行时，其余字段给可解析的最小值。
    fn app_on(jdk_installed_id: &str) -> SpringBootApp {
        serde_json::from_value(serde_json::json!({
            "id": "app", "name": "app", "jar_path": "springboot/app/app.jar",
            "version": "1.0", "jdk_installed_id": jdk_installed_id,
            "jvm_opts": [], "program_args": [], "profile": "",
            "env_vars": [], "status": "Stopped", "pid": null, "port": 8080,
            "log_path": "springboot/app/logs/console.log",
            "start_time": null, "last_error": null, "dependencies": [],
            "auto_start": false, "startup_order": 0, "auto_restart": false,
            "group": null, "jdk_type": "jdk",
        }))
        .expect("测试应用应可解析")
    }

    /// 在 `apps_dir()` 下造一个假的「已装 JDK」目录（判据只需 `bin/jcmd`），
    /// 返回其绝对路径。测试进程的 exe 在 `target/debug/deps`，不会碰真实数据目录。
    fn fake_jdk_dir(rel: &str) -> PathBuf {
        let home = paths::apps_dir().join(rel);
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("bin")).unwrap();
        std::fs::write(jdk_binary(&home, "jcmd"), b"").unwrap();
        home
    }

    /// 🚨 回归（2026-09-24 实机踩到）：`get_installed()` 给的是**相对 apps_dir** 的路径，
    /// 若照原样 `exists()` 判定，用 **JRE** 启动的应用会得到空候选列表，被误判成
    /// 「本机未安装 JDK」→ 无通道 → 直接强杀（demo-jdk17 绑 JRE 17 就是这个现象）。
    #[test]
    fn jre_app_borrows_jdk_through_relative_install_path() {
        let home = fake_jdk_dir("opx-test-jdk-rel");

        let jre = runtime("jre-17", "jre", "17.0.20+101", "jre/17.0.20+101");
        // ⚠️ 这里必须是相对路径，否则测不出原来的 bug
        let jdk = runtime("jdk-21", "jdk", "21.0.12+101.0.LTS", "opx-test-jdk-rel");

        let launcher = resolve_from(&app_on(&jre.id), &[jre, jdk])
            .expect("绑定 JRE 的应用应能借已装 JDK 注入（相对路径必须被解析）");
        assert_eq!(
            launcher.java,
            jdk_binary(&home, "java"),
            "启动器应指向被解析后的绝对路径"
        );
        assert_eq!(launcher.classpath_prefix, None, "JDK 21 无需 tools.jar");

        let _ = std::fs::remove_dir_all(&home);
    }

    /// 候选多于一个时：同主版本优先；无从匹配则取版本最高者（不按安装顺序挑最老的）。
    #[test]
    fn launcher_prefers_same_major_then_newest() {
        let h8 = fake_jdk_dir("opx-test-jdk8");
        let h21 = fake_jdk_dir("opx-test-jdk21");
        let h25 = fake_jdk_dir("opx-test-jdk25");

        let jdks = vec![
            runtime("jdk-8", "jdk", "8.0.504+1", "opx-test-jdk8"),
            runtime("jdk-21", "jdk", "21.0.12+101.0.LTS", "opx-test-jdk21"),
            runtime("jdk-25", "jdk", "25.0.3", "opx-test-jdk25"),
        ];

        let jre17 = runtime("jre-17", "jre", "17.0.20+101", "jre/17.0.20+101");
        let jre21 = runtime("jre-21", "jre", "21.0.12+101.0.LTS", "jre/21.0.12+101.0.LTS");
        let all: Vec<InstalledSoftware> = jdks
            .into_iter()
            .chain([jre17.clone(), jre21.clone()])
            .collect();

        // JRE 17 没有同版本 JDK → 取最高的 25（而不是列表里最靠前的 8）
        let picked = resolve_from(&app_on(&jre17.id), &all).expect("应能借到 JDK");
        assert_eq!(picked.java, jdk_binary(&h25, "java"));

        // 有同版本时必须优先同版本，哪怕它不是最新
        let picked = resolve_from(&app_on(&jre21.id), &all).expect("应能借到 JDK");
        assert_eq!(picked.java, jdk_binary(&h21, "java"));

        for h in [h8, h21, h25] {
            let _ = std::fs::remove_dir_all(&h);
        }
    }

    /// 应用自己绑的就是 JDK 时直接用，不必兜底。
    #[test]
    fn jdk_bound_app_uses_its_own_runtime() {
        let home = fake_jdk_dir("opx-test-jdk-own");
        let jdk = runtime("jdk-own", "jdk", "8.0.504+1", "opx-test-jdk-own");

        let launcher = resolve_from(&app_on(&jdk.id), std::slice::from_ref(&jdk))
            .expect("绑定 JDK 的应用应直接用它");
        assert_eq!(launcher.java, jdk_binary(&home, "java"));

        let _ = std::fs::remove_dir_all(&home);
    }

    /// 本机真的一个 JDK 都没有时降级强杀——这是能力边界，不是 bug。
    /// 此时错误信息要说清「装个 JDK 即可」，否则用户只会看到「无通道」。
    #[test]
    fn reports_missing_jdk_when_only_jre_installed() {
        let jre = runtime("jre-17", "jre", "17.0.20+101", "jre/17.0.20+101");
        let err = resolve_from(&app_on(&jre.id), std::slice::from_ref(&jre))
            .expect_err("只有 JRE 时应降级");
        assert!(err.contains("未安装 JDK"), "错误信息: {err}");
    }

    /// 绑定的运行时记录本身不存在（如运行时已被卸载）也要给出可读原因，不能 panic。
    #[test]
    fn reports_missing_runtime() {
        let err = resolve_from(&app_on("不存在的-id"), &[]).expect_err("应报错");
        assert!(err.contains("未找到"), "错误信息: {err}");
    }
}
