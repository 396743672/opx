use anyhow::Result;
use crate::services::service_registry;

#[tauri::command]
pub fn toggle_opx_system_service(enable: bool) -> Result<(), String> {
    if enable {
        service_registry::register_current()
            .map_err(|e| e.to_string())
    } else {
        service_registry::unregister_current()
            .map_err(|e| e.to_string())
    }
}