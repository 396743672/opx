//! HTTP 命令注册表（批次 4.2，设计 §5 路由表「只读命令先行」+ D7）。
//!
//! ## 为什么注册点在这里而不是 opx-http
//!
//! D7 要求分发器**调用与 Tauri IPC 同一个命令函数**（胶水写一份，禁止在
//! opx-http 重写业务逻辑）。tauri `State<'r, T>` 的内部字段私有、无公开
//! 构造器，opx-http（零 tauri 依赖）无法凭空构造；而本壳层持有
//! `AppHandle`，经 `handle.state::<T>()` 拿到的是与 Tauri IPC **同一个**
//! `State` 实例。故：opx-http 只含分发基建（route / 信封 / Registry 类型），
//! 本模块是唯一的命令注册点。
//!
//! headless 壳复用本模块需等设计 C 方案「用例层下沉 core」（命令签名去
//! tauri 化）——设计文档已预留该演进路径，本批不做。
//!
//! ## 本批注册范围（47 个，全只读）
//!
//! `list_` / `get_` 前缀 + 无副作用的 status / 搜索 / 检查类。**不挂**：
//! 写命令（save/set/create/update/delete/install/start/stop/...）、桌面专属
//! （app.rs 3 + system.rs app_version + update.rs 3，设计 §5 桌面专属表）、
//! 路径型导出（F10，download_log / export_* / export_stack / upload_site_bundle）、
//! 带副作用的网络命令（refresh_catalog / fetch_remote_versions_for 写缓存、
//! sync_ddns_now / test_alert_webhook 发通知、generate_self_signed_cert 写证书）、
//! watch/unwatch_log_file（壳层 watch 状态变更）、verify_lock_password
//! （密码校验面不宜直接暴露给 HTTP，暴力破解面从严）。
//!
//! 参数提取用 [`opx_http::api::arg`]（具名参数，与 Tauri invoke args 同形状），
//! 结果包装用 [`opx_http::api::ok`] / [`opx_http::api::val`]。同步命令在
//! async 块内直调（均为文件读/内存采样级开销，与 Tauri 命令线程池同级）。

use std::sync::Arc;

use tauri::Manager;

use opx_http::api::{arg, ok, val, Registry};

use crate::commands::{audit, config, dns_account, lock_screen, node_app, software, springboot, stack, system, website};
use crate::services::dns_account::DnsAccountManager;
use crate::services::node_app_manager::NodeAppManager;
use crate::services::software_manager::SoftwareManager;
use crate::services::springboot_manager::SpringBootManager;
use crate::services::stack_manager::StackManager;
// WebsiteManager 阶段 2.5 已整体搬入 core（壳层未重导出该模块，与 lib.rs 同用 core 路径）
use opx_core::services::website_manager::WebsiteManager;

