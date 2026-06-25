use crate::models::springboot::{SpringApp, SpringAppList, AppGroup, AppStatus};
use crate::services::springboot_manager::types::ProcessManager;
use crate::utils::file;
use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

pub struct SpringBootManager {
    data_path: PathBuf,
    process_manager: ProcessManager,
}

impl SpringBootManager {
    pub fn new(app_root: &Path) -> Self {
        let data_path = app_root.join("config").join("springboot.json");
        Self {
            data_path,
            process_manager: ProcessManager::new(),
        }
    }

    pub fn list_applications(&self) -> Result<SpringAppList> {
        if !self.data_path.exists() {
            return Ok(SpringAppList::default());
        }
        let list = file::read_json::<SpringAppList>(&self.data_path)?;
        Ok(list)
    }

    pub fn save_applications(&self, list: &SpringAppList) -> Result<()> {
        file::write_json(&self.data_path, list)
    }

    pub fn start_application(&self, app: &mut SpringApp) -> Result<()> {
        self.process_manager.start(app)?;
        app.status = if self.process_manager.is_running(&app.id) {
            AppStatus::Running
        } else {
            AppStatus::Error
        };
        self.save_applications(&self.list_applications()?)?;
        Ok(())
    }

    pub fn stop_application(&self, app: &mut SpringApp) -> Result<()> {
        self.process_manager.stop(&app.id)?;
        app.status = AppStatus::Stopped;
        self.save_applications(&self.list_applications()?)?;
        Ok(())
    }
}