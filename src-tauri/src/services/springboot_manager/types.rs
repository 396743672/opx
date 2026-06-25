use crate::models::springboot::{SpringApp, AppGroup, JvmInfo, SpringAppList};
use anyhow::Result;
use std::collections::HashMap;
use std::process::{Child, Command};
use std::sync::{Mutex, MutexGuard};

pub struct ProcessManager {
    processes: Mutex<HashMap<String, Child>>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            processes: Mutex::new(HashMap::new()),
        }
    }

    pub fn start(&self, app: &SpringApp) -> Result<()> {
        let jar_path = std::path::Path::new(&app.jar_path);
        let mut cmd = Command::new("java");

        if !app.jvm_opts.is_empty() {
            for opt in app.jvm_opts.split_whitespace() {
                cmd.arg(opt);
            }
        }

        cmd.arg("-jar");
        cmd.arg(jar_path);

        if !app.args.is_empty() {
            for arg in app.args.split_whitespace() {
                cmd.arg(arg);
            }
        }

        // Redirect output to log file
        let log_path = std::path::Path::new(&app.log_path);
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let log_file = std::fs::File::create(log_path)?;
        cmd.stdout(log_file.try_clone()?);
        cmd.stderr(log_file);

        let child = cmd.spawn()?;
        let mut processes = self.processes.lock().unwrap();
        processes.insert(app.id.clone(), child);

        Ok(())
    }

    pub fn stop(&self, app_id: &str) -> Result<()> {
        let mut processes = self.processes.lock().unwrap();
        if let Some(mut child) = processes.remove(app_id) {
            child.kill()?;
            child.wait()?;
        }
        Ok(())
    }

    pub fn is_running(&self, app_id: &str) -> bool {
        let processes = self.processes.lock().unwrap();
        if let Some(child) = processes.get(app_id) {
            if let Ok(None) = child.try_wait() {
                return true;
            }
        }
        false
    }
}