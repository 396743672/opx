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
//! ## 注册范围（批次 4.3 起全量接入）
//!
//! - **只读命令（4.2，47 个）**：`list_` / `get_` 前缀 + 无副作用的 status /
//!   搜索 / 检查类；4.3 补挂前端在调但 4.2 漏挂的 `list_custom_templates`。
//! - **写命令（4.3，59 个）**：前端会调 + 无桌面专属依赖 + 非路径型导出的
//!   全部变更类命令（安装/启停/配置写入/快照/证书/DDNS/告警测试等）。其中
//!   **带副作用的网络命令 4 个**（refresh_catalog / fetch_remote_versions_for
//!   写缓存、sync_ddns_now / test_alert_webhook 发外部请求）单独成组列出，
//!   语义上仍属「前端会调」故本批挂上。
//! - **桌面专属（4.3，7 个）**：注册但 handler 直接返回 `desktop_only`
//!   信封（409）——命令可识别但 HTTP 入口无宿主能力，前端按 code 渲染
//!   「请到桌面端操作」提示。
//!
//! **明确不挂**（排除清单）：
//! 路径型导出（F10，download_log / export_combined_log / download_springboot_log /
//! export_springboot_config / export_stack / export_audit_entries /
//! upload_site_bundle——返回/接收本机文件路径，HTTP 入口无意义）、
//! watch/unwatch_log_file（壳层 watch 状态变更）、verify_lock_password
//! （密码校验面不宜直接暴露给 HTTP，暴力破解面从严）。generate_self_signed_cert /
//! issue_site_certificate 虽落盘证书但属前端会调的业务流程，挂上。
//!
//! 参数提取用 [`opx_http::api::arg`]（具名参数，与 Tauri invoke args 同形状），
//! 结果包装用 [`opx_http::api::ok`] / [`opx_http::api::val`]。同步命令在
//! async 块内直调（均为文件读/内存采样级开销，与 Tauri 命令线程池同级）。

use std::sync::Arc;

use tauri::Manager;

use opx_http::api::{arg, ok, val, Registry};
use opx_http::AppContext;
// ApiError 自 opx-http 顶层 re-export（桌面专属命令的 409 信封用）
use opx_http::ApiError;

use crate::commands::{audit, config, dns_account, lock_screen, node_app, software, springboot, stack, system, website};
use crate::services::dns_account::DnsAccountManager;
use crate::services::node_app_manager::NodeAppManager;
use crate::services::software_manager::SoftwareManager;
use crate::services::springboot_manager::SpringBootManager;
use crate::services::stack_manager::StackManager;
// WebsiteManager 阶段 2.5 已整体搬入 core（壳层未重导出该模块，与 lib.rs 同用 core 路径）
use opx_core::services::website_manager::WebsiteManager;

