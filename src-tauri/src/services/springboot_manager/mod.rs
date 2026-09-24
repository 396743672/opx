pub mod agent;
pub mod jvm_opts;
pub mod lifecycle;
pub mod monitor;

use std::sync::RwLock;

use anyhow::Result;
use uuid::Uuid;

use crate::models::springboot::{AppGroup, AppStatus, CreateAppParams, SpringBootApp, SpringBootStore, UpdateAppParams};
use crate::utils::paths;

pub struct SpringBootManager {
    store: RwLock<SpringBootStore>,
}

impl SpringBootManager {
    pub fn new() -> Self {
        let store = Self::load_store().unwrap_or_default();
        // 启动时对账：将 Running/Starting/Stopping 重置为 Stopped
        let mut store = store;
        for app in &mut store.applications {
            if matches!(app.status, AppStatus::Running | AppStatus::Starting | AppStatus::Stopping) {
                app.status = AppStatus::Stopped;
                app.pid = None;
            }
        }
        Self { store: RwLock::new(store) }
    }

    /// 导出用：返回原始存储数据（不做路径解析，保持相对路径）
    pub fn export_apps(&self) -> Vec<SpringBootApp> {
        self.store.read().unwrap().applications.clone()
    }

    pub fn list_apps(&self) -> Vec<SpringBootApp> {
        // ponytail: 兜底校验 — 进程已死但状态卡在 Running/Starting/Stopping 时自动纠正为 Stopped
        let mut store = self.store.write().unwrap();
        let mut changed = false;
        for app in &mut store.applications {
            if let Some(pid) = app.pid {
                if matches!(app.status, AppStatus::Running | AppStatus::Starting | AppStatus::Stopping)
                    && !lifecycle::is_pid_alive(pid)
                {
                    app.status = AppStatus::Stopped;
                    app.pid = None;
                    changed = true;
                }
            }
        }
        if changed {
            let _ = Self::save_store(&store);
        }
        let mut apps = store.applications.clone();
        drop(store);
        for app in &mut apps {
            Self::resolve_app_paths(app);
        }
        apps
    }

    /// 只读快照：不做死进程纠正、不落盘（供看门狗判定意外退出）
    pub fn snapshot_apps(&self) -> Vec<SpringBootApp> {
        self.store.read().unwrap().applications.clone()
    }

    pub fn find_app(&self, id: &str) -> Result<SpringBootApp> {
        let mut app = self.store.read().unwrap().applications.iter()
            .find(|a| a.id == id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("未找到应用: {}", id))?;
        Self::resolve_app_paths(&mut app);
        Ok(app)
    }

    /// 把 apps.json 中的相对路径解析为绝对路径
    fn resolve_app_paths(app: &mut SpringBootApp) {
        app.jar_path = paths::resolve_data_path(&app.jar_path)
            .to_string_lossy().to_string();
        // log_path 保持相对路径，前端显示和 read_springboot_log 中按需解析
    }

    /// 若路径在 data_dir 下则转为相对路径，否则保持原样
    pub fn relativize_data_path(abs_or_rel: &str) -> String {
        let data = paths::data_dir();
        let p = std::path::Path::new(abs_or_rel);
        if let Ok(rel) = p.strip_prefix(&data) {
            rel.to_string_lossy().replace('\\', "/")
        } else {
            abs_or_rel.to_string()
        }
    }

