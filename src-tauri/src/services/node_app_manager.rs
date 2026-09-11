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

    /// 校验并规范化名称（非空 + 无路径非法字符），用于目录/入口命名
    fn sanitize_name(name: &str) -> Result<String, String> {
        let t = name.trim().to_string();
        if t.is_empty() {
            return Err("名称不能为空".to_string());
        }
        if t.chars().any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')) {
            return Err("名称包含非法字符（/\\:*?\"<>|）".to_string());
        }
        Ok(t)
    }

    /// 入口文件相对路径：`node-apps/{name}/{name}.js`（目录与文件名均与应用名一致）
    fn entry_rel(name: &str) -> String {
        format!("node-apps/{}/{}.js", name, name)
    }

    /// 日志相对路径：`node-apps/{name}/logs/console.log`
    fn log_rel(name: &str) -> String {
        format!("node-apps/{}/logs/console.log", name)
    }

    /// 把源入口文件复制到应用运行目录 <app_data>/node-apps/{name}/{name}.js（覆盖旧版），
    /// 返回相对路径。
    fn copy_entry_to_appdir(src: &Path, name: &str) -> Result<String, String> {
        if !src.exists() {
            return Err(format!("入口文件不存在: {}", src.display()));
        }
        let app_dir = paths::data_dir().join("node-apps").join(name);
        fs::create_dir_all(&app_dir).map_err(|e| format!("创建目录失败: {}", e))?;
        // 隔离外部 module 类型（父目录可能存在 "type":"module" 的 package.json），
        // 让运行目录内的 .js 按 CommonJS 解析，避免 require 报错
        let _ = fs::write(app_dir.join("package.json"), "{ \"type\": \"commonjs\" }\n");
        fs::copy(src, app_dir.join(format!("{}.js", name)))
            .map_err(|e| format!("复制入口文件失败: {}", e))?;
        Ok(Self::entry_rel(name))
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
        let apps = inner.apps.clone();
        // 锁内直接写盘（避免经 self.save 二次加锁死锁）
        if let Some(parent) = inner.data_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(&inner.apps) {
            let _ = fs::write(&inner.data_path, content);
        }
        drop(inner);
        apps
    }

    pub fn get(&self, id: &str) -> Option<NodeApp> {
        // 返回相对路径（前端展示用；启动/读取时由调用方解析为绝对）
        self.inner.lock().unwrap().apps.iter().find(|a| a.id == id).cloned()
    }

    pub fn create(&self, payload: CreateNodeAppParams) -> Result<NodeApp, String> {
        let name = Self::sanitize_name(&payload.name)?;
        {
            let inner = self.inner.lock().unwrap();
            if inner.apps.iter().any(|a| a.name == name) {
                return Err("应用名称已存在，请更换名称".to_string());
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        let entry_rel = Self::copy_entry_to_appdir(Path::new(&payload.entry_path), &name)?;
        let log_path = Self::log_rel(&name);
        let app = NodeApp {
            id,
            name,
            entry_path: entry_rel,
            node_installed_id: payload.node_installed_id,
            args: payload.args,
            env_vars: payload.env_vars,
            auto_start: payload.auto_start,
            startup_order: payload.startup_order,
            auto_restart: payload.auto_restart,
            status: NodeAppStatus::Stopped,
            pid: None,
            last_error: None,
            log_path,
        };
        let mut inner = self.inner.lock().unwrap();
        inner.apps.push(app.clone());
        drop(inner);
        let _ = self.save();
        Ok(app)
    }

    pub fn update(&self, id: &str, params: UpdateNodeAppParams) -> Result<NodeApp, String> {
        let mut inner = self.inner.lock().unwrap();
        let idx = inner
            .apps
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| "未找到应用".to_string())?;
        // 名称唯一校验（在取得可变借用前，避免借用冲突）
        let mut rename_to: Option<String> = None;
        if let Some(v) = params.name {
            let nn = Self::sanitize_name(&v)?;
            if nn != inner.apps[idx].name {
                let conflict = inner
                    .apps
                    .iter()
                    .enumerate()
                    .any(|(i, x)| i != idx && x.name == nn);
                if conflict {
                    return Err("应用名称已存在，请更换名称".to_string());
                }
                rename_to = Some(nn);
            }
        }
        if inner.apps[idx].status == NodeAppStatus::Running {
            return Err("运行中的应用不可修改配置".to_string());
        }

        let app = inner.apps.get_mut(idx).unwrap();
        if let Some(nn) = rename_to {
            let old_dir = paths::data_dir().join("node-apps").join(&app.name);
            let new_dir = paths::data_dir().join("node-apps").join(&nn);
            if old_dir.exists() {
                let _ = fs::rename(&old_dir, &new_dir);
            }
            app.name = nn;
            app.entry_path = Self::entry_rel(&app.name);
            app.log_path = Self::log_rel(&app.name);
        }
        if let Some(v) = params.entry_path {
            let v = v.trim().to_string();
            if !v.is_empty() {
                // 重新上传入口：复制覆盖运行目录中的入口文件（存的相对路径不变）
                Self::copy_entry_to_appdir(Path::new(&v), &app.name)?;
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
        if let Some(v) = params.auto_restart {
            app.auto_restart = v;
        }
        let out = app.clone();
        drop(inner);
        let _ = self.save();
        Ok(out)
    }

    pub fn delete(&self, id: &str) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let rm_name = inner.apps.iter().find(|a| a.id == id).map(|a| a.name.clone());
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
            // 删除应用运行目录（入口 JS / 日志 等）
            if let Some(n) = rm_name {
                let dir = paths::data_dir().join("node-apps").join(n);
                let _ = fs::remove_dir_all(&dir);
            }
            let _ = self.save();
        }
        removed
    }

    /// 用指定 node.exe 启动应用（stdout/stderr 重定向到 <app_data>/node-logs/<id>.log）
    pub fn start(&self, app_id: &str, node_exe: &Path) -> Result<(), String> {
        let app = self.get(app_id).ok_or_else(|| "未找到应用".to_string())?;
        let entry = paths::resolve_data_path(&app.entry_path);
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
        cmd.arg(&entry);
        for a in &app.args {
            cmd.arg(a);
        }
        if let Some(cwd) = entry.parent() {
            cmd.current_dir(cwd);
        }
        for (k, v) in &app.env_vars {
            cmd.env(k, v);
        }
        let log_rel = Self::log_rel(&app.name);
        let log_path = paths::resolve_data_path(&log_rel);
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
            a.log_path = log_rel;
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

    /// 只读快照：不改状态、不落盘（供看门狗判定意外退出）
    pub fn snapshot(&self) -> Vec<NodeApp> {
        self.inner.lock().unwrap().apps.clone()
    }

    /// 写回状态/pid/错误并落盘（看门狗复位或放弃时使用）
    pub fn set_status(
        &self,
        app_id: &str,
        status: NodeAppStatus,
        pid: Option<u32>,
        error: Option<String>,
    ) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();
        let a = inner
            .apps
            .iter_mut()
            .find(|a| a.id == app_id)
            .ok_or_else(|| format!("未找到 Node 应用: {}", app_id))?;
        a.status = status;
        a.pid = pid;
        a.last_error = error;
        let apps = inner.apps.clone();
        if let Some(parent) = inner.data_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(&apps) {
            let _ = fs::write(&inner.data_path, content);
        }
        Ok(())
    }

    /// 返回 auto_start 的应用（按 startup_order 升序），供启动编排协调器聚合
    pub fn auto_start_list(&self) -> Vec<NodeApp> {
        let apps = self.inner.lock().unwrap().apps.clone();
        let mut pick: Vec<NodeApp> = apps.into_iter().filter(|a| a.auto_start).collect();
        pick.sort_by_key(|a| a.startup_order);
        pick
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