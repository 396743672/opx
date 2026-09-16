pub mod audit;
pub mod audit_log;
pub mod backup;
pub mod backup_scheduler;
pub mod catalog;
pub mod config_editor;
pub mod health_check;
pub mod installer;
pub mod lifecycle;
pub mod log_viewer;
pub mod log_watcher;
pub mod netutils;
pub mod process_monitor;
pub mod providers;
pub mod uninstall_guard;

use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

use anyhow::Result;
use chrono::{Local, NaiveDateTime};

use crate::models::settings::AppSettings;
use crate::models::software::{Catalog, InstalledSoftware, InstalledSoftwareList, SoftwareStatus};
use crate::utils::paths;

/// 进行中的安装任务状态（用于查重和未来取消）
pub struct InstallTaskState {
    pub key: String,
    pub version: String,
    pub created_at: chrono::DateTime<Local>,
}

pub struct SoftwareManager {
    catalog: RwLock<Catalog>,
    installed: RwLock<InstalledSoftwareList>,
    install_tasks: Mutex<HashMap<String, InstallTaskState>>,
}

/// 启动对账：纠正跨启动残留的状态。
///
/// - `Running` + pid 存活 → **保留**（强杀场景子进程仍在跑，收养：监控/停止照常可用）。
///   ponytail: 只查 pid 存在，不校验进程名——pid 被复用且恰好存活时误收养，概率极低，
///   表现为停止失败，重试即可。
/// - `Running` + pid 已死 / `Starting/Stopping/Initializing`（瞬态不该跨启动存活）→
///   重置 Stopped 并清 pid。
/// - Stopped/Error/Unknown 不动。
///
/// 返回是否有修改（调用方据此落盘，保持内存与 installed.json 一致）。
fn reconcile_stale_statuses(list: &mut InstalledSoftwareList) -> bool {
    let mut changed = false;
    for s in &mut list.software {
        let reset = match s.status {
            SoftwareStatus::Running => {
                s.pid.map(|p| health_check::is_process_alive(p)) != Some(true)
            }
            SoftwareStatus::Starting | SoftwareStatus::Stopping | SoftwareStatus::Initializing => {
                true
            }
            _ => false,
        };
        if reset {
            s.status = SoftwareStatus::Stopped;
            s.pid = None;
            changed = true;
        }
    }
    changed
}