    pub fn create_app(&self, params: CreateAppParams) -> Result<SpringBootApp> {
        let src = std::path::Path::new(&params.jar_path);
        if !src.exists() {
            anyhow::bail!("JAR 文件不存在: {}", params.jar_path);
        }
        // ponytail: 复制 JAR 到数据目录，不原地运行
        let app_dir = paths::data_dir().join("springboot").join(&params.name);
        std::fs::create_dir_all(&app_dir)?;
        let target_jar = app_dir.join("app.jar");
        std::fs::copy(src, &target_jar)?;
        // ponytail: 存相对路径 springboot/{name}/app.jar，避免 data_dir 绝对路径写死
        let jar_path = format!("springboot/{}/app.jar", params.name);

        let version = read_jar_version(
            &paths::data_dir().join(&jar_path).to_string_lossy().to_string()
        ).unwrap_or_else(|| "unknown".to_string());
        // 构建该 JAR 的 Spring Boot 版本：由 repackage 写入 MANIFEST，三版都在
        let spring_boot_version = read_spring_boot_version(
            &paths::data_dir().join(&jar_path).to_string_lossy().to_string()
        );
        // ponytail: 日志在 JAR 同级的 logs/ 目录下
        let log_path = if params.log_path.is_empty() {
            format!("springboot/{}/logs/console.log", params.name)
        } else {
            Self::relativize_data_path(&params.log_path)
        };
        // ponytail: 如果前端未传端口，尝试从 JAR 内部 config 自动读取
        let port = params.port.or_else(|| read_port_from_jar(
            &paths::data_dir().join(&jar_path).to_string_lossy().to_string()
        ));
        let app = SpringBootApp {
            id: Uuid::new_v4().to_string(),
            name: params.name,
            jar_path,
            version,
            spring_boot_version,
            jdk_installed_id: params.jdk_installed_id,
            jvm_opts: params.jvm_opts,
            program_args: params.program_args,
            profile: params.profile,
            env_vars: params.env_vars,
            status: AppStatus::Stopped,
            pid: None,
            port,
            log_path,
            start_time: None,
            last_error: None,
            dependencies: params.dependencies,
            auto_start: params.auto_start,
            startup_order: params.startup_order,
            auto_restart: params.auto_restart,
            group: params.group,
            jdk_type: params.jdk_type,
            stop_timeout_secs: params.stop_timeout_secs,
        };
        let app_clone = app.clone();
        {
            let mut store = self.store.write().unwrap();
            store.applications.push(app);
            Self::save_store(&store)?;
        }
        Ok(app_clone)
    }

