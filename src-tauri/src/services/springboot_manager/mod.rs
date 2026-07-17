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
    fn relativize_data_path(abs_or_rel: &str) -> String {
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
            health_check_timeout_secs: params.health_check_timeout_secs,
            jdk_type: params.jdk_type,
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
		if let Some(v) = params.health_check_timeout_secs { app.health_check_timeout_secs = v; }
		if let Some(v) = params.jdk_type { app.jdk_type = v; }
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
        if status == AppStatus::Running { app.start_time = Some(chrono::Local::now().naive_local()); }
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
pub fn read_jar_version(jar_path: &str) -> Option<String> {
    use std::io::Read;
    let file = std::fs::File::open(jar_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name("META-INF/MANIFEST.MF").ok()?;
    let mut content = String::new();
    entry.read_to_string(&mut content).ok()?;
    for line in content.lines() {
        if let Some(val) = line.strip_prefix("Implementation-Version:") {
            return Some(val.trim().to_string());
        }
    }
    None
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
