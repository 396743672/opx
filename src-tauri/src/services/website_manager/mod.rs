pub mod nginx_conf;

use std::path::PathBuf;
use std::sync::RwLock;

use anyhow::Result;

use crate::models::website::{Site, WebsiteList};
use crate::utils::paths;

pub struct WebsiteManager {
    websites: RwLock<WebsiteList>,
}

impl WebsiteManager {
    pub fn new() -> Self {
        Self {
            websites: RwLock::new(Self::load().unwrap_or_default()),
        }
    }

    fn store_path() -> PathBuf {
        paths::config_dir().join("websites.json")
    }

    fn load() -> Result<WebsiteList> {
        let p = Self::store_path();
        if !p.exists() {
            return Ok(WebsiteList::default());
        }
        Ok(serde_json::from_str(&std::fs::read_to_string(&p)?)?)
    }

    fn save_list(list: &WebsiteList) -> Result<()> {
        std::fs::write(Self::store_path(), serde_json::to_string_pretty(list)?)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<Site> {
        self.websites.read().unwrap().websites.clone()
    }

    pub fn get(&self, id: &str) -> Option<Site> {
        self.websites.read().unwrap().websites.iter().find(|s| s.id == id).cloned()
    }

    /// 仅更新内存，不持久化（用于先验证再保存的场景）
    pub fn upsert_mem(&self, site: Site) -> Result<()> {
        let mut l = self.websites.write().unwrap();
        if let Some(existing) = l.websites.iter_mut().find(|s| s.id == site.id) {
            *existing = site;
        } else {
            l.websites.push(site);
        }
        Ok(())
    }

    pub fn persist(&self) -> Result<()> {
        Self::save_list(&self.websites.read().unwrap())
    }

    pub fn remove_mem(&self, id: &str) -> Result<()> {
        let mut l = self.websites.write().unwrap();
        l.websites.retain(|s| s.id != id);
        Ok(())
    }

    pub fn set_enabled_mem(&self, id: &str, enabled: bool) -> Result<()> {
        let mut l = self.websites.write().unwrap();
        if let Some(s) = l.websites.iter_mut().find(|s| s.id == id) {
            s.enabled = enabled;
        }
        Ok(())
    }

    /// 设置站点的手写模式标记：true 时该站点 conf 由源码视图维护、不参与表单重建
    pub fn set_custom_conf(&self, id: &str, custom: bool) -> Result<()> {
        let mut l = self.websites.write().unwrap();
        if let Some(s) = l.websites.iter_mut().find(|s| s.id == id) {
            s.custom_conf = custom;
        }
        Self::save_list(&l)
    }
}

impl Default for WebsiteManager {
    fn default() -> Self {
        Self::new()
    }
}