    pub fn update_app(&self, id: &str, params: UpdateAppParams) -> Result<SpringBootApp> {
        let mut store = self.store.write().unwrap();
        let app = store.applications.iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| anyhow::anyhow!("未找到应用: {}", id))?;
        if matches!(app.status, AppStatus::Running | AppStatus::Starting) {
            anyhow::bail!("运行中的应用不可修改配置");
        }
        if let Some(v) = params.name { app.name = v; }
        if let Some(v) = params.jdk_installed_id { app.jdk_installed_id = v; }
        if let Some(v) = params.jvm_opts { app.jvm_opts = v; }
        if let Some(v) = params.program_args { app.program_args = v; }
        if let Some(v) = params.profile { app.profile = v; }
        if let Some(v) = params.env_vars { app.env_vars = v; }
        if let Some(v) = params.port { app.port = Some(v); }
        if let Some(v) = params.log_path {
            // ponytail: 若 log_path 在 data_dir 下则存相对路径，避免绝对路径写死
            app.log_path = Self::relativize_data_path(&v);
        }
        if let Some(v) = params.dependencies { app.dependencies = v; }
        if let Some(v) = params.auto_start { app.auto_start = v; }
        if let Some(v) = params.startup_order { app.startup_order = v; }
        if let Some(v) = params.auto_restart { app.auto_restart = v; }
        if let Some(v) = params.group { app.group = v; }
		if let Some(v) = params.jdk_type { app.jdk_type = v; }
        if let Some(v) = params.stop_timeout_secs { app.stop_timeout_secs = v.clamp(1, 600); }
        let cloned = app.clone();
        Self::save_store(&store)?;
        Ok(cloned)
    }

    pub fn delete_app(&self, id: &str) -> Result<()> {
        let mut store = self.store.write().unwrap();
        let pos = store.applications.iter().position(|a| a.id == id)
            .ok_or_else(|| anyhow::anyhow!("未找到应用: {}", id))?;
        let app = &store.applications[pos];
        if matches!(app.status, AppStatus::Running | AppStatus::Starting) {
            anyhow::bail!("运行中的应用不可删除");
        }
        let app_name = app.name.clone();
        store.applications.remove(pos);
        // ponytail: 删除应用目录（jar/logs/config等）
        let app_dir = paths::data_dir().join("springboot").join(&app_name);
        let _ = std::fs::remove_dir_all(&app_dir);
        Self::save_store(&store)?;
        Ok(())
    }

    pub fn update_status(&self, id: &str, status: AppStatus, pid: Option<u32>, error: Option<String>) -> Result<()> {
        let mut store = self.store.write().unwrap();
        let app = store.applications.iter_mut().find(|a| a.id == id)
            .ok_or_else(|| anyhow::anyhow!("未找到应用: {}", id))?;
        // 进入 Starting/Stopped 时清除旧错误，避免上次失败的红字残留
        if matches!(status, AppStatus::Starting | AppStatus::Stopped) {
            app.last_error = None;
        }
        if status == AppStatus::Running {
            app.start_time = Some(chrono::Local::now().naive_local());
        }
        app.status = status;
        app.pid = pid;
        if let Some(e) = error { app.last_error = Some(e); }
        Self::save_store(&store)?;
        Ok(())
    }

    pub fn update_version(&self, id: &str, version: String) -> Result<()> {
        let mut store = self.store.write().unwrap();
        let app = store.applications.iter_mut().find(|a| a.id == id)
            .ok_or_else(|| anyhow::anyhow!("未找到应用: {}", id))?;
        app.version = version;
        // 换 jar 后重新识别 Spring Boot 版本；读不到就置空，避免残留上一个 jar 的值
        app.spring_boot_version = read_spring_boot_version(
            &paths::data_dir().join(&app.jar_path).to_string_lossy(),
        );
        Self::save_store(&store)?;
        Ok(())
    }

    // Groups
    pub fn list_groups(&self) -> Vec<AppGroup> {
        self.store.read().unwrap().groups.clone()
    }

    pub fn save_groups(&self, groups: Vec<AppGroup>) -> Result<()> {
        let mut store = self.store.write().unwrap();
        store.groups = groups;
        Self::save_store(&store)?;
        Ok(())
    }

    pub fn get_global_env_vars(&self) -> Vec<(String, String)> {
        self.store.read().unwrap().global_env_vars.clone()
    }

    pub fn set_global_env_vars(&self, env_vars: Vec<(String, String)>) -> Result<()> {
        let mut store = self.store.write().unwrap();
        store.global_env_vars = env_vars;
        Self::save_store(&store)?;
        Ok(())
    }

    pub fn get_group_env_vars(&self, group_name: &str) -> Vec<(String, String)> {
        let store = self.store.read().unwrap();
        store.groups.iter()
            .find(|g| g.name == group_name)
            .map(|g| g.env_vars.clone())
            .unwrap_or_default()
    }

    fn store_path() -> std::path::PathBuf {
        paths::data_dir().join("springboot").join("apps.json")
    }

    fn load_store() -> Result<SpringBootStore> {
        let path = Self::store_path();
        if !path.exists() { return Ok(SpringBootStore::default()); }
        let content = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    }

    fn save_store(store: &SpringBootStore) -> Result<()> {
        let path = Self::store_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(store)?;
        std::fs::write(&path, content)?;
        Ok(())
    }
}

impl Default for SpringBootManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 从 JAR 文件的 MANIFEST.MF 中读取版本号
/// 读取 JAR 内 `META-INF/MANIFEST.MF` 中某个主属性的值。
///
/// 按 JAR 规范处理折行：单行超过 72 字节会在下一行继续，续行以**一个空格开头**，
/// 该空格是折行标记、不属于值内容。只有紧跟在目标属性后面的续行才会被拼接，
/// 避免把别的属性的续行误加到值上。
pub fn read_jar_manifest_field(jar_path: &str, key: &str) -> Option<String> {
    use std::io::Read;
    let file = std::fs::File::open(jar_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name("META-INF/MANIFEST.MF").ok()?;
    let mut content = String::new();
    entry.read_to_string(&mut content).ok()?;

    let prefix = format!("{key}:");
    let mut value: Option<String> = None;
    let mut collecting = false;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix(' ') {
            if collecting {
                if let Some(v) = value.as_mut() {
                    v.push_str(rest);
                }
            }
        } else {
            collecting = false;
            if let Some(rest) = line.strip_prefix(&prefix) {
                value = Some(rest.trim().to_string());
                collecting = true;
            }
        }
    }
    value
}

