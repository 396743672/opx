pub mod catalog;
pub mod installer;
pub mod providers;

use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

use anyhow::Result;
use chrono::Local;

use crate::models::settings::AppSettings;
use crate::models::software::{Catalog, InstalledSoftware, InstalledSoftwareList};
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
}