impl SoftwareManager {
    pub fn new() -> Self {
        let builtin = catalog::build_builtin_catalog();
        let installed = Self::load_installed_list().unwrap_or_default();
        Self {
            catalog: RwLock::new(builtin),
            installed: RwLock::new(installed),
            install_tasks: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_catalog(&self) -> Catalog {
        self.catalog.read().unwrap().clone()
    }

    /// 用新的 catalog 替换内部 catalog（远程合并后调用）
    pub fn set_catalog(&self, catalog: Catalog) {
        *self.catalog.write().unwrap() = catalog;
    }

    /// 把某软件动态拉取的远程版本合并进 catalog（内置版本优先、去重）。
    /// 安装时 install_software 从 catalog 查版本，故网络版本必须先写回 catalog。
    pub fn merge_entry_versions(
        &self,
        key: &str,
        remote_versions: Vec<crate::models::software::CatalogVersion>,
    ) {
        let mut catalog = self.catalog.write().unwrap();
        if let Some(entry) = catalog.entries.iter_mut().find(|e| e.key == key) {
            let builtin = std::mem::take(&mut entry.versions);
            entry.versions = catalog::merge_versions(builtin, Some(remote_versions));
        }
    }

    /// 获取已安装软件列表（返回 install_path 为相对路径，供前端显示）
    pub fn get_installed(&self) -> Vec<InstalledSoftware> {
        self.installed.read().unwrap().software.clone()
    }

    pub fn is_installed(&self, key: &str, version: &str) -> bool {
        let installed = self.installed.read().unwrap();
        installed
            .software
            .iter()
            .any(|s| s.key == key && s.version == version)
    }

    pub fn is_installing(&self, key: &str, version: &str) -> bool {
        let tasks = self.install_tasks.lock().unwrap();
        tasks.values().any(|t| t.key == key && t.version == version)
    }

    pub fn add_install_task(&self, install_id: String, key: String, version: String) {
        let mut tasks = self.install_tasks.lock().unwrap();
        tasks.insert(
            install_id,
            InstallTaskState {
                key,
                version,
                created_at: Local::now(),
            },
        );
    }

    pub fn remove_install_task(&self, install_id: &str) {
        let mut tasks = self.install_tasks.lock().unwrap();
        tasks.remove(install_id);
    }

    pub fn add_installed(&self, software: InstalledSoftware) -> Result<()> {
        let mut installed = self.installed.write().unwrap();
        installed.software.push(software);
        Self::save_installed_list(&installed)?;
        Ok(())
    }

    /// 卸载已安装软件：删除安装目录 + 从 installed.json 移除记录 + 若是默认 JRE 则清除
    /// 返回被移除的 InstalledSoftware（供命令层用），找不到返回 Err
    pub fn remove_installed(&self, installed_id: &str) -> Result<InstalledSoftware> {
        let removed;
        {
            let mut installed = self.installed.write().unwrap();
            let pos = installed
                .software
                .iter()
                .position(|s| s.id == installed_id)
                .ok_or_else(|| anyhow::anyhow!("未找到安装记录: {}", installed_id))?;
            removed = installed.software.remove(pos);
            Self::save_installed_list(&installed)?;
        } // 写锁在此释放

        // 删除安装目录（锁已释放）
        let install_path = paths::resolve_install_path(&removed.install_path);
        if install_path.exists() {
            if let Err(e) = std::fs::remove_dir_all(&install_path) {
                eprintln!(
                    "[software] 清理安装目录失败 {}: {}",
                    install_path.display(),
                    e
                );
                // 不阻断卸载流程——记录已从 installed.json 移除，目录残留可手动清理
            }
        }

        // 若是默认 JRE，清除 jre_default_id
        if removed.key == "jre" {
            if let Some(default_id) = self.get_jre_default() {
                if default_id == removed.id {
                    if let Err(e) = self.update_jre_default(None) {
                        eprintln!("[software] 清除默认 JRE 失败: {}", e);
                    }
                }
            }
        }

        Ok(removed)
    }

    fn load_installed_list() -> Result<InstalledSoftwareList> {
        let path = paths::config_dir().join("installed.json");
        if !path.exists() {
            return Ok(InstalledSoftwareList::default());
        }
        let content = std::fs::read_to_string(&path)?;
        let mut list: InstalledSoftwareList = serde_json::from_str(&content)?;
        if reconcile_stale_statuses(&mut list) {
            let _ = Self::save_installed_list(&list);
        }
        Ok(list)
    }

    fn save_installed_list(list: &InstalledSoftwareList) -> Result<()> {
        let path = paths::config_dir().join("installed.json");
        let content = serde_json::to_string_pretty(list)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// 更新 settings.json 的 jre_default_id 字段（原子写）
    pub fn update_jre_default(&self, jre_id: Option<String>) -> Result<()> {
        let sp = paths::settings_path();
        let mut settings = if sp.exists() {
            let content = std::fs::read_to_string(&sp)?;
            serde_json::from_str::<AppSettings>(&content).unwrap_or_default()
        } else {
            AppSettings::default()
        };
        settings.jre_default_id = jre_id;
        let tmp = sp.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(&settings)?;
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, &sp)?;
        Ok(())
    }

    pub fn get_jre_default(&self) -> Option<String> {
        let sp = paths::settings_path();
        if !sp.exists() {
            return None;
        }
        let content = std::fs::read_to_string(&sp).ok()?;
        let settings = serde_json::from_str::<AppSettings>(&content).ok()?;
        settings.jre_default_id
    }

    /// 按 installed_id 查找单条记录（返回时解析 install_path 为绝对路径）
    pub fn find_installed(&self, installed_id: &str) -> Option<InstalledSoftware> {
        let installed = self.installed.read().unwrap();
        let mut sw = installed
            .software
            .iter()
            .find(|s| s.id == installed_id)
            .cloned()?;
        sw.install_path = paths::resolve_install_path(&sw.install_path)
            .to_string_lossy()
            .to_string();
        Some(sw)
    }

    /// 更新单条记录的运行时字段（status / pid / last_started_at 等）
    /// 同时持久化到 installed.json
    ///
    /// 注：status 与 pid 是无条件覆盖（pid 传 None 表示清除 PID）。
    /// last_started_at / last_stopped_at / last_error 是条件更新——传 None
    /// 表示"不改"，需显式清空 last_error 请用 clear_last_error()。
    pub fn update_runtime_fields(
        &self,
        installed_id: &str,
        status: SoftwareStatus,
        pid: Option<u32>,
        last_started_at: Option<NaiveDateTime>,
        last_stopped_at: Option<NaiveDateTime>,
        last_error: Option<String>,
    ) -> Result<()> {
        let mut installed = self.installed.write().unwrap();
        let item = installed
            .software
            .iter_mut()
            .find(|s| s.id == installed_id)
            .ok_or_else(|| anyhow::anyhow!("未找到安装记录: {}", installed_id))?;
        item.status = status;
        item.pid = pid;
        if let Some(t) = last_started_at {
            item.last_started_at = Some(t);
        }
        if let Some(t) = last_stopped_at {
            item.last_stopped_at = Some(t);
        }
        if let Some(e) = last_error {
            item.last_error = Some(e);
        }
        Self::save_installed_list(&installed)?;
        Ok(())
    }

    /// 显式清空 last_error（update_runtime_fields 传 None 表示不改）
    pub fn clear_last_error(&self, installed_id: &str) -> Result<()> {
        let mut installed = self.installed.write().unwrap();
        let item = installed
            .software
            .iter_mut()
            .find(|s| s.id == installed_id)
            .ok_or_else(|| anyhow::anyhow!("未找到安装记录: {}", installed_id))?;
        item.last_error = None;
        Self::save_installed_list(&installed)?;
        Ok(())
    }

    /// 更新 installed.json 中某条记录的 config（配置编辑后调用）
    pub fn update_config(&self, installed_id: &str, config: serde_json::Value) -> Result<()> {
        let mut installed = self.installed.write().unwrap();
        let item = installed
            .software
            .iter_mut()
            .find(|s| s.id == installed_id)
            .ok_or_else(|| anyhow::anyhow!("未找到安装记录: {}", installed_id))?;
        item.config = config;
        Self::save_installed_list(&installed)?;
        Ok(())
    }

    /// 更新启动设置（auto_start + startup_order + auto_restart）
    pub fn update_startup_settings(
        &self,
        installed_id: &str,
        auto_start: bool,
        order: u32,
        auto_restart: bool,
    ) -> Result<()> {
        let mut installed = self.installed.write().unwrap();
        let item = installed
            .software
            .iter_mut()
            .find(|s| s.id == installed_id)
            .ok_or_else(|| anyhow::anyhow!("未找到安装记录: {}", installed_id))?;
        item.auto_start_on_app_start = auto_start;
        item.startup_order = order;
        item.auto_restart = auto_restart;
        Self::save_installed_list(&installed)?;
        Ok(())
    }

    /// 更新软件依赖清单（depends_on）。自动去重、剔除自引用与不存在项之后保存。
    pub fn update_dependencies(&self, installed_id: &str, deps: Vec<String>) -> Result<()> {
        let mut installed = self.installed.write().unwrap();
        // 有效依赖 id：必须已安装（先取集合，避免与 item 可变借用冲突）
        let valid_ids: std::collections::HashSet<String> =
            installed.software.iter().map(|s| s.id.clone()).collect();
        let item = installed
            .software
            .iter_mut()
            .find(|s| s.id == installed_id)
            .ok_or_else(|| anyhow::anyhow!("未找到安装记录: {}", installed_id))?;
        // 有效依赖：非自身、非空、必须已安装、去重
        let mut cleaned: Vec<String> = Vec::new();
        for d in deps {
            if d == installed_id || d.is_empty() || !valid_ids.contains(&d) {
                continue;
            }
            if !cleaned.contains(&d) {
                cleaned.push(d);
            }
        }
        item.depends_on = cleaned;
        Self::save_installed_list(&installed)?;
        Ok(())
    }

    /// 获取所有 auto_start=true 的实例（按 startup_order 升序排序，返回时解析路径）
    pub fn list_auto_start(&self) -> Vec<InstalledSoftware> {
        let installed = self.installed.read().unwrap();
        let mut v: Vec<_> = installed
            .software
            .iter()
            .filter(|s| s.auto_start_on_app_start)
            .cloned()
            .collect();
        for s in &mut v {
            s.install_path = paths::resolve_install_path(&s.install_path)
                .to_string_lossy()
                .to_string();
        }
        v.sort_by_key(|s| s.startup_order);
        v
    }

    /// 保存自定义软件的启动命令（任务 10 自定义启动命令命令层调用）
    pub fn set_custom_start_command(
        &self,
        installed_id: &str,
        cmd: crate::models::software::CustomStartCommand,
    ) -> Result<()> {
        let mut installed = self.installed.write().unwrap();
        let item = installed
            .software
            .iter_mut()
            .find(|s| s.id == installed_id)
            .ok_or_else(|| anyhow::anyhow!("未找到安装记录: {}", installed_id))?;
        item.custom_start_command = Some(cmd);
        Self::save_installed_list(&installed)?;
        Ok(())
    }
}

impl Default for SoftwareManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::software::{InstalledSoftware, SoftwareStatus};

    /// 测试条目：走 JSON 构造避免逐字段填 20+ 个字段。
    /// 伪 pid 用不可能存活的极大值（走「死 pid → 重置」路径）。
    fn entry(status: &str, pid: Option<u32>) -> InstalledSoftware {
        let json = serde_json::json!({
            "id": "t1", "key": "mysql", "version": "1.0", "name": "T",
            "install_path": "apps/mysql/1.0",
            "install_time": "2026-01-01T00:00:00",
            "status": status, "port": 3306, "config": {},
            "is_custom": false, "auto_start_on_app_start": false,
            "startup_order": 0,
            "source": {"Mirror": {"mirror_name": "m", "url": "https://x"}},
            "pid": pid,
        });
        serde_json::from_value(json).expect("test entry parses")
    }

    fn list(items: Vec<InstalledSoftware>) -> InstalledSoftwareList {
        InstalledSoftwareList { software: items }
    }

    #[test]
    fn running_with_dead_pid_resets_to_stopped() {
        let mut l = list(vec![entry("Running", Some(9_999_999))]);
        assert!(reconcile_stale_statuses(&mut l), "死 pid 应触发修改");
        assert_eq!(l.software[0].status, SoftwareStatus::Stopped);
        assert_eq!(l.software[0].pid, None);
    }

    #[test]
    fn transient_states_reset_regardless_of_pid() {
        for st in ["Starting", "Stopping", "Initializing"] {
            let mut l = list(vec![entry(st, Some(std::process::id()))]);
            assert!(reconcile_stale_statuses(&mut l), "{st} 应无条件重置");
            assert_eq!(l.software[0].status, SoftwareStatus::Stopped);
        }
    }

    #[test]
    fn settled_states_untouched() {
        let mut l = list(vec![
            entry("Stopped", None),
            entry("Error", None),
            entry("Unknown", None),
        ]);
        assert!(!reconcile_stale_statuses(&mut l), "稳定态不应触发修改/落盘");
        assert_eq!(l.software[0].status, SoftwareStatus::Stopped);
        assert_eq!(l.software[1].status, SoftwareStatus::Error);
        assert_eq!(l.software[2].status, SoftwareStatus::Unknown);
    }

    #[test]
    fn running_without_pid_resets() {
        // Running 但 pid 缺失（数据异常）：无法探活，按死处理
        let mut l = list(vec![entry("Running", None)]);
        assert!(reconcile_stale_statuses(&mut l));
        assert_eq!(l.software[0].status, SoftwareStatus::Stopped);
    }
}