/// 构建全量命令注册表（批次 4.2 只读 47 + 4.3 补挂 list_custom_templates /
/// 写命令 59 / 桌面专属 7）。
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
    // 4.3 补挂：前端在调（自定义启动命令模板选择），无 State 纯读，4.2 漏挂
    reg.register("list_custom_templates", |_ctx, _args| {
        Box::pin(async move { ok(software::list_custom_templates().await) })
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

    // =====================================================================
    // 批次 4.3：写命令 + 桌面专属（语义化 409）
    // =====================================================================
    // 与 4.2 只读段同一模式：`h.state::<T>()` 取与 Tauri IPC 同一个 State 实例
    // 调用**同一个命令函数**；`Fn` 闭包体内先 `let h = h.clone();`（future 须
    // 'static）。参数名与 Tauri invoke args 同形状（arg 提取，缺失/类型不符 →
    // 400 invalid_args）；`Result<_, String>` 统一经 ok 包装（Err → 500
    // command_failed，message 原样透传走前端 translateError）。

    // ===== config 写命令（4；含带副作用网络命令 test_alert_webhook /
    //       sync_ddns_now——发外部 webhook / DDNS 请求，前端会调故挂上）=====
    reg.register("save_settings", |_ctx, args| {
        Box::pin(async move {
            let settings = arg(&args, "settings")?;
            ok(config::save_settings(settings))
        })
    });
    reg.register("set_autostart", |_ctx, args| {
        Box::pin(async move {
            let enabled: bool = arg(&args, "enabled")?;
            ok(config::set_autostart(enabled))
        })
    });
    reg.register("test_alert_webhook", |_ctx, _args| {
        Box::pin(async move { ok(config::test_alert_webhook().await) })
    });
    reg.register("sync_ddns_now", |_ctx, _args| {
        Box::pin(async move { ok(config::sync_ddns_now().await) })
    });

    // ===== lock_screen 写命令（2；verify_lock_password 维持排除——密码
    //       校验面从严，HTTP 入口不做暴力破解面扩张）=====
    reg.register("set_lock_password", |_ctx, args| {
        Box::pin(async move {
            let pw: String = arg(&args, "pw")?;
            ok(lock_screen::set_lock_password(pw))
        })
    });
    reg.register("clear_lock_password", |_ctx, _args| {
        Box::pin(async move { ok(lock_screen::clear_lock_password()) })
    });

    // ===== software 写命令（21；含带副作用网络命令 refresh_catalog /
    //       fetch_remote_versions_for——写软件源缓存，前端会调故挂上）=====
    let h = handle.clone();
    reg.register("refresh_catalog", move |_ctx, _args| {
        let h = h.clone(); // Fn 闭包按调用克隆（future 须 'static）
        Box::pin(async move {
            let m = h.state::<Arc<SoftwareManager>>();
            let c = h.state::<AppContext>();
            ok(software::refresh_catalog(m, c).await)
        })
    });
    let h = handle.clone();
    reg.register("fetch_remote_versions_for", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let key: String = arg(&args, "key")?;
            let m = h.state::<Arc<SoftwareManager>>();
            ok(software::fetch_remote_versions_for(m, key).await)
        })
    });
    let h = handle.clone();
    reg.register("install_software", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let params = arg(&args, "params")?;
            let m = h.state::<Arc<SoftwareManager>>();
            let c = h.state::<AppContext>();
            ok(software::install_software(m, c, params).await)
        })
    });
    let h = handle.clone();
    reg.register("upgrade_software", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let m = h.state::<Arc<SoftwareManager>>();
            let c = h.state::<AppContext>();
            ok(software::upgrade_software(m, c, installed_id).await)
        })
    });
    let h = handle.clone();
    reg.register("install_custom", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let params = arg(&args, "params")?;
            let m = h.state::<Arc<SoftwareManager>>();
            let c = h.state::<AppContext>();
            ok(software::install_custom(m, c, params).await)
        })
    });
    let h = handle.clone();
    reg.register("uninstall_software", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let m = h.state::<Arc<SoftwareManager>>();
            let c = h.state::<AppContext>();
            ok(software::uninstall_software(m, c, installed_id).await)
        })
    });
    let h = handle.clone();
    reg.register("start_software", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let init_password: Option<String> = arg(&args, "init_password")?;
            let m = h.state::<Arc<SoftwareManager>>();
            let c = h.state::<AppContext>();
            ok(software::start_software(m, c, installed_id, init_password).await)
        })
    });
    let h = handle.clone();
    reg.register("stop_software", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let m = h.state::<Arc<SoftwareManager>>();
            let c = h.state::<AppContext>();
            ok(software::stop_software(m, c, installed_id).await)
        })
    });
    let h = handle.clone();
    reg.register("restart_software", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let init_password: Option<String> = arg(&args, "init_password")?;
            let m = h.state::<Arc<SoftwareManager>>();
            let c = h.state::<AppContext>();
            ok(software::restart_software(m, c, installed_id, init_password).await)
        })
    });
    let h = handle.clone();
    reg.register("update_software_deps", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let depends_on: Vec<String> = arg(&args, "depends_on")?;
            let m = h.state::<Arc<SoftwareManager>>();
            ok(software::update_software_deps(m, installed_id, depends_on).await)
        })
    });
    let h = handle.clone();
    reg.register("write_config_form", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let data = arg(&args, "data")?;
            let m = h.state::<Arc<SoftwareManager>>();
            ok(software::write_config_form(m, installed_id, data).await)
        })
    });
    let h = handle.clone();
    reg.register("write_config_source", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let content: String = arg(&args, "content")?;
            let m = h.state::<Arc<SoftwareManager>>();
            ok(software::write_config_source(m, installed_id, content).await)
        })
    });
    let h = handle.clone();
    reg.register("save_custom_start_command", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let cmd = arg(&args, "cmd")?;
            let m = h.state::<Arc<SoftwareManager>>();
            ok(software::save_custom_start_command(m, installed_id, cmd).await)
        })
    });
    let h = handle.clone();
    reg.register("save_startup_settings", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let auto_start: bool = arg(&args, "auto_start")?;
            let order: u32 = arg(&args, "order")?;
            let auto_restart: bool = arg(&args, "auto_restart")?;
            let m = h.state::<Arc<SoftwareManager>>();
            ok(software::save_startup_settings(m, installed_id, auto_start, order, auto_restart).await)
        })
    });
    let h = handle.clone();
    reg.register("restore_config_backup", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let backup_name: String = arg(&args, "backup_name")?;
            let m = h.state::<Arc<SoftwareManager>>();
            ok(software::restore_config_backup(m, installed_id, backup_name).await)
        })
    });
    let h = handle.clone();
    reg.register("create_snapshot", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let mode = arg(&args, "mode")?;
            let name: Option<String> = arg(&args, "name")?;
            let note: Option<String> = arg(&args, "note")?;
            let m = h.state::<Arc<SoftwareManager>>();
            let c = h.state::<AppContext>();
            ok(software::create_snapshot(m, c, installed_id, mode, name, note).await)
        })
    });
    let h = handle.clone();
    reg.register("restore_snapshot", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let snapshot_id: String = arg(&args, "snapshot_id")?;
            let force: bool = arg(&args, "force")?;
            let m = h.state::<Arc<SoftwareManager>>();
            ok(software::restore_snapshot(m, installed_id, snapshot_id, force).await)
        })
    });
    // 无 State：直调（与 4.2 无 State 只读命令同模式）
    reg.register("delete_snapshot", |_ctx, args| {
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let snapshot_id: String = arg(&args, "snapshot_id")?;
            ok(software::delete_snapshot(installed_id, snapshot_id).await)
        })
    });
    reg.register("set_backup_schedule", |_ctx, args| {
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let minutes: u64 = arg(&args, "minutes")?;
            ok(software::set_backup_schedule(installed_id, minutes).await)
        })
    });
    let h = handle.clone();
    reg.register("reset_instance", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let m = h.state::<Arc<SoftwareManager>>();
            ok(software::reset_instance(m, installed_id).await)
        })
    });
    let h = handle.clone();
    reg.register("rollback_software", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let installed_id: String = arg(&args, "installed_id")?;
            let m = h.state::<Arc<SoftwareManager>>();
            ok(software::rollback_software(m, installed_id).await)
        })
    });

    // ===== springboot 写命令（11；import_springboot_config 接收本机文件
    //       路径，属前端流程内文件选择结果，与路径型「导出」不同挂上）=====
    let h = handle.clone();
    reg.register("create_springboot_app", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let params = arg(&args, "params")?;
            let m = h.state::<Arc<SpringBootManager>>();
            ok(springboot::create_springboot_app(m, params).await)
        })
    });
    let h = handle.clone();
    reg.register("update_springboot_app", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let params = arg(&args, "params")?;
            let m = h.state::<Arc<SpringBootManager>>();
            ok(springboot::update_springboot_app(m, id, params).await)
        })
    });
    let h = handle.clone();
    reg.register("delete_springboot_app", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let m = h.state::<Arc<SpringBootManager>>();
            ok(springboot::delete_springboot_app(m, id).await)
        })
    });
    let h = handle.clone();
    reg.register("start_springboot_app", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let m = h.state::<Arc<SpringBootManager>>();
            let sw = h.state::<Arc<SoftwareManager>>();
            let c = h.state::<AppContext>();
            ok(springboot::start_springboot_app(m, sw, c, id).await)
        })
    });
    let h = handle.clone();
    reg.register("stop_springboot_app", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let m = h.state::<Arc<SpringBootManager>>();
            let sw = h.state::<Arc<SoftwareManager>>();
            let c = h.state::<AppContext>();
            ok(springboot::stop_springboot_app(m, sw, c, id).await)
        })
    });
    let h = handle.clone();
    reg.register("restart_springboot_app", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let m = h.state::<Arc<SpringBootManager>>();
            let sw = h.state::<Arc<SoftwareManager>>();
            let c = h.state::<AppContext>();
            ok(springboot::restart_springboot_app(m, sw, c, id).await)
        })
    });
    let h = handle.clone();
    reg.register("replace_springboot_jar", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let new_jar_path: String = arg(&args, "new_jar_path")?;
            let m = h.state::<Arc<SpringBootManager>>();
            ok(springboot::replace_springboot_jar(m, id, new_jar_path).await)
        })
    });
    let h = handle.clone();
    reg.register("replace_springboot_jar_and_restart", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let new_jar_path: String = arg(&args, "new_jar_path")?;
            let m = h.state::<Arc<SpringBootManager>>();
            let sw = h.state::<Arc<SoftwareManager>>();
            let c = h.state::<AppContext>();
            ok(springboot::replace_springboot_jar_and_restart(m, sw, c, id, new_jar_path).await)
        })
    });
    let h = handle.clone();
    reg.register("save_springboot_groups", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let groups = arg(&args, "groups")?;
            let m = h.state::<Arc<SpringBootManager>>();
            ok(springboot::save_springboot_groups(m, groups).await)
        })
    });
    let h = handle.clone();
    reg.register("set_springboot_global_env_vars", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let env_vars: Vec<(String, String)> = arg(&args, "env_vars")?;
            let m = h.state::<Arc<SpringBootManager>>();
            ok(springboot::set_springboot_global_env_vars(m, env_vars).await)
        })
    });
    let h = handle.clone();
    reg.register("import_springboot_config", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let file_path: String = arg(&args, "file_path")?;
            let c = h.state::<AppContext>();
            let m = h.state::<Arc<SpringBootManager>>();
            ok(springboot::import_springboot_config(c, m, file_path).await)
        })
    });

    // ===== stack 写命令（7；import_stack 接收前端选定的本机 JSON 路径，
    //       同 import_springboot_config 理由挂上）=====
    let h = handle.clone();
    reg.register("create_stack", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let payload = arg(&args, "payload")?;
            let m = h.state::<Arc<StackManager>>();
            ok(stack::create_stack(m, payload).await)
        })
    });
    let h = handle.clone();
    reg.register("update_stack", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let payload = arg(&args, "payload")?;
            let m = h.state::<Arc<StackManager>>();
            ok(stack::update_stack(m, id, payload).await)
        })
    });
    let h = handle.clone();
    reg.register("delete_stack", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let m = h.state::<Arc<StackManager>>();
            ok(stack::delete_stack(m, id).await)
        })
    });
    let h = handle.clone();
    reg.register("start_stack", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let m = h.state::<Arc<StackManager>>();
            let c = h.state::<AppContext>();
            ok(stack::start_stack(m, c, id).await)
        })
    });
    let h = handle.clone();
    reg.register("stop_stack", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let m = h.state::<Arc<StackManager>>();
            let c = h.state::<AppContext>();
            ok(stack::stop_stack(m, c, id).await)
        })
    });
    let h = handle.clone();
    reg.register("restart_stack", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let m = h.state::<Arc<StackManager>>();
            let c = h.state::<AppContext>();
            ok(stack::restart_stack(m, c, id).await)
        })
    });
    let h = handle.clone();
    reg.register("import_stack", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let path: String = arg(&args, "path")?;
            let m = h.state::<Arc<StackManager>>();
            ok(stack::import_stack(m, path).await)
        })
    });

    // ===== website 写命令（7；generate_self_signed_cert / issue_site_certificate
    //       落盘证书但属前端会调的业务流程；upload_site_bundle 路径型不挂）=====
    let h = handle.clone();
    reg.register("save_website", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let site = arg(&args, "site")?;
            let sm = h.state::<Arc<SoftwareManager>>();
            let wm = h.state::<Arc<WebsiteManager>>();
            ok(website::save_website(sm, wm, site))
        })
    });
    let h = handle.clone();
    reg.register("delete_website", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let sm = h.state::<Arc<SoftwareManager>>();
            let wm = h.state::<Arc<WebsiteManager>>();
            ok(website::delete_website(sm, wm, id))
        })
    });
    let h = handle.clone();
    reg.register("set_website_enabled", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let enabled: bool = arg(&args, "enabled")?;
            let sm = h.state::<Arc<SoftwareManager>>();
            let wm = h.state::<Arc<WebsiteManager>>();
            ok(website::set_website_enabled(sm, wm, id, enabled))
        })
    });
    let h = handle.clone();
    reg.register("set_site_conf", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let content: String = arg(&args, "content")?;
            let sm = h.state::<Arc<SoftwareManager>>();
            let wm = h.state::<Arc<WebsiteManager>>();
            ok(website::set_site_conf(sm, wm, id, content))
        })
    });
    let h = handle.clone();
    reg.register("unlock_site_conf", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let sm = h.state::<Arc<SoftwareManager>>();
            let wm = h.state::<Arc<WebsiteManager>>();
            ok(website::unlock_site_conf(sm, wm, id))
        })
    });
    let h = handle.clone();
    reg.register("generate_self_signed_cert", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let domain: String = arg(&args, "domain")?;
            let sm = h.state::<Arc<SoftwareManager>>();
            ok(website::generate_self_signed_cert(sm, domain))
        })
    });
    let h = handle.clone();
    reg.register("issue_site_certificate", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let site_id: String = arg(&args, "site_id")?;
            let c = h.state::<AppContext>();
            let wm = h.state::<Arc<WebsiteManager>>();
            let sm = h.state::<Arc<SoftwareManager>>();
            let dns = h.state::<Arc<DnsAccountManager>>();
            ok(website::issue_site_certificate(c, wm, sm, dns, site_id).await)
        })
    });

    // ===== dns_account 写命令（2）=====
    let h = handle.clone();
    reg.register("save_dns_account", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let account = arg(&args, "account")?;
            let m = h.state::<Arc<DnsAccountManager>>();
            ok(dns_account::save_dns_account(m, account))
        })
    });
    let h = handle.clone();
    reg.register("delete_dns_account", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let m = h.state::<Arc<DnsAccountManager>>();
            let wm = h.state::<Arc<WebsiteManager>>();
            ok(dns_account::delete_dns_account(m, wm, id))
        })
    });

    // ===== node_app 写命令（5；NodeApp 模块写操作，前端会调）=====
    let h = handle.clone();
    reg.register("add_node_app", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let params = arg(&args, "params")?;
            let m = h.state::<Arc<NodeAppManager>>();
            ok(node_app::add_node_app(m, params).await)
        })
    });
    let h = handle.clone();
    reg.register("update_node_app", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let params = arg(&args, "params")?;
            let m = h.state::<Arc<NodeAppManager>>();
            ok(node_app::update_node_app(m, id, params).await)
        })
    });
    let h = handle.clone();
    reg.register("delete_node_app", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let m = h.state::<Arc<NodeAppManager>>();
            ok(node_app::delete_node_app(m, id).await)
        })
    });
    let h = handle.clone();
    reg.register("start_node_app", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let m = h.state::<Arc<NodeAppManager>>();
            let sw = h.state::<Arc<SoftwareManager>>();
            ok(node_app::start_node_app(m, sw, id).await)
        })
    });
    let h = handle.clone();
    reg.register("stop_node_app", move |_ctx, args| {
        let h = h.clone();
        Box::pin(async move {
            let id: String = arg(&args, "id")?;
            let m = h.state::<Arc<NodeAppManager>>();
            ok(node_app::stop_node_app(m, id).await)
        })
    });

    // ===== 桌面专属（7）：注册但 handler 直接返回 desktop_only 信封 =====
    //
    // 语义（设计 §5 桌面专属表）：命令**可识别**（app.rs 窗口/托盘 3 +
    // system.rs app_version 1 + update.rs 更新器 3），但 HTTP 入口没有对应
    // 宿主能力（无 AppHandle 的窗口/托盘/更新器）。返回 409 而非 404：
    // 前端按 `code == "desktop_only"` 渲染「请到桌面端操作」提示，而不是
    // 当作未知命令报 bug。见 api.rs 模块文档错误契约。
    reg.register("quit_app", |_ctx, _args| {
        Box::pin(async move { Err(ApiError::desktop_only("quit_app")) })
    });
    reg.register("exit_app", |_ctx, _args| {
        Box::pin(async move { Err(ApiError::desktop_only("exit_app")) })
    });
    reg.register("hide_main_window", |_ctx, _args| {
        Box::pin(async move { Err(ApiError::desktop_only("hide_main_window")) })
    });
    reg.register("app_version", |_ctx, _args| {
        Box::pin(async move { Err(ApiError::desktop_only("app_version")) })
    });
    reg.register("check_app_update", |_ctx, _args| {
        Box::pin(async move { Err(ApiError::desktop_only("check_app_update")) })
    });
    reg.register("install_app_update", |_ctx, _args| {
        Box::pin(async move { Err(ApiError::desktop_only("install_app_update")) })
    });
    reg.register("auto_check", |_ctx, _args| {
        Box::pin(async move { Err(ApiError::desktop_only("auto_check")) })
    });
    // 批次 4.5：token 查看/重置为桌面专属——令牌明文不得经 HTTP 响应回传
    //（Web 端重置后新令牌无法送回浏览器，等于把所有会话锁在门外）
    reg.register("get_web_token", |_ctx, _args| {
        Box::pin(async move { Err(ApiError::desktop_only("get_web_token")) })
    });
    reg.register("reset_web_token", |_ctx, _args| {
        Box::pin(async move { Err(ApiError::desktop_only("reset_web_token")) })
    });

    reg
}
