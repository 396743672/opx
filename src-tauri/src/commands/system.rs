use opx_core::models::system::{SystemInfo, HistoryPoint};
use opx_core::services::system_monitor;
use anyhow::Result;

#[tauri::command]
pub fn system_info() -> SystemInfo {
    opx_core::services::system_monitor::info::sample_system()
}

#[tauri::command]
pub fn system_history() -> Result<Vec<HistoryPoint>, String> {
    // 文件缺失/损坏都降级为空曲线，不把错误抛给前端（采样器侧会对损坏文件留 warn 并跳过落盘）
    let h = system_monitor::history::load_metrics(&system_monitor::history::metrics_path())
        .unwrap_or_default();
    Ok(h.system)
}

/// 各进程（pid 字符串键）的持久化样本，供 Dashboard 行内趋势图。
#[tauri::command]
pub fn process_metrics_history() -> Result<std::collections::HashMap<String, Vec<HistoryPoint>>, String> {
    let h = system_monitor::history::load_metrics(&system_monitor::history::metrics_path())
        .unwrap_or_default();
    Ok(h.processes)
}

/// 最近一次应用启动编排报告（无记录返回 null，前端展示空态）
#[tauri::command]
pub fn get_last_startup_report() -> Option<opx_core::models::startup::StartupReport> {
    opx_core::services::startup_bootstrap::last_report()
}

/// 当前应用版本（读 tauri.conf.json 的 version，供「关于」页展示）
#[tauri::command]
pub fn app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

/// 本机网卡地址候选（设置页「对外访问地址」下拉 + 服务配置的 bind 快捷选项）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct LocalAddressView {
    pub address: String,
    pub interface: String,
    pub loopback: bool,
    pub ipv6: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LocalAddressList {
    pub addresses: Vec<LocalAddressView>,
    /// 系统配置里显式选定的地址（空 = 自动）
    pub configured: String,
    /// 实际生效值：显式配置优先，否则本机首选非回环地址
    pub effective: String,
}

/// 列出本机地址，供配置界面让用户在多网卡之间指定「对外访问地址」。
///
/// 多网卡机器（Docker / VPN / 双网卡）光靠文本输入很容易绑错地址，
/// 绑到不存在的地址服务根本起不来，所以候选列表由后端探测、前端只做展示。
#[tauri::command]
pub fn list_local_addresses() -> LocalAddressList {
    use opx_core::services::software_manager::netutils;
    let configured = opx_core::utils::settings::read_settings()
        .map(|s| s.service_bind_address.trim().to_string())
        .unwrap_or_default();
    LocalAddressList {
        addresses: netutils::local_addresses()
            .into_iter()
            .map(|a| LocalAddressView {
                address: a.address,
                interface: a.interface,
                loopback: a.loopback,
                ipv6: a.ipv6,
            })
            .collect(),
        effective: netutils::default_service_bind(),
        configured,
    }
}
