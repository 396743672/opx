use anyhow::{Result, Context};
use std::path::PathBuf;

pub fn register_current() -> Result<()> {
    use windows_service::{
        service::{ServiceConfig, ServiceType, ServiceAccess},
        manager::{ServiceManager, Service},
    };

    let manager = ServiceManager::local()
        .context("Failed to connect to service manager")?;

    let executable_path = std::env::current_exe()
        .context("Failed to get current executable path")?;

    let service_name = "OPX";
    let display_name = "OPX Operations Management Service";

    let config = ServiceConfig {
        service_type: ServiceType::OwnProcess,
        executable_path,
        display_name: Some(display_name),
        description: Some("Lightweight cross-platform operations management tool by OPX"),
    };

    // Check if service already exists
    if let Ok(service) = manager.open_service(service_name, ServiceAccess::All) {
        // Delete existing service before recreating
        service.delete()
            .context("Failed to delete existing service")?;
    }

    manager.create_service(config, service_name)
        .context("Failed to create service")?;

    println!("Service registered successfully");
    Ok(())
}

pub fn unregister_current() -> Result<()> {
    use windows_service::{
        manager::{ServiceManager, Service},
        service::ServiceState,
    };

    let manager = ServiceManager::local()
        .context("Failed to connect to service manager")?;

    let service_name = "OPX";

    if let Ok(service) = manager.open_service(service_name, ServiceAccess::All) {
        // Stop if running
        let status = service.query()?;
        if status.current_state != ServiceState::Stopped {
            service.stop()?;
        }
        service.delete()
            .context("Failed to delete service")?;
        println!("Service unregistered successfully");
    }

    Ok(())
}