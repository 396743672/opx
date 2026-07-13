use anyhow::Result;

use crate::models::software::{
    InstalledSoftware, JreDependent, JreUsageReport, SoftwareStatus, UninstallBlocker,
    UninstallSafetyReport,
};
use crate::services::software_manager::lifecycle;

/// 检查卸载是否安全
/// - 运行中/启动中/停止中/初始化中 → 阻止
/// - JRE 且是默认或被依赖 → 阻止
pub fn check_uninstall_safety(software: &InstalledSoftware) -> Result<UninstallSafetyReport> {
    let mut blockers: Vec<UninstallBlocker> = Vec::new();

    // B. nginx 站点依赖校验
    if software.key == "nginx" {
        let ws_path = crate::utils::paths::config_dir().join("websites.json");
        if let Ok(content) = std::fs::read_to_string(&ws_path) {
            if let Ok(list) = serde_json::from_str::<serde_json::Value>(&content) {
                let count = list["websites"].as_array().map(|a| a.len()).unwrap_or(0);
                if count > 0 {
                    blockers.push(UninstallBlocker {
                        kind: "nginx_has_sites".to_string(),
                        message_i18n: "uninstallBlockedNginxSites".to_string(),
                        dependents: vec![],
                    });
                }
            }
        }
    }

    // C. 运行中校验
    if is_running_or_transitioning(software) {
        blockers.push(UninstallBlocker {
            kind: "running".to_string(),
            message_i18n: "uninstallBlockedRunning".to_string(),
            dependents: vec![],
        });
    } else if lifecycle::get(&software.id).is_some() {
        // 注册表有记录但状态非运行中 → 也阻止（状态过期或 PID 残留）
        blockers.push(UninstallBlocker {
            kind: "running".to_string(),
            message_i18n: "uninstallBlockedRunning".to_string(),
            dependents: vec![],
        });
    }

    // C. JRE 依赖校验（仅 key=="jre"）
    if software.key == "jre" {
        let jre_report = check_jre_in_use(&software.id)?;
        if jre_report.in_use {
            if jre_report.is_default {
                blockers.push(UninstallBlocker {
                    kind: "jre_default_in_use".to_string(),
                    message_i18n: "uninstallBlockedJreDefault".to_string(),
                    dependents: vec![],
                });
            }
            if !jre_report.dependents.is_empty() {
                blockers.push(UninstallBlocker {
                    kind: "jre_app_dependent".to_string(),
                    message_i18n: "uninstallBlockedJreDependents".to_string(),
                    dependents: jre_report.dependents,
                });
            }
        }
    }

    Ok(UninstallSafetyReport {
        safe: blockers.is_empty(),
        blockers,
    })
}

pub fn is_running_or_transitioning(software: &InstalledSoftware) -> bool {
    matches!(
        software.status,
        SoftwareStatus::Running | SoftwareStatus::Starting | SoftwareStatus::Stopping
    )
    // Initializing 不阻止卸载：初始化失败会卡在 Initializing，
    // 用户需要能卸载重装（Initializing 时无 PID 注册到 lifecycle）
}

/// 检查 JRE 是否被使用
/// - settings.jre_default_id == jre_id → is_default
/// - springboot-manager 运行中应用依赖此 JRE → dependents
pub fn check_jre_in_use(jre_installed_id: &str) -> Result<JreUsageReport> {
    let mut report = JreUsageReport {
        in_use: false,
        is_default: false,
        dependents: vec![],
    };

    // 1. 检查是否为默认 JRE
    if let Some(default_id) = load_jre_default_id()? {
        if default_id == jre_installed_id {
            report.is_default = true;
            report.in_use = true;
        }
    }

    // 2. 查询 springboot-manager（当前未实现，返回空）
    if let Some(apps) = try_load_springboot_apps() {
        for app in apps {
            if app.jre_id == jre_installed_id && app.status == "running" {
                report.dependents.push(JreDependent {
                    kind: "springboot-app".to_string(),
                    id: app.id,
                    name: app.name,
                    status: "running".to_string(),
                });
                report.in_use = true;
            }
        }
    }

    Ok(report)
}

fn load_jre_default_id() -> Result<Option<String>> {
    let sp = crate::utils::paths::settings_path();
    if !sp.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&sp)?;
    let settings: crate::models::settings::AppSettings =
        serde_json::from_str(&content).unwrap_or_default();
    Ok(settings.jre_default_id)
}

/// springboot-manager 应用条目（待该模块实现后填充）
struct SpringbootApp {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    jre_id: String,
    #[allow(dead_code)]
    status: String,
}

fn try_load_springboot_apps() -> Option<Vec<SpringbootApp>> {
    // 当前 springboot-manager 是空文件，返回 None 表示未实现
    None
}
