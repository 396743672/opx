use crate::models::settings::AppSettings;
use crate::utils::file;
use anyhow::Result;
use std::path::{Path, PathBuf};

#[tauri::command]
pub fn get_settings() -> Result<AppSettings, String> {
    let app_root = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot get app directory".to_string())?
        .to_path_buf();

    let settings_path = app_root.join("config").join("settings.json");

    if !settings_path.exists() {
        return Ok(AppSettings::default());
    }

    let settings = file::read_json::<AppSettings>(&settings_path)
        .map_err(|e| e.to_string())?;

    Ok(settings)
}

#[tauri::command]
pub fn save_settings(settings: AppSettings) -> Result<(), String> {
    let app_root = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot get app directory".to_string())?
        .to_path_buf();

    let settings_path = app_root.join("config").join("settings.json");
    file::write_json(&settings_path, &settings)
        .map_err(|e| e.to_string())?;

    Ok(())
}