//! 旧全局 DNS 配置 → DNS 账号的一次性迁移。
//!
//! 迁移信号是「dns-accounts.json 不存在」，不是「旧字段有值」：
//! 必须区分「从没配过」（不建账号）与「迁移后用户删光了账号」
//! （不能每次启动又冒出来一个）。

use crate::models::dns_account::DnsAccount;

/// 迁移决策。
#[derive(Debug, PartialEq)]
pub enum Plan {
    /// 账号文件已存在：什么都不做
    AlreadyDone,
    /// 建一条账号（携带可直接落盘的 DnsAccount）
    Create(DnsAccount),
    /// 旧配置为空：只标记「迁移已完成」，不建账号
    MarkOnly,
}

/// 给定「账号文件是否已存在」与「旧 settings 里的 cloudflare token」，决定怎么迁。
/// `id` 由调用方生成（便于测试注入固定值）。
pub fn plan(account_file_exists: bool, legacy_token: &str, id: String) -> Plan {
    if account_file_exists {
        return Plan::AlreadyDone;
    }
    let token = legacy_token.trim();
    if token.is_empty() {
        return Plan::MarkOnly;
    }
    Plan::Create(DnsAccount {
        id,
        name: "默认 Cloudflare".to_string(),
        provider: "cloudflare".to_string(),
        token: token.to_string(),
        access_key_id: String::new(),
        access_key_secret: String::new(),
        zones: Vec::new(),
        tested_at: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn already_migrated_file_is_left_alone() {
        // 哪怕旧 token 还有值，文件存在就绝不重复迁移
        assert_eq!(plan(true, "tok", "x".into()), Plan::AlreadyDone);
        assert_eq!(plan(true, "", "x".into()), Plan::AlreadyDone);
    }

    #[test]
    fn creates_account_when_legacy_token_present() {
        match plan(false, "tok-123", "acc-1".into()) {
            Plan::Create(a) => {
                assert_eq!(a.id, "acc-1");
                assert_eq!(a.provider, "cloudflare");
                assert_eq!(a.token, "tok-123");
                assert_eq!(a.name, "默认 Cloudflare");
                assert!(a.zones.is_empty());
                assert!(a.tested_at.is_none());
            }
            other => panic!("应建账号，实际 {:?}", other),
        }
    }

    #[test]
    fn marks_only_when_legacy_token_blank() {
        assert_eq!(plan(false, "", "x".into()), Plan::MarkOnly);
        // 纯空白也算空，别建出一个 token 全空格的账号
        assert_eq!(plan(false, "   ", "x".into()), Plan::MarkOnly);
    }

    #[test]
    fn token_is_trimmed() {
        match plan(false, "  tok  ", "acc-1".into()) {
            Plan::Create(a) => assert_eq!(a.token, "tok"),
            other => panic!("应建账号，实际 {:?}", other),
        }
    }
}
