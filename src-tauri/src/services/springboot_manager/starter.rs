use crate::models::springboot::{SpringApp, AppGroup};
use anyhow::{Result, anyhow};
use std::collections::{HashMap, BTreeMap};
use std::sync::atomic::{AtomicBool, Ordering};

// 按 startupOrder 分组启动，同顺序并行启动
pub async fn start_ordered(apps: &[SpringApp], on_progress: impl Fn(String) -> ()) -> Result<()> {
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
            let handle = tokio::spawn(async move {
                // Actually start
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                // caller will do the actual start
                Result::<(), _>::Ok(())
            });
            handles.push(handle);
        }

        // Wait for all in this layer
        for handle in handles {
            if let Err(e) = handle.await {
                failed.store(true, Ordering::SeqCst);
                eprintln!("Join error: {}", e);
            }
        }

        if failed.load(Ordering::SeqCst) {
            return Err(anyhow!("Failed to start some apps in layer {}", order));
        }
    }

    Ok(())
}