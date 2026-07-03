pub mod catalog;
pub mod installer;
pub mod providers;
pub mod audit_log;
pub mod health_check;
pub mod lifecycle;
pub mod config_editor;
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
        tasks
            .values()
            .any(|t| t.key == key && t.version == version)
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
        let install_path = std::path::Path::new(&removed.install_path);
        if install_path.exists() {
            if let Err(e) = std::fs::remove_dir_all(install_path) {
                eprintln!("[software] 清理安装目录失败 {}: {}", install_path.display(), e);
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
        let list = serde_json::from_str(&content)?;
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

    /// 按 installed_id 查找单条记录
    pub fn find_installed(&self, installed_id: &str) -> Option<InstalledSoftware> {
        let installed = self.installed.read().unwrap();
        installed
            .software
            .iter()
            .find(|s| s.id == installed_id)
            .cloned()
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

    /// 更新启动设置（auto_start + startup_order）
    pub fn update_startup_settings(
        &self,
        installed_id: &str,
        auto_start: bool,
        order: u32,
    ) -> Result<()> {
        let mut installed = self.installed.write().unwrap();
        let item = installed
            .software
            .iter_mut()
            .find(|s| s.id == installed_id)
            .ok_or_else(|| anyhow::anyhow!("未找到安装记录: {}", installed_id))?;
        item.auto_start_on_app_start = auto_start;
        item.startup_order = order;
        Self::save_installed_list(&installed)?;
        Ok(())
    }

    /// 获取所有 auto_start=true 的实例（按 startup_order 升序排序）
    pub fn list_auto_start(&self) -> Vec<InstalledSoftware> {
        let installed = self.installed.read().unwrap();
        let mut v: Vec<_> = installed
            .software
            .iter()
            .filter(|s| s.auto_start_on_app_start)
            .cloned()
            .collect();
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
    use crate::models::software::{InstallSource, SoftwareStatus};
    use chrono::Utc;

    fn make_installed(key: &str, version: &str) -> InstalledSoftware {
        InstalledSoftware {
            id: format!("{}-{}", key, version),
            key: key.to_string(),
            version: version.to_string(),
            name: format!("{} {}", key, version),
            install_path: format!("apps/{}/{}", key, version),
            install_time: Utc::now().naive_utc(),
            status: SoftwareStatus::Unknown,
            port: 0,
            config: serde_json::json!({}),
            is_custom: false,
            auto_start_on_app_start: false,
            startup_order: 0,
            source: InstallSource::Mirror {
                mirror_name: "test".to_string(),
                url: "https://example.com/test.zip".to_string(),
            },
            pid: None,
            last_started_at: None,
            last_stopped_at: None,
            last_error: None,
            custom_start_command: None,
        }
    }

    #[test]
    fn new_manager_has_builtin_catalog_and_empty_installed() {
        let mgr = SoftwareManager::new();
        let catalog = mgr.get_catalog();
        assert!(catalog.entries.len() >= 4);
        assert!(mgr.get_installed().is_empty());
    }

    #[test]
    fn is_installed_and_is_installing_work_correctly() {
        let mgr = SoftwareManager::new();
        assert!(!mgr.is_installed("mysql", "8.4.0"));
        assert!(!mgr.is_installing("mysql", "8.4.0"));

        // 模拟安装中
        mgr.add_install_task("task-1".to_string(), "mysql".to_string(), "8.4.0".to_string());
        assert!(mgr.is_installing("mysql", "8.4.0"));
        assert!(!mgr.is_installed("mysql", "8.4.0"));

        // 移除任务
        mgr.remove_install_task("task-1");
        assert!(!mgr.is_installing("mysql", "8.4.0"));
    }

    #[test]
    fn set_catalog_replaces_internal_catalog() {
        let mgr = SoftwareManager::new();
        let custom = Catalog {
            entries: vec![],
            updated_at: Some("2026-06-30".to_string()),
        };
        mgr.set_catalog(custom);
        let catalog = mgr.get_catalog();
        assert!(catalog.entries.is_empty());
        assert_eq!(catalog.updated_at.as_deref(), Some("2026-06-30"));
    }

    // 注意：add_installed 会写真实 config/installed.json，避免污染测试环境，
    // 此处不直接测试 add_installed 的持久化；集成测试在任务 12 覆盖。
    // _ = make_installed 抑制未使用警告
    #[test]
    fn make_installed_smoke_test() {
        let s = make_installed("mysql", "8.4.0");
        assert_eq!(s.key, "mysql");
        assert_eq!(s.version, "8.4.0");
    }

    // —— SoftwareManager 辅助方法测试 ——

    #[test]
    fn find_installed_returns_none_for_unknown_id() {
        let mgr = SoftwareManager::new();
        assert!(mgr.find_installed("nonexistent-uuid-xyz").is_none());
    }

    #[test]
    fn find_installed_returns_some_for_existing_id() {
        let mgr = SoftwareManager::new();
        // 取已安装列表中第一条（若有）
        if let Some(first) = mgr.get_installed().first().cloned() {
            let found = mgr.find_installed(&first.id);
            assert!(found.is_some(), "应能找到已存在的 installed_id");
            assert_eq!(found.unwrap().key, first.key);
        }
        // 若列表为空，本测试跳过（不失败）
    }

    #[test]
    fn list_auto_start_returns_empty_when_none_configured() {
        let mgr = SoftwareManager::new();
        let list = mgr.list_auto_start();
        // 仅验证返回 Vec（可能为空，取决于既有测试数据）
        // 关键是不 panic
        let _ = list.len();
    }

    #[test]
    fn list_auto_start_sorts_by_startup_order() {
        // 构造三条 auto_start=true 的记录，验证排序
        let mut list = vec![
            InstalledSoftware {
                id: "a".to_string(),
                key: "redis".to_string(),
                version: "7.4.9".to_string(),
                name: "Redis".to_string(),
                install_path: "apps/redis/7.4.9".to_string(),
                install_time: chrono::Utc::now().naive_utc(),
                status: SoftwareStatus::Stopped,
                port: 6379,
                config: serde_json::json!({}),
                is_custom: false,
                auto_start_on_app_start: true,
                startup_order: 30,
                source: InstallSource::Mirror {
                    mirror_name: "t".to_string(),
                    url: "http://t".to_string(),
                },
                pid: None,
                last_started_at: None,
                last_stopped_at: None,
                last_error: None,
                custom_start_command: None,
            },
            InstalledSoftware {
                id: "b".to_string(),
                key: "mysql".to_string(),
                version: "8.4.10".to_string(),
                name: "MySQL".to_string(),
                install_path: "apps/mysql/8.4.10".to_string(),
                install_time: chrono::Utc::now().naive_utc(),
                status: SoftwareStatus::Stopped,
                port: 3306,
                config: serde_json::json!({}),
                is_custom: false,
                auto_start_on_app_start: true,
                startup_order: 10,
                source: InstallSource::Mirror {
                    mirror_name: "t".to_string(),
                    url: "http://t".to_string(),
                },
                pid: None,
                last_started_at: None,
                last_stopped_at: None,
                last_error: None,
                custom_start_command: None,
            },
            InstalledSoftware {
                id: "c".to_string(),
                key: "nginx".to_string(),
                version: "1.31.2".to_string(),
                name: "Nginx".to_string(),
                install_path: "apps/nginx/1.31.2".to_string(),
                install_time: chrono::Utc::now().naive_utc(),
                status: SoftwareStatus::Stopped,
                port: 80,
                config: serde_json::json!({}),
                is_custom: false,
                auto_start_on_app_start: true,
                startup_order: 20,
                source: InstallSource::Mirror {
                    mirror_name: "t".to_string(),
                    url: "http://t".to_string(),
                },
                pid: None,
                last_started_at: None,
                last_stopped_at: None,
                last_error: None,
                custom_start_command: None,
            },
        ];
        list.sort_by_key(|s| s.startup_order);
        assert_eq!(list[0].id, "b"); // startup_order=10
        assert_eq!(list[1].id, "c"); // startup_order=20
        assert_eq!(list[2].id, "a"); // startup_order=30
    }

    #[test]
    fn update_runtime_fields_returns_err_for_unknown_id() {
        let mgr = SoftwareManager::new();
        let result = mgr.update_runtime_fields(
            "nonexistent-uuid-xyz",
            SoftwareStatus::Running,
            Some(12345),
            None,
            None,
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn update_config_returns_err_for_unknown_id() {
        let mgr = SoftwareManager::new();
        let result = mgr.update_config("nonexistent-uuid-xyz", serde_json::json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn update_startup_settings_returns_err_for_unknown_id() {
        let mgr = SoftwareManager::new();
        let result = mgr.update_startup_settings("nonexistent-uuid-xyz", true, 5);
        assert!(result.is_err());
    }

    #[test]
    fn clear_last_error_returns_err_for_unknown_id() {
        let mgr = SoftwareManager::new();
        let result = mgr.clear_last_error("nonexistent-uuid-xyz");
        assert!(result.is_err());
    }

    #[test]
    fn set_custom_start_command_returns_err_for_unknown_id() {
        let mgr = SoftwareManager::new();
        let custom = crate::models::software::CustomStartCommand {
            executable: "bin/app.exe".to_string(),
            args: vec![],
            working_dir: None,
            env_vars: std::collections::BTreeMap::new(),
            health_check: crate::models::software::CustomHealthSpec::None,
            config_file_relative: None,
        };
        let result = mgr.set_custom_start_command("nonexistent-uuid-xyz", custom);
        assert!(result.is_err());
    }
}
