use std::sync::Arc;

use tauri::State;

use crate::models::dns_account::DnsAccount;
use crate::services::acme::dns::provider_for_account;
use crate::services::dns_account::DnsAccountManager;
use crate::services::website_manager::WebsiteManager;
use crate::{audited, audited_async};

#[tauri::command]
pub fn list_dns_accounts(m: State<'_, Arc<DnsAccountManager>>) -> Vec<DnsAccount> {
    m.list()
}

#[tauri::command]
pub fn save_dns_account(
    m: State<'_, Arc<DnsAccountManager>>,
    account: DnsAccount,
) -> Result<(), String> {
    let target = format!("{} ({})", account.name, account.provider);
    audited!("save_dns_account", target, "", {
        m.upsert(account).map_err(|e| format!("{:#}", e))
    })
}

/// 删除账号；仍被站点引用时拒绝（否则那些站点会突然签不出证书）。
#[tauri::command]
pub fn delete_dns_account(
    m: State<'_, Arc<DnsAccountManager>>,
    wm: State<'_, Arc<WebsiteManager>>,
    id: String,
) -> Result<(), String> {
    let target = id.clone();
    audited!("delete_dns_account", target, "", {
        let refs: Vec<String> = wm
            .list()
            .into_iter()
            .filter(|s| s.ssl.dns_account_id.as_deref() == Some(id.as_str()))
            .map(|s| s.name)
            .collect();
        if !refs.is_empty() {
            return Err(format!(
                "该账号仍被 {} 个站点引用：{}。请先在站点里改掉再删除。",
                refs.len(),
                refs.join("、")
            ));
        }
        m.remove(&id).map_err(|e| format!("{:#}", e))
    })
}

/// 测试账号连通：拉 zone 列表并缓存（只验读权限；写权限留到签发时暴露）。
///
/// 返回**落盘后的账号**而非仅 zone 列表：前端拿它覆盖表单，避免「测试成功
/// 后再保存」用旧的空 zones 把刚缓存的结果抹掉。
#[tauri::command]
pub async fn test_dns_account(
    m: State<'_, Arc<DnsAccountManager>>,
    id: String,
) -> Result<DnsAccount, String> {
    let target = id.clone();
    audited_async!("test_dns_account", target, "", {
        let mut account = m.get(&id).ok_or_else(|| format!("未找到账号: {}", id))?;
        let provider = provider_for_account(&account)
            .ok_or_else(|| "凭证不完整或服务商不支持".to_string())?;
        account.zones = provider.list_zones().await.map_err(|e| format!("{:#}", e))?;
        account.tested_at = Some(chrono::Local::now().to_rfc3339());
        m.upsert(account.clone()).map_err(|e| format!("{:#}", e))?;
        Ok(account)
    })
}