/// 读取 JAR 的应用版本（MANIFEST 的 `Implementation-Version`）。
///
/// ⚠️ 该字段**并非必然存在**：它由 Maven jar plugin 的 `addDefaultImplementationEntries`
/// 控制，而该开关默认为 `false`。实测 Spring Boot 2.3.3 / 3.5.11 打出的可执行 jar
/// 都没有这个字段（只有显式配置过的项目才有，如 SimImage 的 `1.0.0-SNAPSHOT`）。
/// 判断「这个 jar 用什么 Spring Boot 构建」应改用 [`read_spring_boot_version`]。
pub fn read_jar_version(jar_path: &str) -> Option<String> {
    read_jar_manifest_field(jar_path, "Implementation-Version")
}

/// 读取 JAR 是由哪个 Spring Boot 版本构建的（MANIFEST 的 `Spring-Boot-Version`）。
///
/// 该字段由 `spring-boot-maven-plugin` 的 repackage 自动写入，**2.x/3.x/4.x 都有**
/// （实测 `2.3.3.RELEASE` / `3.5.11` / `4.0.8`），比 `Implementation-Version` 可靠得多，
/// 也是判断「该 jar 需要什么 JDK」的唯一依据。
pub fn read_spring_boot_version(jar_path: &str) -> Option<String> {
    read_jar_manifest_field(jar_path, "Spring-Boot-Version")
}

/// 解析 Spring Boot 大版本号：`2.3.3.RELEASE` → 2、`3.5.11` → 3、`4.0.8` → 4。
///
/// 2.x 的版本串带 `.RELEASE` 后缀（Maven 老式命名），故只取第一个数字段。
pub fn parse_spring_boot_major(version: &str) -> Option<u32> {
    version.trim().split('.').next()?.trim().parse::<u32>().ok()
}

/// 该 Spring Boot 大版本要求的最低 JDK 主版本；未知大版本返回 `None`（不做判断）。
///
/// - 2.x：Java 8 是基线（2.6/2.7 向上兼容到 17/21）
/// - 3.x / 4.x：**Java 17**（Spring Framework 6/7 的基线）
///
/// 用错 JDK 的失败是启动级的（`UnsupportedClassVersionError`），故在表单里提前提示。
/// 下限只由大版本决定，是全表唯一来源——[`jdk_range_for_spring_boot`] 复用它。
pub fn min_jdk_for_spring_boot(major: u32) -> Option<u32> {
    match major {
        2 => Some(8),
        3 | 4 => Some(17),
        _ => None,
    }
}

/// 解析 Spring Boot 小版本号：`2.7.18` → 7、`3.5.11` → 5、`2.7` → 7。
pub fn parse_spring_boot_minor(version: &str) -> Option<u32> {
    version.trim().split('.').nth(1)?.trim().parse::<u32>().ok()
}

/// 该 Spring Boot 版本官方兼容的 Java **上限**，按小版本查表。
///
/// ⚠️ 这个上限的含义是「官方测试到哪」，**不是硬约束**，而且随补丁发布往上抬：
/// 官方原文即 `Spring Boot 2.7.18 requires Java 8 and is compatible up to and
/// including Java 21`，而 2.3 只到 15。所以调用方应当只**警告**、不要拦截保存——
/// 超出上限的组合多数仍能跑，只是官方没测过。
///
/// 未知小版本返回 `None`：宁可少提示，也不给一个错的警告。
fn max_jdk_for_spring_boot(major: u32, minor: u32) -> Option<u32> {
    match (major, minor) {
        // 2.x 的上限随补丁逐级上抬
        (2, 0) => Some(9),
        (2, 1) => Some(12),
        (2, 2) | (2, 3) => Some(15),
        (2, 4) => Some(16),
        (2, 5) => Some(18),
        (2, 6) => Some(19),
        (2, 7) => Some(21),
        // 3.x / 4.x 的基线、上限同样按小版本区分
        (3, 0) | (3, 1) | (3, 2) => Some(21),
        (3, 3) => Some(23),
        (3, 4) => Some(24),
        (3, 5) => Some(25),
        (4, 0) => Some(25),
        (4, 1) => Some(26),
        _ => None,
    }
}

