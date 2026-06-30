pub mod settings;
pub mod software;
pub mod springboot;
pub mod system;

#[cfg(test)]
mod tests {
    use super::settings::{AppSettings, CloseWindowAction};
    use super::software::{
        InstallParams, InstalledSoftware, InstalledSoftwareList, SoftwareMeta, SoftwareStatus,
    };
    use super::springboot::{AppGroup, AppStatus, JvmInfo, SpringApp, SpringAppList};
    use super::system::{DiskInfo, HistoryPoint, NetworkInfo, SystemInfo};
    use chrono::NaiveDate;
    use serde::{de::DeserializeOwned, Serialize};

    fn assert_json_roundtrip<T>(value: &T) -> T
    where
        T: Serialize + DeserializeOwned,
    {
        let json = serde_json::to_string(value).expect("model should serialize to JSON");
        serde_json::from_str(&json).expect("model should deserialize from JSON")
    }

    #[test]
    fn app_settings_default_matches_specification() {
        let settings = AppSettings::default();

        assert_eq!(settings.theme, "auto");
        assert_eq!(settings.language, "zh-CN");
        assert!(!settings.sidebar_collapsed);
        assert_eq!(settings.software_root, "apps");
        assert_eq!(settings.config_root, "config");
        assert_eq!(settings.mirror_url, "https://mirrors.aliyun.com");
        assert!(settings.auto_check_update);
        assert!(matches!(
            settings.close_window_action,
            CloseWindowAction::CloseToTray
        ));
        assert!(!settings.register_as_system_service);
        assert!(settings.auto_start_managed_services);
    }

    #[test]
    fn list_and_status_defaults_match_specification() {
        let software_list = InstalledSoftwareList::default();
        let spring_list = SpringAppList::default();

        assert!(software_list.software.is_empty());
        assert!(spring_list.applications.is_empty());
        assert!(spring_list.groups.is_empty());
        assert_eq!(AppStatus::default(), AppStatus::Stopped);
    }

    #[test]
    fn system_models_support_json_roundtrip() {
        let system = SystemInfo {
            cpu_usage: 12.5,
            memory_used: 1024,
            memory_total: 4096,
            memory_usage: 25.0,
            disks: vec![DiskInfo {
                mount_point: "/".to_string(),
                total: 100,
                used: 40,
                usage: 40.0,
            }],
            network: NetworkInfo {
                bytes_sent: 10,
                bytes_recv: 20,
                packets_sent: 1,
                packets_recv: 2,
            },
            os_name: "Windows".to_string(),
            os_version: "11".to_string(),
            hostname: "opx".to_string(),
            boot_time: 123456,
        };
        let history = HistoryPoint {
            timestamp: 123456,
            cpu_usage: 12.5,
            memory_usage: 25.0,
        };

        let system_roundtrip = assert_json_roundtrip(&system);
        let history_roundtrip = assert_json_roundtrip(&history);

        assert_eq!(system_roundtrip.disks[0].mount_point, "/");
        assert_eq!(system_roundtrip.network.bytes_recv, 20);
        assert_eq!(history_roundtrip.timestamp, 123456);
    }

    #[test]
    fn software_models_support_json_roundtrip() {
        let installed_at = NaiveDate::from_ymd_opt(2026, 6, 26)
            .unwrap()
            .and_hms_opt(9, 30, 0)
            .unwrap();
        let meta = SoftwareMeta {
            key: "mysql".to_string(),
            name: "MySQL".to_string(),
            description: "Database".to_string(),
            available_versions: vec!["8.0".to_string()],
            default_version: "8.0".to_string(),
        };
        let software = InstalledSoftware {
            id: "mysql-1".to_string(),
            key: "mysql".to_string(),
            name: "MySQL".to_string(),
            version: "8.0".to_string(),
            install_path: "apps/mysql".to_string(),
            install_time: installed_at,
            status: SoftwareStatus::Running,
            port: 3306,
            config: serde_json::json!({ "charset": "utf8mb4" }),
            is_custom: false,
            auto_start_on_app_start: true,
            startup_order: 10,
        };
        let params = InstallParams {
            key: "mysql".to_string(),
            version: "8.0".to_string(),
            install_path: "apps/mysql".to_string(),
        };

        let meta_roundtrip = assert_json_roundtrip(&meta);
        let software_roundtrip = assert_json_roundtrip(&software);
        let params_roundtrip = assert_json_roundtrip(&params);

        assert_eq!(meta_roundtrip.default_version, "8.0");
        assert_eq!(software_roundtrip.status, SoftwareStatus::Running);
        assert_eq!(software_roundtrip.startup_order, 10);
        assert_eq!(params_roundtrip.install_path, "apps/mysql");
    }

    #[test]
    fn springboot_models_support_json_roundtrip() {
        let started_at = NaiveDate::from_ymd_opt(2026, 6, 26)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        let app = SpringApp {
            id: "app-1".to_string(),
            name: "demo".to_string(),
            jar_path: "apps/demo/demo.jar".to_string(),
            version: "1.0.0".to_string(),
            env: "prod".to_string(),
            port: 8080,
            jvm_opts: "-Xmx512m".to_string(),
            args: "--spring.profiles.active=prod".to_string(),
            status: AppStatus::Starting,
            log_path: "logs/demo.log".to_string(),
            start_time: Some(started_at),
            backup_enabled: true,
            auto_restart: true,
            group: Some("core".to_string()),
            auto_start_on_app_start: true,
            startup_order: 20,
        };
        let group = AppGroup {
            id: "core".to_string(),
            name: "Core Services".to_string(),
            order: 1,
            depends_on: vec!["infra".to_string()],
        };
        let jvm = JvmInfo {
            heap_used: 100,
            heap_max: 512,
            non_heap_used: 50,
            thread_count: 24,
            gc_count: 3,
            gc_time: 150,
        };

        let app_roundtrip = assert_json_roundtrip(&app);
        let group_roundtrip = assert_json_roundtrip(&group);
        let jvm_roundtrip = assert_json_roundtrip(&jvm);

        assert_eq!(app_roundtrip.status, AppStatus::Starting);
        assert_eq!(app_roundtrip.group.as_deref(), Some("core"));
        assert_eq!(group_roundtrip.depends_on, vec!["infra".to_string()]);
        assert_eq!(jvm_roundtrip.thread_count, 24);
    }
}
