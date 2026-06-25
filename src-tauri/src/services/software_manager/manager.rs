use crate::models::software::{InstalledSoftware, InstalledSoftwareList, InstallParams};
use crate::services::software_manager::Installer;
use crate::utils::file;
use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

pub struct SoftwareManager {
    installer: Installer,
    data_path: PathBuf,
}

impl SoftwareManager {
    pub fn new(app_root: &Path) -> Self {
        let data_path = app_root.join("config").join("installed.json");
        Self {
            installer: Installer::new(),
            data_path,
        }
    }

    pub fn list_available(&self) -> Vec<SoftwareMeta> {
        self.installer.list_available()
    }

    pub fn list_installed(&self) -> Result<InstalledSoftwareList> {
        if !self.data_path.exists() {
            return Ok(InstalledSoftwareList::default());
        }
        let list = file::read_json::<InstalledSoftwareList>(&self.data_path)?;
        Ok(list)
    }

    pub fn save_installed(&self, list: &InstalledSoftwareList) -> Result<()> {
        file::write_json(&self.data_path, list)
    }
}