/// Spring Boot 官方兼容的 Java 区间 `(下限, 上限)`；上限为 `None` 表示未知。
///
/// 下限按大版本取（与 [`min_jdk_for_spring_boot`] 同源），上限按小版本查表。
pub fn jdk_range_for_spring_boot(major: u32, minor: u32) -> Option<(u32, Option<u32>)> {
    Some((
        min_jdk_for_spring_boot(major)?,
        max_jdk_for_spring_boot(major, minor),
    ))
}

/// 把 Java 版本串归一化成主版本：`1.8.0_302` → 8、`17` → 17、`21.0.5` → 21。
pub fn parse_java_major(version: &str) -> Option<u32> {
    let mut parts = version.trim().split('.');
    let first: u32 = parts.next()?.trim().parse().ok()?;
    if first == 1 {
        // JDK 8 及更早用 `1.x` 命名（1.8.0 → 8）
        return parts.next()?.trim().parse().ok();
    }
    Some(first)
}

/// 读取构建该 JAR 的 JDK 主版本。
///
/// ⚠️ 字段名是 **`Build-Jdk-Spec`**，不是 `Build-Jdk`：Maven Archiver 3.5 起因构建不可复现
/// 弃用了 `Build-Jdk`，默认只写 `Build-Jdk-Spec`。实测三个真实 jar（Maven JAR Plugin
/// 3.2.2 / 3.4.2 打出）**都只有 `Build-Jdk-Spec`**，读 `Build-Jdk` 会得到恒为 `None`
/// 的静默失效。这里先用前者，再兜底老插件写的后者。
///
/// 注意它只是**构建环境**，不是运行门槛——同一份源码能用 `--release 17` 在 JDK 21 上编出
/// v61 字节码。所以它只能作为「最佳搭配」的参考提示，不能当硬约束。
pub fn read_build_jdk(jar_path: &str) -> Option<u32> {
    read_jar_manifest_field(jar_path, "Build-Jdk-Spec")
        .or_else(|| read_jar_manifest_field(jar_path, "Build-Jdk"))
        .and_then(|v| parse_java_major(&v))
}

/// ponytail: 从 JAR 内部配置文件读取 server.port
/// 按优先级扫描 BOOT-INF/classes/ 和根目录下的 application.yml/properties/bootstrap.yml/properties
pub fn read_port_from_jar(jar_path: &str) -> Option<u16> {
    use std::io::Read;
    let file = std::fs::File::open(jar_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let candidates = [
        "BOOT-INF/classes/application.yml",
        "BOOT-INF/classes/application.properties",
        "BOOT-INF/classes/bootstrap.yml",
        "application.yml",
        "application.properties",
        "bootstrap.yml",
    ];
    for name in &candidates {
        if let Ok(mut entry) = archive.by_name(name) {
            let mut content = String::new();
            if entry.read_to_string(&mut content).is_ok() {
                if let Some(port) = extract_port_from_config(&content) {
                    return Some(port);
                }
            }
        }
    }
    None
}

fn extract_port_from_config(content: &str) -> Option<u16> {
    fn parse_port_val(s: &str) -> Option<u16> {
        let s = s.trim();
        // ${VAR:4033} → 4033
        if let Some(inner) = s.strip_prefix("${") {
            if let Some(default) = inner.split(':').nth(1) {
                if let Some(end) = default.find('}') {
                    return default[..end].parse::<u16>().ok();
                }
            }
        }
        s.parse::<u16>().ok()
    }
    // properties: server.port=8080 or server.port: 8080
    for line in content.lines() {
        let t = line.trim();
        if let Some(val) = t.strip_prefix("server.port") {
            let after = val.trim_start_matches(&['=', ':', ' '][..]);
            if let Some(p) = parse_port_val(after) { return Some(p); }
        }
    }
    // YAML: server:\n  port: 8080 or port: ${VAR:4033}
    let lines: Vec<&str> = content.lines().collect();
    for i in 0..lines.len() {
        if lines[i].trim() == "server:" {
            for j in i + 1..lines.len().min(i + 5) {
                let t = lines[j].trim();
                if let Some(val) = t.strip_prefix("port:") {
                    if let Some(p) = parse_port_val(val) { return Some(p); }
                } else if !t.is_empty() && !t.starts_with('#')
                    && !lines[j].starts_with(' ') && !lines[j].starts_with('\t')
                {
                    break;
                }
            }
        }
    }
    None
}
