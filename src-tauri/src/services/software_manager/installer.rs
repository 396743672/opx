use crate::models::software::{InstalledSoftware, InstallParams, SoftwareMeta};
use crate::services::software_manager::providers::SoftwareProvider;
use anyhow::{Result, anyhow};
use std::path::{PathBuf, Path};
use std::collections::HashMap;

pub struct Installer {
    providers: HashMap<String, Box<dyn SoftwareProvider>>,
}

impl Installer {
    pub fn new() -> Self {
        let mut providers = HashMap::new();

        // register all providers
        providers.insert(
            "mysql".to_string(),
            Box::new(crate::services::software_manager::providers::MysqlProvider)
        );
        providers.insert(
            "jdk".to_string(),
            Box::new(crate::services::software_manager::providers::JdkProvider)
        );
        providers.insert(
            "redis".to_string(),
            Box::new(crate::services::software_manager::providers::RedisProvider)
        );
        providers.insert(
            "nginx".to_string(),
            Box::new(crate::services::software_manager::providers::NginxProvider)
        );

        Self { providers }
    }

    pub fn list_available(&self) -> Vec<SoftwareMeta> {
        self.providers.values().map(|p| p.meta()).collect()
    }

    pub fn get_provider(&self, key: &str) -> Option<&dyn SoftwareProvider> {
        self.providers.get(key).map(|p| p.as_ref())
    }
}