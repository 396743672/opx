use crate::models::springboot::{SpringApp, AppGroup};
use crate::services::springboot_manager::types::ProcessManager;
use anyhow::{Result, anyhow};
use std::collections::{HashMap, BTreeMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// 按 startupOrder 分组启动，同顺序并行启动
pub async fn start_ordered(apps: &[SpringApp], process_manager: &ProcessManager, on_progress: impl Fn(String) -> ()) -> Result<()> {
    // Group by startup_order
    let mut ordered: BTreeMap<u32, Vec<&SpringApp>> = BTreeMap::new();

    for app in apps {
        if app.auto_start_on_app_start {
            ordered.entry(app.startup_order).or_default().push(app);
        }
    }

    for (order, group) in ordered {
        let msg = format!("Starting layer {}, {} apps", order, group.len());
        on_progress(msg);

        // Parallel start all apps in this order layer
        let mut handles = Vec::new();
        let failed = AtomicBool::new(false);

        for app in group {
            let app_clone = app.clone();
            let process_manager_ref = process_manager.clone();

            let handle = tokio::spawn(async move {
                if let Err(e) = process_manager_ref.start(&app_clone) {
                    eprintln!("Failed to start {}: {}", app_clone.name, e);
                    return Err(e);
                }
                Ok(())
            });
            handles.push(handle);
        }

        // Wait for all in this layer
        for handle in handles {
            match handle.await {
                Ok(Ok(_)) => {},
                Ok(Err(e)) => {
                    failed.store(true, Ordering::SeqCst);
                    eprintln!("Start error: {}", e);
                },
                Err(e) => {
                    failed.store(true, Ordering::SeqCst);
                    eprintln!("Join error: {}", e);
                }
            }
        }

        if failed.load(Ordering::SeqCst) {
            return Err(anyhow!("Failed to start some apps in layer {}", order));
        }
    }

    Ok(())
}