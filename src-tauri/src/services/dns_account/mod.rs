//! DNS 服务商账号管理（新增/编辑/删除/被引用检查）。

pub mod migrate;

use std::path::PathBuf;
use std::sync::RwLock;

use anyhow::Result;

use crate::models::dns_account::DnsAccount;
use crate::utils::paths;

pub struct DnsAccountManager {
    accounts: RwLock<Vec<DnsAccount>>,
}

impl DnsAccountManager {
    pub fn new() -> Self {
        Self {
            accounts: RwLock::new(Self::load().unwrap_or_default()),
        }
    }

    pub fn store_path() -> PathBuf {
        paths::config_dir().join("dns-accounts.json")
    }

    fn load() -> Result<Vec<DnsAccount>> {
        let p = Self::store_path();
        if !p.exists() {
            return Ok(Vec::new());
        }
        Ok(serde_json::from_str(&std::fs::read_to_string(&p)?)?)
    }

    fn save(list: &[DnsAccount]) -> Result<()> {
        std::fs::write(Self::store_path(), serde_json::to_string_pretty(list)?)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<DnsAccount> {
        self.accounts.read().unwrap().clone()
    }

    pub fn get(&self, id: &str) -> Option<DnsAccount> {
        self.accounts
            .read()
            .unwrap()
            .iter()
            .find(|a| a.id == id)
            .cloned()
    }

    /// 新增或更新（按 id 匹配）并落盘。
    pub fn upsert(&self, a: DnsAccount) -> Result<()> {
        let mut l = self.accounts.write().unwrap();
        match l.iter_mut().find(|x| x.id == a.id) {
            Some(slot) => *slot = a,
            None => l.push(a),
        }
        Self::save(&l)
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let mut l = self.accounts.write().unwrap();
        l.retain(|a| a.id != id);
        Self::save(&l)
    }
}

impl Default for DnsAccountManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 执行一次性迁移：把旧 settings 里的 cloudflare token 变成一条账号，
/// 并绑定所有已启用 ACME 的站点。
pub fn run_startup_migration() -> anyhow::Result<()> {
    use anyhow::Context;

    let path = DnsAccountManager::store_path();
    let legacy_token = read_legacy_token();
    let id = format!("acc-{}", uuid::Uuid::new_v4());

    match migrate::plan(path.exists(), &legacy_token, id.clone()) {
        migrate::Plan::AlreadyDone => Ok(()),
        migrate::Plan::MarkOnly => {
            DnsAccountManager::save(&[]).context("写入空的 dns-accounts.json 失败")?;
            Ok(())
        }
        migrate::Plan::Create(account) => {
            DnsAccountManager::save(std::slice::from_ref(&account))
                .context("写入 dns-accounts.json 失败")?;
            bind_unbound_acme_sites(&id).context("绑定站点到新账号失败")?;
            tracing::info!(account = %account.name, "已从旧全局 DNS 配置迁移出一个账号");
            Ok(())
        }
    }
}

/// 从 settings.json 原始 JSON 取旧 token（结构体已删该字段，故按 Value 读）。
fn read_legacy_token() -> String {
    let p = paths::settings_path();
    let Ok(raw) = std::fs::read_to_string(&p) else {
        return String::new();
    };
    serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|v| v.get("cloudflare_api_token")?.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// 给所有「已启用 ACME 且未绑定账号」的站点绑上该账号，并落盘。
fn bind_unbound_acme_sites(account_id: &str) -> anyhow::Result<()> {
    let p = crate::services::website_manager::WebsiteManager::store_path();
    if !p.exists() {
        return Ok(());
    }
    let raw = std::fs::read_to_string(&p)?;
    let mut list: crate::models::website::WebsiteList = serde_json::from_str(&raw)?;
    let mut changed = false;
    for s in list.websites.iter_mut() {
        if s.ssl.acme && s.ssl.dns_account_id.is_none() {
            s.ssl.dns_account_id = Some(account_id.to_string());
            changed = true;
        }
    }
    if changed {
        std::fs::write(&p, serde_json::to_string_pretty(&list)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// store_path 必须落在 config 目录、文件名固定（改错目录会让数据丢失）
    #[test]
    fn store_path_is_in_config_dir() {
        let p = DnsAccountManager::store_path();
        assert_eq!(p.file_name().unwrap(), "dns-accounts.json");
        assert_eq!(p.parent().unwrap(), paths::config_dir());
    }

    /// 账号列表的序列化形状：Vec<DnsAccount> 的 JSON 必须是数组（不是对象）
    #[test]
    fn serializes_as_array() {
        let list = vec![DnsAccount {
            id: "a1".into(),
            name: "n".into(),
            provider: "cloudflare".into(),
            token: "t".into(),
            access_key_id: String::new(),
            access_key_secret: String::new(),
            zones: vec![],
            tested_at: None,
        }];
        let json = serde_json::to_string(&list).unwrap();
        assert!(json.starts_with('['), "必须是数组：{}", json);
        let back: Vec<DnsAccount> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].id, "a1");
    }
}
