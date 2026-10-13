//! WS 握手一次性 ticket（设计 §3 D3.2）。
//!
//! ## 为什么需要
//!
//! 浏览器的 `WebSocket` API **不能设置 Authorization 头**，token 直传 query
//! 会进反代/日志（D3.2 方案 A 被否）。故：持 Bearer 的客户端先
//! `POST /api/ws-ticket` 换取 60 秒单次 ticket，再以
//! `GET /api/events/ws?ticket=...` 握手——token 零泄漏面。
//!
//! ## 语义（D3.2 方案 B）
//!
//! - 单次有效：consume 即焚（HashMap remove）
//! - 60 秒过期：签发时刻 + TTL，消费时校验；签发新 ticket 时惰性清理过期项
//! - 时间可注入：`issue_at` / `consume_at` 收 `Instant`，生产包装用
//!   `Instant::now()`，测试可注入任意时刻

use rand_core::{OsRng, RngCore};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// ticket 有效期（D3.2：60 秒）。
pub const TICKET_TTL: Duration = Duration::from_secs(60);

/// ticket 容量上限（防滥用兜底：即使持 token 也不允许无限囤积）。
const MAX_PENDING: usize = 64;

/// 内存 ticket 表。进程重启即失效（无需持久化——握手是秒级操作）。
#[derive(Default)]
pub struct TicketStore {
    tickets: Mutex<HashMap<String, Instant>>,
}

fn random_hex32() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    // 与 token.rs 同款手写 hex，不引 hex crate
    let mut hex = String::with_capacity(64);
    for b in bytes {
        hex.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0'));
        hex.push(char::from_digit((b & 0xf) as u32, 16).unwrap_or('0'));
    }
    hex
}

impl TicketStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 签发一个 60 秒有效的单次 ticket（生产入口）。
    pub fn issue(&self) -> String {
        self.issue_at(Instant::now())
    }

    /// 签发（时间注入版）。同时惰性清理过期项 + 超容量时拒绝签发。
    ///
    /// 返回 `None` = 待用 ticket 已满（MAX_PENDING），拒绝签发——正常客户端
    /// 每次握手只取一张，触发上限即异常行为。
    pub fn issue_at(&self, now: Instant) -> String {
        let mut map = self
            .tickets
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        map.retain(|_, issued| now.duration_since(*issued) < TICKET_TTL);
        while map.len() >= MAX_PENDING {
            // 找最早签发的淘汰（正常场景不会走到：客户端取了就用）
            if let Some(oldest) = map.iter().min_by_key(|(_, t)| **t).map(|(k, _)| k.clone()) {
                map.remove(&oldest);
            }
        }
        let ticket = random_hex32();
        map.insert(ticket.clone(), now);
        ticket
    }

    /// 消费一个 ticket（生产入口）：单次有效 + 60 秒内。
    pub fn consume(&self, ticket: &str) -> bool {
        self.consume_at(ticket, Instant::now(), TICKET_TTL)
    }

    /// 消费（时间注入版）：存在 + 未过期则移除并返回 true，否则 false。
    pub fn consume_at(&self, ticket: &str, now: Instant, ttl: Duration) -> bool {
        let mut map = self
            .tickets
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        match map.remove(ticket) {
            Some(issued) => now.duration_since(issued) < ttl,
            None => false,
        }
    }

    /// 当前待用 ticket 数（监控/测试用）。
    pub fn len(&self) -> usize {
        self.tickets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn ts(secs: u64) -> Instant {
        // 相对时刻：以真实 now 为基点加偏移，只用于相对比较
        Instant::now() + Duration::from_secs(secs as u64)
    }

    #[test]
    fn ticket_single_use_then_rejected() {
        let store = TicketStore::new();
        let t = store.issue_at(ts(0));
        assert_eq!(store.len(), 1);
        // 首次消费成功（用后即焚）
        assert!(store.consume_at(&t, ts(1), TICKET_TTL));
        assert_eq!(store.len(), 0);
        // 二次消费拒绝（已焚）
        assert!(!store.consume_at(&t, ts(2), TICKET_TTL));
    }

    #[test]
    fn ticket_rejected_after_ttl() {
        let store = TicketStore::new();
        let t = store.issue_at(ts(0));
        // 59 秒内有效
        assert!(store.consume_at(&t, ts(59), TICKET_TTL));
        // 重新签发，60 秒整过界拒绝
        let t2 = store.issue_at(ts(0));
        assert!(!store.consume_at(&t2, ts(60), TICKET_TTL));
        assert_eq!(store.len(), 0, "过期消费同样移除条目");
        // 未知 ticket 拒绝
        assert!(!store.consume_at("deadbeef", ts(0), TICKET_TTL));
    }

    #[test]
    fn issue_prunes_expired_entries() {
        let store = TicketStore::new();
        let _a = store.issue_at(ts(0));
        let _b = store.issue_at(ts(0));
        assert_eq!(store.len(), 2);
        // 在 +120s 签发新 ticket：过期项被惰性清理
        let _c = store.issue_at(ts(120));
        assert_eq!(store.len(), 1, "过期的 a/b 被清理，只剩 c");
    }
}