/// 构建只读命令注册表（批次 4.2：47 个）。
///
/// `handle` 仅用于 `state::<T>()` 取与 Tauri IPC 相同的 State 实例；调用方
/// 须保证六个管理器 State 已 manage（lib.rs setup 顺序保证）。泛型 `R` 使
/// 测试可用 `tauri::test` 的 MockRuntime 构建同一注册表。
pub fn build_registry<R: tauri::Runtime>(handle: &tauri::AppHandle<R>) -> Registry {
    let mut reg = Registry::new();

    // ===== config（2）=====
    reg.register("get_settings", |_ctx, _args| {
        Box::pin(async move { val(config::get_settings()) })
    });
    reg.register("get_autostart", |_ctx, _args| {
        Box::pin(async move { val(config::get_autostart()) })
    });

    // ===== system（4；app_version 为桌面专属不挂）=====
    reg.register("system_info", |_ctx, _args| {
        Box::pin(async move { val(system::system_info()) })
    });
    reg.register("system_history", |_ctx, _args| {
        Box::pin(async move { ok(system::system_history()) })
    });
    reg.register("process_metrics_history", |_ctx, _args| {
        Box::pin(async move { ok(system::process_metrics_history()) })
    });
    reg.register("get_last_startup_report", |_ctx, _args| {
        Box::pin(async move { val(system::get_last_startup_report()) })
    });

    // ===== audit（2；export_audit_entries 写文件不挂）=====
    reg.register("list_audit_entries", |_ctx, args| {
        Box::pin(async move {
            let days: u64 = arg(&args, "days")?;
            let action: Option<String> = arg(&args, "action")?;
            let keyword: Option<String> = arg(&args, "keyword")?;
            let result: Option<String> = arg(&args, "result")?;
            let limit: Option<usize> = arg(&args, "limit")?;
            let offset: Option<usize> = arg(&args, "offset")?;
            val(audit::list_audit_entries(days, action, keyword, result, limit, offset))
        })
    });
    reg.register("audit_stats", |_ctx, args| {
        Box::pin(async move {
            let days: u64 = arg(&args, "days")?;
            val(audit::audit_stats(days))
        })
    });

    // ===== lock_screen（1；verify_lock_password 不挂——密码面从严）=====
    reg.register("has_lock_password", |_ctx, _args| {
        Box::pin(async move { ok(lock_screen::has_lock_password()) })
    });

    // ===== software（19；无 State 的纯读在前）=====
    reg.register("list_snapshots", |_ctx, args| {
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            ok(software::list_snapshots(installed_id).await)
        })
    });
    reg.register("check_uninstall_safety", |_ctx, args| {
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            ok(software::check_uninstall_safety(installed_id).await)
        })
    });
    reg.register("check_jre_in_use", |_ctx, args| {
        Box::pin(async move {
            let jre_installed_id: String = arg(&args, "jre_installed_id")?;
            ok(software::check_jre_in_use(jre_installed_id).await)
        })
    });
    reg.register("get_custom_start_command", |_ctx, args| {
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            ok(software::get_custom_start_command(installed_id).await)
        })
    });
    reg.register("read_config_form", |_ctx, args| {
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            ok(software::read_config_form(installed_id).await)
        })
    });
    reg.register("read_config_source", |_ctx, args| {
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            ok(software::read_config_source(installed_id).await)
        })
    });
    reg.register("list_config_backups", |_ctx, args| {
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            ok(software::list_config_backups(installed_id).await)
        })
    });
    reg.register("get_backup_schedule", |_ctx, args| {
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            ok(software::get_backup_schedule(installed_id).await)
        })
    });
    reg.register("sample_process_resources", |_ctx, args| {
        Box::pin(async move {
            let pids: Vec<u32> = arg(&args, "pids")?;
            ok(software::sample_process_resources(pids))
        })
    });
    let h = handle.clone();
    reg.register("list_available_software", move |_ctx, _args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let st = h.state::<Arc<SoftwareManager>>();
            ok(software::list_available_software(st).await)
        })
    });
    let h = handle.clone();
    reg.register("list_installed_software", move |_ctx, _args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let st = h.state::<Arc<SoftwareManager>>();
            ok(software::list_installed_software(st).await)
        })
    });
    let h = handle.clone();
    reg.register("check_upgrades", move |_ctx, _args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let st = h.state::<Arc<SoftwareManager>>();
            val(software::check_upgrades(st))
        })
    });
    let h = handle.clone();
    reg.register("resolve_software_deps", move |_ctx, args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let st = h.state::<Arc<SoftwareManager>>();
            ok(software::resolve_software_deps(st, installed_id).await)
        })
    });
    let h = handle.clone();
    reg.register("get_software_port_report", move |_ctx, args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let st = h.state::<Arc<SoftwareManager>>();
            ok(software::get_software_port_report(st, installed_id).await)
        })
    });
    let h = handle.clone();
    reg.register("get_software_status", move |_ctx, args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let st = h.state::<Arc<SoftwareManager>>();
            ok(software::get_software_status(st, installed_id).await)
        })
    });
    let h = handle.clone();
    reg.register("get_config_schema", move |_ctx, args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let st = h.state::<Arc<SoftwareManager>>();
            ok(software::get_config_schema(st, installed_id).await)
        })
    });
    let h = handle.clone();
    reg.register("get_log_sources", move |_ctx, args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let st = h.state::<Arc<SoftwareManager>>();
            ok(software::get_log_sources(st, installed_id).await)
        })
    });
    let h = handle.clone();
    reg.register("read_log", move |_ctx, args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let source_index: usize = arg(&args, "source_index")?;
            let archive_index: Option<usize> = arg(&args, "archive_index")?;
            let offset: Option<u64> = arg(&args, "offset")?;
            let before: Option<bool> = arg(&args, "before")?;
            let limit: Option<u64> = arg(&args, "limit")?;
            let keyword: Option<String> = arg(&args, "keyword")?;
            let regex: bool = arg(&args, "regex")?;
            let level: Option<String> = arg(&args, "level")?;
            let st = h.state::<Arc<SoftwareManager>>();
            ok(software::read_log(st, installed_id, source_index, archive_index, offset, before, limit, keyword, regex, level).await)
        })
    });
    let h = handle.clone();
    reg.register("search_all_logs", move |_ctx, args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let keyword: String = arg(&args, "keyword")?;
            let per_source_limit: Option<usize> = arg(&args, "per_source_limit")?;
            let total_limit: Option<usize> = arg(&args, "total_limit")?;
            let st = h.state::<Arc<SoftwareManager>>();
            ok(software::search_all_logs(st, keyword, per_source_limit, total_limit).await)
        })
    });

    // ===== springboot（10）=====
    let h = handle.clone();
    reg.register("list_springboot_apps", move |_ctx, _args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let st = h.state::<Arc<SpringBootManager>>();
            ok(springboot::list_springboot_apps(st).await)
        })
    });
    let h = handle.clone();
    reg.register("list_springboot_groups", move |_ctx, _args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let st = h.state::<Arc<SpringBootManager>>();
            ok(springboot::list_springboot_groups(st).await)
        })
    });
    let h = handle.clone();
    reg.register("get_springboot_global_env_vars", move |_ctx, _args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let st = h.state::<Arc<SpringBootManager>>();
            ok(springboot::get_springboot_global_env_vars(st).await)
        })
    });
    let h = handle.clone();
    reg.register("get_springboot_jvm_metrics", move |_ctx, args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let st = h.state::<Arc<SpringBootManager>>();
            let sw = h.state::<Arc<SoftwareManager>>();
            ok(springboot::get_springboot_jvm_metrics(st, sw, id).await)
        })
    });
    let h = handle.clone();
    reg.register("list_springboot_log_sources", move |_ctx, args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let app_id: String = arg(&args, "app_id")?;
            let st = h.state::<Arc<SpringBootManager>>();
            ok(springboot::list_springboot_log_sources(st, app_id).await)
        })
    });
    let h = handle.clone();
    reg.register("read_springboot_log", move |_ctx, args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let app_id: String = arg(&args, "app_id")?;
            let source_index: usize = arg(&args, "source_index")?;
            let archive_index: Option<usize> = arg(&args, "archive_index")?;
            let offset: Option<u64> = arg(&args, "offset")?;
            let before: Option<bool> = arg(&args, "before")?;
            let limit: Option<u64> = arg(&args, "limit")?;
            let keyword: Option<String> = arg(&args, "keyword")?;
            let regex: Option<bool> = arg(&args, "regex")?;
            let level: Option<String> = arg(&args, "level")?;
            let st = h.state::<Arc<SpringBootManager>>();
            ok(springboot::read_springboot_log(st, app_id, source_index, archive_index, offset, before, limit, keyword, regex, level).await)
        })
    });
    let h = handle.clone();
    reg.register("get_recommended_jvm_opts", move |_ctx, args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let jdk_installed_id: String = arg(&args, "jdk_installed_id")?;
            let sw = h.state::<Arc<SoftwareManager>>();
            ok(springboot::get_recommended_jvm_opts(jdk_installed_id, sw).await)
        })
    });
    let h = handle.clone();
    reg.register("list_springboot_dependency_candidates", move |_ctx, _args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let sw = h.state::<Arc<SoftwareManager>>();
            ok(springboot::list_springboot_dependency_candidates(sw).await)
        })
    });
    reg.register("read_jar_info", |_ctx, args| {
        Box::pin(async move {
            let jar_path: String = arg(&args, "jar_path")?;
            ok(springboot::read_jar_info(jar_path).await)
        })
    });
    reg.register("read_jar_port", |_ctx, args| {
        Box::pin(async move {
            let jar_path: String = arg(&args, "jar_path")?;
            ok(springboot::read_jar_port(jar_path).await)
        })
    });

    // ===== node_app（2）=====
    let h = handle.clone();
    reg.register("list_node_apps", move |_ctx, _args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let st = h.state::<Arc<NodeAppManager>>();
            ok(node_app::list_node_apps(st).await)
        })
    });
    let h = handle.clone();
    reg.register("read_node_app_log", move |_ctx, args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let st = h.state::<Arc<NodeAppManager>>();
            ok(node_app::read_node_app_log(st, id).await)
        })
    });

    // ===== stack（2；export/import/start/stop/restart 不挂）=====
    let h = handle.clone();
    reg.register("list_stacks", move |_ctx, _args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let st = h.state::<Arc<StackManager>>();
            ok(stack::list_stacks(st).await)
        })
    });
    let h = handle.clone();
    reg.register("get_stack", move |_ctx, args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let st = h.state::<Arc<StackManager>>();
            ok(stack::get_stack(st, id).await)
        })
    });

    // ===== website（3；写操作 / 证书 / conf 写入不挂）=====
    let h = handle.clone();
    reg.register("list_websites", move |_ctx, _args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let wm = h.state::<Arc<WebsiteManager>>();
            val(website::list_websites(wm))
        })
    });
    let h = handle.clone();
    reg.register("list_account_refs", move |_ctx, _args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let wm = h.state::<Arc<WebsiteManager>>();
            val(website::list_account_refs(wm))
        })
    });
    let h = handle.clone();
    reg.register("get_site_conf", move |_ctx, args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let sm = h.state::<Arc<SoftwareManager>>();
            let wm = h.state::<Arc<WebsiteManager>>();
            ok(website::get_site_conf(sm, wm, id))
        })
    });

    // ===== dns_account（2；test 为无副作用探测，save/delete 不挂）=====
    let h = handle.clone();
    reg.register("list_dns_accounts", move |_ctx, _args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let m = h.state::<Arc<DnsAccountManager>>();
            val(dns_account::list_dns_accounts(m))
        })
    });
    let h = handle.clone();
    reg.register("test_dns_account", move |_ctx, args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let m = h.state::<Arc<DnsAccountManager>>();
            ok(dns_account::test_dns_account(m, id).await)
        })
    });

    reg
}
