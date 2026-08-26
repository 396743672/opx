use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use crate::models::node_app::{
    CreateNodeAppParams, NodeApp, NodeAppStatus, UpdateNodeAppParams,
};
use crate::services::software_manager::health_check;
use crate::services::software_manager::lifecycle;
use crate::utils::paths;

pub struct NodeAppManager {
    inner: Mutex<NodeAppManagerInner>,
}

struct NodeAppManagerInner {
    apps: Vec<NodeApp>,
    data_path: PathBuf,
}

impl NodeAppManager {
    pub fn new() -> Self {
        let data_path = paths::data_dir().join("node-apps.json");
        let apps = Self::read_apps(&data_path);
        Self {
            inner: Mutex::new(NodeAppManagerInner { apps, data_path }),
        }
    }

    fn read_apps(data_path: &Path) -> Vec<NodeApp> {
        if !data_path.exists() {
            return Vec::new();
        }
        match fs::read_to_string(data_path) {
            Ok(c) => serde_json::from_str(&c).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    fn save(&self) -> Result<(), String> {
        let inner = self.inner.lock().unwrap();
        if let Some(parent) = inner.data_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let content =
            serde_json::to_string_pretty(&inner.apps).map_err(|e| format!("序列化失败: {}", e))?;
        fs::write(&inner.data_path, content).map_err(|e| format!("写入失败: {}", e))?;
        Ok(())
    }

    /// 把源入口文件复制到应用运行目录 <app_data>/node-apps/{id}/app.js（统一文件名，覆盖旧版），
    /// 返回相对路径 `node-apps/{id}/app.js`。
    fn copy_entry_to_appdir(src: &Path, app_id: &str) -> Result<String, String> {
        if !src.exists() {
            return Err(format!("入口文件不存在: {}", src.display()));
        }
        let app_dir = paths::data_dir().join("node-apps").join(app_id);
        fs::create_dir_all(&app_dir).map_err(|e| format!("创建目录失败: {}", e))?;
        fs::copy(src, app_dir.join("app.js")).map_err(|e| format!("复制入口文件失败: {}", e))?;
        Ok(format!("node-apps/{}/app.js", app_id))
    }

    /// 把存储的相对入口路径解析为绝对路径（供前端展示 / 启动使用）
    fn resolve_app_entry(app: &mut NodeApp) {
        app.entry_path = paths::resolve_data_path(&app.entry_path)
            .to_string_lossy()
            .into_owned();
    }

    pub fn list(&self) -> Vec<NodeApp> {
        let mut inner = self.inner.lock().unwrap();
        for a in inner.apps.iter_mut() {
            // 运行中但进程已退出 → 标 Error（便于前端及时更新）
            if a.status == NodeAppStatus::Running {
                if let Some(pid) = a.pid {
                    if !health_check::is_process_alive(pid) {
                        a.status = NodeAppStatus::Error;
                        a.last_error = Some("进程已退出".to_string());
                    }
                }
            }
        }
        let mut apps = inner.apps.clone();
        // 锁内直接写盘（避免经 self.save 二次加锁死锁）
        if let Some(parent) = inner.data_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(&inner.apps) {
            let _ = fs::write(&inner.data_path, content);
        }
        drop(inner);
        for a in apps.iter_mut() {
            Self::resolve_app_entry(a);
        }
        apps
    }

    pub fn get(&self, id: &str) -> Option<NodeApp> {
        let mut app = self
            .inner
            .lock()
            .unwrap()
            .apps
            .iter()
            .find(|a| a.id == id)
            .cloned()?;
        Self::resolve_app_entry(&mut app);
        Some(app)
    }

    pub fn create(&self, payload: CreateNodeAppParams) -> Result<NodeApp, String> {
        let name = payload.name.trim().to_string();
        if name.is_empty() {
            return Err("名称不能为空".to_string());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let entry_rel = Self::copy_entry_to_appdir(Path::new(&payload.entry_path), &id)?;
        let app = NodeApp {
            id,
            name,
            entry_path: entry_rel,
            node_installed_id: payload.node_installed_id,
            args: payload.args,
            env_vars: payload.env_vars,
            auto_start: payload.auto_start,
            startup_order: payload.startup_order,
            status: NodeAppStatus::Stopped,
            pid: None,
            last_error: None,
            log_path: String::new(),
        };
        let mut inner = self.inner.lock().unwrap();
        inner.apps.push(app.clone());
        drop(inner);
        let _ = self.save();
        Ok(app)
    }

    pub fn update(&self, id: &str, params: UpdateNodeAppParams) -> Result<NodeApp, String> {
        let mut inner = self.inner.lock().unwrap();
        let app = inner
            .apps
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| "未找到应用".to_string())?;
        if app.status == NodeAppStatus::Running {
            return Err("运行中的应用不可修改配置".to_string());
        }
        if let Some(v) = params.name {
            let v = v.trim().to_string();
            if v.is_empty() {
                return Err("名称不能为空".to_string());
            }
            app.name = v;
        }
        if let Some(v) = params.entry_path {
            let v = v.trim().to_string();
            if !v.is_empty() {
                // 重新上传入口：复制覆盖运行目录中的 app.js（存的相对路径不变）
                Self::copy_entry_to_appdir(Path::new(&v), &app.id)?;
            }
        }
        if let Some(v) = params.node_installed_id {
            app.node_installed_id = v;
        }
        if let Some(v) = params.args {
            app.args = v;
        }
        if let Some(v) = params.env_vars {
            app.env_vars = v;
        }
        if let Some(v) = params.auto_start {
            app.auto_start = v;
        }
        if let Some(v) = params.startup_order {
            app.startup_order = v;
        }
        let out = app.clone();
        drop(inner);
        let _ = self.save();
        Ok(out)
    }

    pub fn delete(&self, id: &str) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let running = inner
            .apps
            .iter()
            .any(|a| a.id == id && a.status == NodeAppStatus::Running);
        if running {
            drop(inner);
            return false;
        }
        let before = inner.apps.len();
        inner.apps.retain(|a| a.id != id);
        let removed = inner.apps.len() != before;
        drop(inner);
        if removed {
            // 删除应用运行目录（入口 JS 等）
            let dir = paths::data_dir().join("node-apps").join(id);
            let _ = fs::remove_dir_all(&dir);
            let _ = self.save();
        }
        removed
    }

    /// 用指定 node.exe 启动应用（stdout/stderr 重定向到 <app_data>/node-logs/<id>.log）
    pub fn start(&self, app_id: &str, node_exe: &Path) -> Result<(), String> {
        let app = self.get(app_id).ok_or_else(|| "未找到应用".to_string())?;
        let entry = Path::new(&app.entry_path);
        if !entry.exists() {
            return Err(format!("入口文件不存在: {}", app.entry_path));
        }
        if !node_exe.exists() {
            return Err("未找到已安装的 Node.js 运行时".to_string());
        }
        if let Some(pid) = app.pid {
            if health_check::is_process_alive(pid) {
                return Err("应用已在运行".to_string());
            }
        }

        let mut cmd = Command::new(node_exe);
        cmd.arg(entry);
        for a in &app.args {
            cmd.arg(a);
        }
        if let Some(cwd) = entry.parent() {
            cmd.current_dir(cwd);
        }
        for (k, v) in &app.env_vars {
            cmd.env(k, v);
        }
        let log_path = paths::data_dir().join("node-logs").join(format!("{}.log", app.id));
        if let Some(parent) = log_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let file = fs::File::create(&log_path).map_err(|e| format!("创建日志失败: {}", e))?;
        cmd.stdout(Stdio::from(file.try_clone().map_err(|e| format!("日志: {}", e))?))
            .stderr(Stdio::from(file));

        let child = cmd.spawn().map_err(|e| format!("启动失败: {}", e))?;
        let pid = child.id();

        let mut inner = self.inner.lock().unwrap();
        if let Some(a) = inner.apps.iter_mut().find(|a| a.id == app_id) {
            a.pid = Some(pid);
            a.status = NodeAppStatus::Running;
            a.last_error = None;
            a.log_path = log_path.to_string_lossy().into_owned();
        }
        drop(inner);
        let _ = self.save();
        Ok(())
    }

    pub fn stop(&self, app_id: &str) -> Result<(), String> {
        let app = self.get(app_id).ok_or_else(|| "未找到应用".to_string())?;
        if let Some(pid) = app.pid {
            if health_check::is_process_alive(pid) {
                lifecycle::stop_one(pid);
            }
        }
        let mut inner = self.inner.lock().unwrap();
        if let Some(a) = inner.apps.iter_mut().find(|a| a.id == app_id) {
            a.pid = None;
            a.status = NodeAppStatus::Stopped;
        }
        drop(inner);
        let _ = self.save();
        Ok(())
    }

    /// 应用启动时按 order 拉起 auto_start 的应用（node_exe 由命令层解析已装 Node）
    pub fn auto_start_all(&self, node_exe: &Path) {
        let apps = self.inner.lock().unwrap().apps.clone();
        let mut pick: Vec<NodeApp> = apps.into_iter().filter(|a| a.auto_start).collect();
        pick.sort_by_key(|a| a.startup_order);
        for a in pick {
            if a.status != NodeAppStatus::Running {
                let _ = self.start(&a.id, node_exe);
            }
        }
    }
}