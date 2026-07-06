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

    // A. 运行中校验
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::software::InstallSource;
    use chrono::NaiveDateTime;

    fn make_software(key: &str, status: SoftwareStatus, id: &str) -> InstalledSoftware {
        InstalledSoftware {
            id: id.to_string(),
            key: key.to_string(),
            version: "1.0".to_string(),
            name: "Test".to_string(),
            install_path: "apps/test".to_string(),
            install_time: NaiveDateTime::from_timestamp_opt(1700000000, 0).unwrap(),
            status,
            port: 0,
            config: serde_json::json!({}),
            is_custom: false,
            auto_start_on_app_start: false,
            startup_order: 0,
            source: InstallSource::Mirror {
                mirror_name: "t".to_string(),
                url: "http://t".to_string(),
            },
            pid: None,
            last_started_at: None,
            last_stopped_at: None,
            last_error: None,
            custom_start_command: None,
        }
    }

    #[test]
    fn running_software_is_unsafe() {
        let sw = make_software("mysql", SoftwareStatus::Running, "uuid-1");
        let report = check_uninstall_safety(&sw).unwrap();
        assert!(!report.safe);
        assert_eq!(report.blockers[0].kind, "running");
    }

    #[test]
    fn starting_software_is_unsafe() {
        let sw = make_software("mysql", SoftwareStatus::Starting, "uuid-2");
        let report = check_uninstall_safety(&sw).unwrap();
        assert!(!report.safe);
    }

    #[test]
    fn stopping_software_is_unsafe() {
        let sw = make_software("mysql", SoftwareStatus::Stopping, "uuid-stop");
        let report = check_uninstall_safety(&sw).unwrap();
        assert!(!report.safe);
    }

    #[test]
    fn initializing_software_is_safe_to_uninstall() {
        // Initializing 状态初始化失败会卡住，需要能卸载重装
        let sw = make_software("mysql", SoftwareStatus::Initializing, "uuid-init");
        let report = check_uninstall_safety(&sw).unwrap();
        assert!(report.safe);
    }

    #[test]
    fn stopped_software_is_safe() {
        let sw = make_software("mysql", SoftwareStatus::Stopped, "uuid-3");
        let report = check_uninstall_safety(&sw).unwrap();
        assert!(report.safe);
        assert!(report.blockers.is_empty());
    }

    #[test]
    fn error_software_is_safe() {
        let sw = make_software("mysql", SoftwareStatus::Error, "uuid-4");
        let report = check_uninstall_safety(&sw).unwrap();
        assert!(report.safe);
    }

    #[test]
    fn unknown_software_is_safe() {
        let sw = make_software("mysql", SoftwareStatus::Unknown, "uuid-5");
        let report = check_uninstall_safety(&sw).unwrap();
        assert!(report.safe);
    }

    #[test]
    fn is_running_or_transitioning_covers_all_active_states() {
        assert!(is_running_or_transitioning(&make_software(
            "x",
            SoftwareStatus::Running,
            "1"
        )));
        assert!(is_running_or_transitioning(&make_software(
            "x",
            SoftwareStatus::Starting,
            "2"
        )));
        assert!(is_running_or_transitioning(&make_software(
            "x",
            SoftwareStatus::Stopping,
            "3"
        )));
        // Initializing 不再被视为运行中（允许初始化失败时卸载重装）
        assert!(!is_running_or_transitioning(&make_software(
            "x",
            SoftwareStatus::Initializing,
            "4"
        )));
        assert!(!is_running_or_transitioning(&make_software(
            "x",
            SoftwareStatus::Stopped,
            "5"
        )));
        assert!(!is_running_or_transitioning(&make_software(
            "x",
            SoftwareStatus::Error,
            "6"
        )));
        assert!(!is_running_or_transitioning(&make_software(
            "x",
            SoftwareStatus::Unknown,
            "7"
        )));
    }

    #[test]
    fn jre_check_returns_not_in_use_when_settings_missing() {
        // settings.json 不存在时，is_default=false，in_use=false
        // 注意：此测试依赖 paths::settings_path()，可能在真实 config 目录
        // 但因 settings.json 通常存在或不存在都返回 None，不影响
        let report = check_jre_in_use("nonexistent-jre-id").unwrap();
        // 不一定是 false（若 settings.json 存在且 jre_default_id 匹配），
        // 但对于 "nonexistent-jre-id" 几乎不可能匹配
        assert!(!report.is_default, "未知 JRE 不应是默认");
    }

    #[test]
    fn check_uninstall_safety_for_jre_does_not_panic() {
        // JRE 卸载校验应能正常执行（即使 springboot-manager 未实现）
        let sw = make_software("jre", SoftwareStatus::Stopped, "jre-uuid-1");
        let report = check_uninstall_safety(&sw).unwrap();
        // safe 取决于 settings.json，但不应 panic
        let _ = report.safe;
    }

    #[test]
    fn check_uninstall_safety_for_custom_software_skips_jre_check() {
        // 自定义软件不应走 JRE 依赖检查
        let mut sw = make_software("custom-app", SoftwareStatus::Stopped, "custom-1");
        sw.is_custom = true;
        let report = check_uninstall_safety(&sw).unwrap();
        assert!(report.safe);
        // blockers 中不应有 jre_default_in_use 或 jre_app_dependent
        for b in &report.blockers {
            assert!(
                !b.kind.starts_with("jre_"),
                "自定义软件不应有 JRE 阻止原因，实际：{}",
                b.kind
            );
        }
    }
}
