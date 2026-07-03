use std::collections::HashMap;
use std::sync::Mutex;

use chrono::Local;
use once_cell::sync::Lazy;

#[derive(Debug, Clone, serde::Serialize)]
pub struct RegisteredProcess {
    pub installed_id: String,
    pub pid: u32,
    pub name: String,
    pub key: String,
    pub kind: String,
    pub started_at: i64,
}

pub struct ProcessRegistry {
    processes: HashMap<String, RegisteredProcess>, // key = installed_id
}

impl ProcessRegistry {
    fn new() -> Self {
        Self { processes: HashMap::new() }
    }

    pub fn register(
        &mut self,
        installed_id: String,
        pid: u32,
        name: String,
        key: String,
        kind: String,
    ) {
        let entry = RegisteredProcess {
            installed_id: installed_id.clone(),
            pid,
            name,
            key,
            kind,
            started_at: Local::now().timestamp(),
        };
        self.processes.insert(installed_id, entry);
    }

    pub fn unregister(&mut self, installed_id: &str) {
        self.processes.remove(installed_id);
    }

    pub fn get(&self, installed_id: &str) -> Option<&RegisteredProcess> {
        self.processes.get(installed_id)
    }

    pub fn list(&self) -> Vec<RegisteredProcess> {
        self.processes.values().cloned().collect()
    }

    pub fn drain(&mut self) -> Vec<RegisteredProcess> {
        let v: Vec<_> = self.processes.values().cloned().collect();
        self.processes.clear();
        v
    }
}

static REGISTRY: Lazy<Mutex<ProcessRegistry>> =
    Lazy::new(|| Mutex::new(ProcessRegistry::new()));

pub fn register(installed_id: String, pid: u32, name: String, key: String, kind: String) {
    REGISTRY.lock().unwrap().register(installed_id, pid, name, key, kind);
}

pub fn unregister(installed_id: &str) {
    REGISTRY.lock().unwrap().unregister(installed_id);
}

pub fn get(installed_id: &str) -> Option<RegisteredProcess> {
    REGISTRY.lock().unwrap().get(installed_id).cloned()
}

pub fn list() -> Vec<RegisteredProcess> {
    REGISTRY.lock().unwrap().list()
}

pub fn drain() -> Vec<RegisteredProcess> {
    REGISTRY.lock().unwrap().drain()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_get_returns_entry() {
        let mut reg = ProcessRegistry::new();
        reg.register(
            "uuid-1".to_string(),
            12345,
            "MySQL 8.4.10".to_string(),
            "mysql".to_string(),
            "Database".to_string(),
        );
        let entry = reg.get("uuid-1").unwrap();
        assert_eq!(entry.pid, 12345);
        assert_eq!(entry.key, "mysql");
    }

    #[test]
    fn unregister_removes_entry() {
        let mut reg = ProcessRegistry::new();
        reg.register(
            "uuid-2".to_string(), 111, "Test".to_string(),
            "redis".to_string(), "Cache".to_string(),
        );
        assert!(reg.get("uuid-2").is_some());
        reg.unregister("uuid-2");
        assert!(reg.get("uuid-2").is_none());
    }

    #[test]
    fn register_overwrites_same_id() {
        let mut reg = ProcessRegistry::new();
        reg.register(
            "uuid-3".to_string(), 100, "Old".to_string(),
            "mysql".to_string(), "Database".to_string(),
        );
        reg.register(
            "uuid-3".to_string(), 200, "New".to_string(),
            "mysql".to_string(), "Database".to_string(),
        );
        let entry = reg.get("uuid-3").unwrap();
        assert_eq!(entry.pid, 200);
        assert_eq!(entry.name, "New");
    }

    #[test]
    fn list_returns_all_entries() {
        let mut reg = ProcessRegistry::new();
        reg.register("a".to_string(), 1, "A".to_string(), "k".to_string(), "K".to_string());
        reg.register("b".to_string(), 2, "B".to_string(), "k".to_string(), "K".to_string());
        let list = reg.list();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn drain_clears_and_returns_all() {
        let mut reg = ProcessRegistry::new();
        reg.register("a".to_string(), 1, "A".to_string(), "k".to_string(), "K".to_string());
        reg.register("b".to_string(), 2, "B".to_string(), "k".to_string(), "K".to_string());
        let drained = reg.drain();
        assert_eq!(drained.len(), 2);
        assert!(reg.list().is_empty());
    }

    #[test]
    fn get_returns_none_for_unknown_id() {
        let reg = ProcessRegistry::new();
        assert!(reg.get("nonexistent").is_none());
    }
}
