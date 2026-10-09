//! 启动 / 停止 / 依赖编排 —— 批次 3B 从壳层 `commands/software.rs` 搬入。
//!
//! 本模块是「启动链路」的完整实现：`do_start_software`（启动主体）+ 依赖编排
//! （`ensure_dependencies` / `wait_dependency_ready`）+ 优雅停止
//! （`graceful_stop_software`）+ 启动前的辅助判定（端口收集 / JDK·MySQL 定位 /
//! post-start HTTP 初始化探针）。
//!
//! ## 为什么与 `lifecycle` 分两个文件
//!
//! 壳层 `commands/software.rs` 里这 13 个函数是**连续的一块**（`:846-1604`），
//! 依赖方向单一（都调`lifecycle::*` / `health_check::*` / `providers::*`），
//! 且**零 Tauri 依赖**（无 `AppHandle` / `State` / `tauri::`）——因此可整块搬入。
//! 放在独立文件而非并入既有 `lifecycle.rs`，是为了让「进程注册表/启停原语」
//! 与「一次启动的完整编排」在文件层面分开，便于后续定位。
//!
//! ## 壳层如何调用
//!
//! 壳层 `commands/software.rs` 的三个 `#[tauri::command]`（`start_software` /
//! `stop_software` / `restart_software`）只做参数接收与错误转字符串，实现全部
//! 经 `pub use` 转发到本模块。Tauri 相关类型（`State<'_, Arc<SoftwareManager>>`、
//! `AppHandle`）**只出现在壳层**，core 侧零感知。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Local;
use crate::oplog_result;

use crate::event::EventSink;
use crate::models::software::{InstalledSoftware, SoftwareStatus};
use crate::services::software_manager::health_check;
use crate::services::software_manager::lifecycle;
use crate::services::software_manager::providers::{self, HealthContext, StartContext};
use crate::services::software_manager::SoftwareManager;
use crate::utils::topo::topo_layers;


/// 收集软件启动将监听的端口，用于启动前占用校验。
/// 标准软件：取 config_schema 中 field_type=Port 的字段，从 config 读端口值（缺失回退字段默认值）。
/// 自定义软件：从 custom_start_command 的健康检查规格推导（Tcp 端口 / Http url 端口）。
/// 可见性：壳层 `get_software_port_report`（端口冲突报告）仍调它，故提为 `pub`
/// 并经壳层 `pub use` 转发。`do_start_software` 只是**顺带**用它做启动前占用预检。
pub fn collect_configured_ports(
    software: &InstalledSoftware,
    provider: Option<&dyn providers::SoftwareProvider>,
) -> Vec<u16> {
    use crate::models::software::{ConfigFieldType, CustomHealthSpec};
    let mut ports = Vec::new();
    if software.is_custom {
        if let Some(c) = &software.custom_start_command {
            match &c.health_check {
                CustomHealthSpec::Tcp { port } => ports.push(*port),
                CustomHealthSpec::Http { url, .. } => {
                    if let Some(p) = reqwest::Url::parse(url)
                        .ok()
                        .and_then(|u| u.port_or_known_default())
                    {
                        ports.push(p);
                    }
                }
                CustomHealthSpec::None => {}
            }
        }
    } else if let Some(schema) = provider.and_then(|p| p.config_schema()) {
        for field in &schema.fields {
            if matches!(field.field_type, ConfigFieldType::Port) {
                let v = software
                    .config
                    .get(&field.key)
                    .unwrap_or(&field.default_value);
                if let Some(p) = v.as_u64() {
                    if (1..=65535).contains(&p) {
                        ports.push(p as u16);
                    }
                }
            }
        }
    }
    ports
}

/// 找已安装 JDK/JRE 的 install_path（供 Nacos 等 Java 软件启动拼 java 命令）。
/// 解析启动用的 JDK/JRE install_path：
/// 优先用软件配置里选的 jdk（存 installed_id，来自表单选择，与 SpringBoot 一致），
/// 其次自动找已装 JDK/JRE（优先 JDK 其次 JRE）。找不到返回 None。
fn find_installed_jdk(
    manager: &Arc<SoftwareManager>,
    config: &serde_json::Value,
) -> Option<String> {
    let installed = manager.get_installed();
    // 1. 配置里显式选了 JDK（installed_id）→ 按 id 解析路径
    if let Some(id) = config
        .get("jdk")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        if let Some(sw) = installed.iter().find(|s| s.id == id) {
            return Some(
                crate::utils::paths::resolve_install_path(&sw.install_path)
                    .to_string_lossy()
                    .to_string(),
            );
        }
    }
    // 2. 回退：自动找第一个 JDK/JRE
    installed
        .iter()
        .filter(|s| s.key == "jdk" || s.key == "jre")
        .min_by_key(|s| if s.key == "jdk" { 0 } else { 1 })
        .map(|s| {
            crate::utils::paths::resolve_install_path(&s.install_path)
                .to_string_lossy()
                .to_string()
        })
}

/// 找已安装 MySQL 的 install_path（Nacos 选 MySQL 数据库模式时建库建表用）。
/// 返回 None 表示未装 MySQL。
fn find_installed_mysql(manager: &Arc<SoftwareManager>) -> Option<String> {
    manager
        .get_installed()
        .iter()
        .find(|s| s.key == "mysql")
        .map(|s| {
            crate::utils::paths::resolve_install_path(&s.install_path)
                .to_string_lossy()
                .to_string()
        })
}

/// 幂等探针判定：响应体与 marker 各自去掉全部空白后做子串匹配。
///
/// InfluxDB 的 `/api/v2/setup` 会带缩进换行返回（实测 `{\n\t"allowed": false\n}`），
/// 若 marker 写成 `"allowed":false` 直接 `contains` 必然漏判（冒号后有空格），
/// 表现为「每次启动都重复 POST 一次初始化」。规范化空白后比较与 JSON 排版无关。
fn probe_reports_done(body: &str, marker: &str) -> bool {
    let normalize = |s: &str| -> String { s.chars().filter(|c| !c.is_whitespace()).collect() };
    let marker = normalize(marker);
    !marker.is_empty() && normalize(body).contains(&marker)
}

/// 重复初始化判定：该响应是否表示「已经初始化过」而非真失败。
///
/// InfluxDB 2.x 对重复 onboarding 返回 **422** 且 body 为
/// `{"code":"conflict","message":"onboarding has already been completed"}`
/// （实测 2.9.1；部分版本为 409）。两者均视为幂等跳过，避免每次启动刷 error 日志。
fn post_init_already_done(status: u16, body: &str) -> bool {
    status == 409 || (status == 422 && body.to_lowercase().contains("conflict"))
}

/// 执行 post-start HTTP 初始化（如 InfluxDB 2 onboarding）。
/// 返回 Ok(true) 表示本次执行完成；Ok(false) 表示状态已满足无需执行（幂等跳过）。
/// 探针：先 GET probe_url，响应包含 probe_done_marker 则已初始化，跳过；否则 POST body。
fn run_post_start_http_init(ps: &providers::PostStartHttpInit) -> anyhow::Result<bool> {
    let client = reqwest::blocking::Client::builder()
        // 该初始化打的是 127.0.0.1 本机端口，环境代理（ALL_PROXY 等）必然劫持并失败，
        // 这里显式关掉环境变量探测；无需支持用户代理，故不引 utils::http
        .no_proxy()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| anyhow::anyhow!("HTTP client 构建失败: {}", e))?;

    // 1. 幂等探测
    if let Some(probe_url) = &ps.probe_url {
        if let Ok(resp) = client.get(probe_url).header("User-Agent", "OPX").send() {
            if resp.status().is_success() {
                if let Ok(text) = resp.text() {
                    if probe_reports_done(&text, &ps.probe_done_marker) {
                        return Ok(false); // 已初始化，跳过
                    }
                }
            }
        }
    }

    // 2. 执行初始化
    let resp = client
        .post(&ps.url)
        .header("User-Agent", "OPX")
        .header("Content-Type", "application/json")
        .json(&ps.body)
        .send()
        .map_err(|e| anyhow::anyhow!("post-start init 请求失败: {}", e))?;
    // 2xx（包含 201 Onboarding 完成）视为成功；冲突（已 onboarding）视为跳过。
    // 注意：status() 为 &self、text() 消费 resp，故先取状态码再取 body。
    let status = resp.status();
    if status.is_success() {
        return Ok(true);
    }
    let code = status.as_u16();
    let body = resp.text().unwrap_or_default();
    if post_init_already_done(code, &body) {
        return Ok(false); // InfluxDB: onboarding already completed (409/422 conflict)
    }
    anyhow::bail!("post-start init 返回 {}: {}", status, body)
}

/// 解析并拉起目标软件的依赖（拓扑序，最底层依赖先启动）。
///
/// 语义（与设计文档一致）：
/// - 依赖须处于运行态：未运行（Stopped/Unknown/Error）则先自动启动；已运行跳过；
/// - 依赖未安装 → 报错并列出缺失项；
/// - 依赖成环 → 报错并返回环路径；
/// - 依赖启动失败 → 中止本次启动链，已拉起的依赖保留运行（不回滚，避免误杀共享依赖）。
///
/// 返回本次已拉起的依赖 id 列表。
///
/// 可见性：3B 搬入 core 前为壳层私有 fn。壳层 `start_software` 的命令入口
/// 仍直接调它（`commands/software.rs` 内「先拉起依赖、再启动本体」那段编排），
/// 故提为 `pub` 并经壳层 `pub use` 转发。
pub async fn ensure_dependencies(
    manager: &Arc<SoftwareManager>,
    sink: &Arc<dyn EventSink>,
    target_id: &str,
) -> anyhow::Result<Vec<String>> {
    // 1. DFS 展开依赖闭包（含 target，用于环检测；visited 防环无限递归）
    let mut visited: Vec<String> = Vec::new();
    let mut stack: Vec<String> = vec![target_id.to_string()];
    while let Some(id) = stack.pop() {
        if visited.contains(&id) {
            continue;
        }
        visited.push(id.clone());
        if let Some(sw) = manager.find_installed(&id) {
            for dep in &sw.depends_on {
                stack.push(dep.clone());
            }
        }
    }

    // 2. 拓扑分层 + 环检测（target 也纳入，确保 target→依赖成环能检出）
    let deps = |n: &str| -> Vec<String> {
        manager
            .find_installed(n)
            .map(|sw| sw.depends_on)
            .unwrap_or_default()
    };
    let tiebreak: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let plan = topo_layers(&visited, deps, &tiebreak);
    if let Some(cycle) = plan.cycle {
        return Err(anyhow::anyhow!("检测到依赖环：{}", cycle.join(" -> ")));
    }

    // 3. 校验依赖均已安装
    let mut missing: Vec<String> = Vec::new();
    for id in &visited {
        if id.as_str() == target_id {
            continue;
        }
        if manager.find_installed(id).is_none() {
            missing.push(id.clone());
        }
    }
    if !missing.is_empty() {
        return Err(anyhow::anyhow!("依赖未安装：{}", missing.join(", ")));
    }

    // 4. 逐层拉起未运行的依赖（layers[0] 为最底层）
    let mut started: Vec<String> = Vec::new();
    for layer in &plan.layers {
        for dep_id in layer {
            if dep_id.as_str() == target_id {
                continue;
            }
            let sw = manager
                .find_installed(dep_id)
                .ok_or_else(|| anyhow::anyhow!("依赖不存在：{}", dep_id))?;
            if sw.status == SoftwareStatus::Running {
                continue; // 已在运行，跳过
            }
            // 拉起依赖（递归，依赖的依赖也按自身 depends_on 编排）。
            // 必须等到健康检查把状态翻成 Running 才能放行：
            // do_start_software 在 spawn 后即返回，不等的话依赖端口尚未监听，
            // 依赖方（如 Nacos 连 MySQL）会在依赖就绪前启动而报错。
            let dep_name = sw.name.clone();
            let dep_detail = format!("{} ({}, 依赖编排)", sw.version, sw.id);
            let outcome = match do_start_software(manager, sink, dep_id, None).await {
                Ok(()) => wait_dependency_ready(manager, dep_id).await,
                Err(e) => Err(e),
            };
            // 被拉起的依赖也是用户可见的启动动作，补记操作结果（否则操作记录缺失）
            oplog_result!("start", dep_name, dep_detail, outcome);
            outcome?;
            started.push(dep_id.clone());
        }
    }

    Ok(started)
}

/// 等待依赖编排拉起的软件就绪（健康检查把状态翻成 Running）。
/// do_start_software 返回时状态为 Starting，健康检查在游离任务中异步完成；
/// 轮询状态直到 Running / Error / 超时。健康检查本身最多 60s，这里给 90s 余量。
///
/// 可见性：随 [`ensure_dependencies`] 一同提为 `pub`（`ensure_dependencies` 内部
/// 递归调用它；壳层也可经转发直接用）。
pub async fn wait_dependency_ready(manager: &Arc<SoftwareManager>, dep_id: &str) -> anyhow::Result<()> {
    for _ in 0..90 {
        let sw = manager
            .find_installed(dep_id)
            .ok_or_else(|| anyhow::anyhow!("依赖记录消失：{}", dep_id))?;
        match sw.status {
            SoftwareStatus::Running => return Ok(()),
            SoftwareStatus::Error => {
                let msg = sw.last_error.unwrap_or_else(|| "未知原因".to_string());
                return Err(anyhow::anyhow!("依赖 [{}] 启动失败：{}", sw.name, msg));
            }
            _ => {}
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let name = manager
        .find_installed(dep_id)
        .map(|s| s.name)
        .unwrap_or_else(|| dep_id.to_string());
    Err(anyhow::anyhow!(
        "依赖 [{}] 未在 90s 内就绪，请检查其日志",
        name
    ))
}

/// 启动软件内部实现（供 start_software / restart_software / auto_start 复用）
pub async fn do_start_software(
    manager: &Arc<SoftwareManager>,
    sink: &Arc<dyn EventSink>,
    installed_id: &str,
    init_password: Option<String>,
) -> anyhow::Result<()> {
    let software = manager
        .find_installed(installed_id)
        .ok_or_else(|| anyhow::anyhow!("未找到安装记录: {}", installed_id))?;

    let providers_list = providers::all_providers();
    let provider = providers_list
        .iter()
        .find(|p| p.key() == software.key)
        .ok_or_else(|| anyhow::anyhow!("未找到 provider: {}", software.key))?;

    // 启动前端口占用校验：逐个检查配置中声明的端口是否已被占用，被占用则拒绝启动。
    // 放在此处（重启流程已先停旧进程）可避免把软件自身占用的端口误判为冲突。
    for port in collect_configured_ports(&software, Some(&**provider)) {
        if !health_check::is_port_free(port) {
            return Err(anyhow::anyhow!(
                "端口 {} 已被占用，无法启动。请修改配置端口或停止占用该端口的程序后重试。",
                port
            ));
        }
    }

    let start_ctx = StartContext {
        installed_id: software.id.clone(),
        install_path: software.install_path.clone(),
        version: software.version.clone(),
        config: software.config.clone(),
        custom_start_command: software.custom_start_command.clone(),
        init_password: init_password.clone(),
        // 需要 JDK 的软件（如 Nacos）：优先用配置里选的 JDK（installed_id），
        // 回退自动找。解析出的 install_path 供 start_command 拼 java 命令。
        jdk_install_path: find_installed_jdk(manager, &software.config),
        // Nacos 选 MySQL 数据库模式时，用已装 MySQL 的 mysql.exe 建库建表
        mysql_install_path: find_installed_mysql(manager),
    };

    // 构造 StartCommand（自定义软件走 build_custom_command，否则用 provider）
    let mut cmd = if software.is_custom {
        let custom = software
            .custom_start_command
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("自定义软件未配置启动命令"))?;
        lifecycle::build_custom_command(&software.install_path, custom)?
    } else {
        provider.start_command(&start_ctx)?
    };

    // 捕获首次初始化通过 --init-file 注入的临时 SQL 文件路径（MySQL 设置 root 密码用）。
    // RC4 修复：不在 do_start_software 返回时立即删除（InitSqlGuard 竞态），
    // 而是延迟到健康检查完成后删除——此时 mysqld 已启动并必然读取过 --init-file。
    // spawn_process 失败时在此处立即清理。
    let init_sql_path: Option<PathBuf> = cmd
        .args
        .iter()
        .find_map(|a| a.strip_prefix("--init-file="))
        .map(std::path::PathBuf::from);

    // 捕获 PostgreSQL initdb 通过 --pwfile 注入的临时明文密码文件路径。
    // PG 的 initdb 是短命进程：run_first_run_init 同步阻塞等它退出后 pwfile 必已读取，
    // 返回后立即删除即可（不复用 MySQL 的延迟删除时机）。
    let init_pwfile_path: Option<PathBuf> = cmd
        .first_run_init
        .as_ref()
        .and_then(|fri| {
            fri.init_command
                .args
                .iter()
                .find_map(|a| a.strip_prefix("--pwfile="))
        })
        .map(std::path::PathBuf::from);
    let cleanup_pwfile = || {
        if let Some(ref p) = init_pwfile_path {
            let _ = std::fs::remove_file(p);
        }
    };

    // 首次初始化（如 mysqld --initialize-insecure）
    // 用 take() 取出所有权，避免后续 spawn_process(cmd) 时 cmd 仍被借用
    if let Some(fri) = cmd.first_run_init.take() {
        let initialized = software
            .config
            .get("initialized")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let data_dir = fri.init_command.working_dir.join("data");
        if initialized {
            // 已成功初始化过（config.initialized == true），直接跳过
            tracing::info!(
                installed_id = %installed_id,
                data_dir = %data_dir.display(),
                "already initialized, skipping first_run_init"
            );
            // 防御性清理：已初始化不应再有 PG pwfile 明文残留
            cleanup_pwfile();
        } else {
            // 未初始化：若 data 目录非空（上次 init 超时/kill 残留的半初始化文件，
            // 即 RC3 链式放大），先彻底清空再重新初始化，避免用损坏的 data 目录
            // 直接拉起 mysqld 导致卡死/崩溃。
            let wiped = lifecycle::wipe_data_dir_if_nonempty(&data_dir)?;
            if wiped {
                tracing::warn!(
                    installed_id = %installed_id,
                    data_dir = %data_dir.display(),
                    "data dir non-empty but not initialized; wiped before re-init (RC3)"
                );
            }
            // 继续执行下方初始化流程
            // 打印初始化命令方便诊断
            tracing::info!(
                installed_id = %installed_id,
                program = %fri.init_command.program,
                args = ?fri.init_command.args,
                working_dir = %fri.init_command.working_dir.display(),
                "running first_run_init"
            );

            // 确保 data 目录存在（MySQL 要求 datadir 存在）
            let _ = std::fs::create_dir_all(&data_dir);
            manager.update_runtime_fields(
                installed_id,
                SoftwareStatus::Initializing,
                None,
                None,
                None,
                None,
            )?;
            lifecycle::emit_status_changed(
                sink,
                installed_id,
                SoftwareStatus::Initializing,
                None,
                None,
            );

            // 用 spawn_blocking 包裹阻塞的 output() 调用
            // fri 的所有权移入闭包，闭包内 &fri 借用闭包自身拥有的数据，满足 'static
            let init_result =
                tokio::task::spawn_blocking(move || lifecycle::run_first_run_init(&fri))
                    .await
                    .map_err(|e| anyhow::anyhow!("初始化任务 join 失败: {}", e));

            // 初始化失败时恢复状态为 Error（否则会卡在 Initializing 无法卸载/重启）
            let output = match init_result {
                Ok(Ok(output)) => output,
                Ok(Err(e)) => {
                    let msg = format!("初始化失败：{}", e);
                    manager.update_runtime_fields(
                        installed_id,
                        SoftwareStatus::Error,
                        None,
                        None,
                        None,
                        Some(msg.clone()),
                    )?;
                    lifecycle::emit_status_changed(
                        sink,
                        installed_id,
                        SoftwareStatus::Error,
                        None,
                        Some(msg),
                    );
                    cleanup_pwfile();
                    return Err(e);
                }
                Err(e) => {
                    let err = anyhow::anyhow!("初始化任务 join 失败: {}", e);
                    manager.update_runtime_fields(
                        installed_id,
                        SoftwareStatus::Error,
                        None,
                        None,
                        None,
                        Some(format!("{}", err)),
                    )?;
                    cleanup_pwfile();
                    return Err(err);
                }
            };
            // 初始化命令的 stdout/stderr 均为 null（见 lifecycle::run_first_run_init），无需读取
            let _ = output;

            // initdb 已退出（pwfile 必已读取），立即删除临时明文密码文件
            cleanup_pwfile();

            // 标记 initialized = true
            let mut new_config = software.config.clone();
            if let Some(obj) = new_config.as_object_mut() {
                obj.insert("initialized".to_string(), serde_json::json!(true));
            } else {
                new_config = serde_json::json!({ "initialized": true });
            }
            manager.update_config(installed_id, new_config)?;
        }
    }

    // spawn 子进程（打印完整命令方便诊断启动问题）
    tracing::info!(
        installed_id = %installed_id,
        program = %cmd.program,
        args = ?cmd.args,
        working_dir = %cmd.working_dir.display(),
        env_vars = ?cmd.env_vars,
        "spawning software"
    );

    let child = match lifecycle::spawn_process(cmd, installed_id) {
        Ok(child) => child,
        Err(e) => {
            if let Some(ref p) = init_sql_path {
                let _ = std::fs::remove_file(p);
            }
            return Err(e.into());
        }
    };
    let pid = lifecycle::monitored_pid(&child);

    // 更新状态为 Starting，清除旧的 last_error（避免启动成功后仍显示旧错误）
    manager.update_runtime_fields(
        installed_id,
        SoftwareStatus::Starting,
        Some(pid),
        Some(Local::now().naive_local()),
        None,
        None,
    )?;
    let _ = manager.clear_last_error(installed_id);

    // 注册到 lifecycle
    let kind = format!("{:?}", provider.catalog_entry().category);
    lifecycle::register(
        installed_id.to_string(),
        pid,
        format!("{} {}", software.name, software.version),
        software.key.clone(),
        kind,
        software.startup_order,
    );

    // emit 时 error 显式传 None（清除前端旧错误）
    lifecycle::emit_status_changed(sink, installed_id, SoftwareStatus::Starting, Some(pid), None);
    tracing::info!(installed_id = %installed_id, pid = pid, "start_software spawned");

    // 异步健康检查（30 次 × 1s 间隔，最多 30s）
    let hctx = HealthContext {
        installed_id: installed_id.to_string(),
        install_path: software.install_path.clone(),
        port: software.port,
        config: software.config.clone(),
    };
    let spec = if software.is_custom {
        // 自定义软件的健康检查从 custom_start_command 推导
        match &software.custom_start_command {
            Some(c) => match &c.health_check {
                crate::models::software::CustomHealthSpec::None => {
                    crate::models::software::HealthCheckSpec::ProcessOnly
                }
                crate::models::software::CustomHealthSpec::Tcp { port } => {
                    crate::models::software::HealthCheckSpec::Tcp {
                        port: *port,
                        timeout_ms: 1000,
                    }
                }
                crate::models::software::CustomHealthSpec::Http {
                    url,
                    expected_status,
                } => crate::models::software::HealthCheckSpec::Http {
                    url: url.clone(),
                    expected_status: *expected_status,
                    timeout_ms: 1000,
                },
            },
            None => crate::models::software::HealthCheckSpec::ProcessOnly,
        }
    } else {
        provider.health_check(&hctx)
    };

    // post-start 一次性 HTTP 初始化（如 InfluxDB 2 onboarding）。
    // 在闭包 move 前从 provider 计算好，随闭包传入健康检查通过后执行。
    let post_start = if software.is_custom {
        None
    } else {
        provider.post_start_http_init(&hctx)
    };

    let manager_clone = manager.clone();
    let sink_for_check = sink.clone();
    let installed_id_clone = installed_id.to_string();
    let pid_for_check = pid;
    let init_sql_path_for_cleanup = init_sql_path.clone();
    let post_start_for_check = post_start;
    tokio::spawn(async move {
        // 健康检查前先检查进程是否存活（避免进程崩溃后误报"健康检查超时"）
        let pid_alive =
            tokio::task::spawn_blocking(move || health_check::is_process_alive(pid_for_check))
                .await
                .unwrap_or(false);
        if !pid_alive {
            let _ = manager_clone.update_runtime_fields(
                &installed_id_clone,
                SoftwareStatus::Error,
                None,
                None,
                None,
                Some("进程意外退出（启动后立即崩溃，请检查端口冲突或 data 目录权限）".to_string()),
            );
            lifecycle::emit_status_changed(
                &sink_for_check,
                &installed_id_clone,
                SoftwareStatus::Error,
                None,
                Some("进程意外退出".to_string()),
            );
            lifecycle::unregister(&installed_id_clone);
            tracing::error!(
                installed_id = %installed_id_clone,
                pid = pid_for_check,
                "process exited immediately after spawn"
            );
            if let Some(ref p) = init_sql_path_for_cleanup {
                let _ = std::fs::remove_file(p);
            }
            return;
        }
        // 60 次 × 1s = 最多 60s，给慢启动软件（如 MinIO/RustFS）足够 ready 时间
        let result = health_check::run_health_check(&spec, Some(pid), 60, 1000).await;
        match result {
            health_check::HealthCheckResult::Healthy => {
                let _ = manager_clone.update_runtime_fields(
                    &installed_id_clone,
                    SoftwareStatus::Running,
                    Some(pid),
                    None,
                    None,
                    None,
                );
                lifecycle::emit_status_changed(
                    &sink_for_check,
                    &installed_id_clone,
                    SoftwareStatus::Running,
                    Some(pid),
                    None,
                );
                tracing::info!(installed_id = %installed_id_clone, "software healthy");

                // post-start 一次性初始化（InfluxDB 2 onboarding 等）：
                // 健康检查通过 → 服务已就绪 → 执行 HTTP 初始化 → 成功后写 config.initialized=true
                // 与 config 回写字段（如 admin_token）。幂等依据探针（allowed=false 则跳过）。
                if let Some(ps) = &post_start_for_check {
                    let ps = ps.clone();
                    let config_fields = ps.config_fields.clone();
                    let manager_ps = manager_clone.clone();
                    let installed_ps = installed_id_clone.clone();
                    let onb =
                        tokio::task::spawn_blocking(move || run_post_start_http_init(&ps)).await;
                    match onb {
                        Ok(Ok(true)) => {
                            // 回写 config（initialized + 额外字段）
                            let cur_cfg = manager_ps
                                .find_installed(&installed_ps)
                                .map(|s| s.config.clone());
                            if let Some(cfg) = cur_cfg {
                                let mut new_cfg = cfg;
                                if let Some(obj) = new_cfg.as_object_mut() {
                                    obj.insert("initialized".to_string(), serde_json::json!(true));
                                    for (k, v) in &config_fields {
                                        obj.insert(k.clone(), v.clone());
                                    }
                                }
                                let _ = manager_ps.update_config(&installed_ps, new_cfg);
                            }
                            tracing::info!(
                                installed_id = %installed_ps,
                                "post-start init succeeded"
                            );
                        }
                        Ok(Ok(false)) => {
                            tracing::info!(
                                installed_id = %installed_ps,
                                "post-start init skipped (already done)"
                            );
                        }
                        Ok(Err(e)) => {
                            // 初始化失败不阻塞运行；记录日志，用户可稍后手动处理。
                            tracing::error!(
                                installed_id = %installed_ps,
                                error = %e,
                                "post-start init failed (non-fatal)"
                            );
                        }
                        Err(e) => {
                            tracing::error!("post-start init join failed: {}", e);
                        }
                    }
                }
            }
            health_check::HealthCheckResult::Timeout => {
                // 关键：健康检查失败必须清理子进程树，否则残留僵尸软件（如 nginx）累积
                // 重复占用端口 → 后续健康检查更易失败 → 恶性循环
                let dead_pid = pid;
                let _ = tokio::task::spawn_blocking(move || lifecycle::stop_one(dead_pid)).await;
                lifecycle::unregister(&installed_id_clone);
                let _ = manager_clone.update_runtime_fields(
                    &installed_id_clone,
                    SoftwareStatus::Error,
                    None,
                    None,
                    None,
                    Some("健康检查超时".to_string()),
                );
                lifecycle::emit_status_changed(
                    &sink_for_check,
                    &installed_id_clone,
                    SoftwareStatus::Error,
                    None,
                    Some("健康检查超时".to_string()),
                );
            }
            health_check::HealthCheckResult::ProcessExited => {
                let _ = manager_clone.update_runtime_fields(
                    &installed_id_clone,
                    SoftwareStatus::Error,
                    None,
                    None,
                    None,
                    Some("进程意外退出".to_string()),
                );
                lifecycle::emit_status_changed(
                    &sink_for_check,
                    &installed_id_clone,
                    SoftwareStatus::Error,
                    None,
                    Some("进程意外退出".to_string()),
                );
                lifecycle::unregister(&installed_id_clone);
            }
        }
        // RC4: 健康检查完成后清理 init SQL 文件。
        // 此时 mysqld 已经历完整启动序列（或已退出），无论结果如何，
        // --init-file 都已被读取（或不再需要），可以安全删除明文密码文件。
        if let Some(ref p) = init_sql_path_for_cleanup {
            let _ = std::fs::remove_file(p);
        }
    });

    Ok(())
}

/// P1-3 语义化优雅停止：优先按 provider 返回的关闭命令停止（数据库/中间件避免被强杀损坏），
/// 失败/超时回退 lifecycle::stop_one 强杀。返回 (是否成功, 状态串)。
///
/// 选择逻辑：按 installed_id 找到 provider → 构造 StopContext → 调
/// `provider.graceful_stop_command`；返回 None（如 MySQL root 密码为一次性 ephemeral、
/// 未持久化，mysqladmin shutdown 无法认证）或无对应 provider → 直接强杀。
///
/// 可见性：3B 搬入 core 前为壳层私有 fn。壳层 `stop_software`（`:1668`）与
/// `restart_software`（`:1756`）都调它，故提为 `pub` 并经壳层 `pub use` 转发。
/// 自身不被 [`do_start_software`] 调用——停止与启动是两条独立链路。
pub fn graceful_stop_software(software: &InstalledSoftware, pid: u32) -> (bool, String) {
    let provider = providers::all_providers()
        .into_iter()
        .find(|p| p.key() == software.key);
    let Some(provider) = provider else {
        return lifecycle::stop_one(pid);
    };
    let ctx = providers::StopContext {
        installed_id: software.id.clone(),
        install_path: software.install_path.clone(),
        version: software.version.clone(),
        config: software.config.clone(),
        port: software.port,
    };
    let Some(cmd) = provider.graceful_stop_command(&ctx) else {
        return lifecycle::stop_one(pid);
    };
    if lifecycle::run_graceful_stop(&cmd, pid) {
        (true, "stopped".to_string())
    } else {
        lifecycle::stop_one(pid)
    }
}

// ============================================================================
// 单测
// ============================================================================
//
// 3B 随 [`probe_reports_done`] / [`post_init_already_done`] 一同从壳层
// `commands/software.rs` 的测试模块迁入——单测与被测函数同处一个 crate，
// 无需跨 crate 引用，也不产生新的公开 API。
#[cfg(test)]
mod tests {
    use super::{post_init_already_done, probe_reports_done};

    /// 回归：InfluxDB 的 /api/v2/setup 带缩进返回，探针必须与 JSON 排版无关。
    /// 实测 body 为 `{\n\t"allowed": false\n}`（冒号后有空格），原先直接 contains
    /// 写死无空格的 marker 会导致幂等探测永不命中 → 每次启动重复初始化。
    #[test]
    fn probe_reports_done_ignores_whitespace() {
        let marker = r#""allowed":false"#;
        // 未初始化（首次启动）→ 不应命中
        assert!(!probe_reports_done("{\n\t\"allowed\": true\n}", marker));
        // 已初始化（实测 InfluxDB 2.9.1 格式）→ 必须命中
        assert!(probe_reports_done("{\n\t\"allowed\": false\n}", marker));
        // 紧凑格式同样命中
        assert!(probe_reports_done(r#"{"allowed":false}"#, marker));
        // 空marker 不得误判为已完成
        assert!(!probe_reports_done("anything", ""));
    }

    /// 回归：InfluxDB 2.9.1 对重复 onboarding 返回 422 + code=conflict（非 409），
    /// 原先只认 409 → 每次启动都走 bail 记一条 "post-start init failed" 错误日志。
    #[test]
    fn post_init_already_done_accepts_conflict() {
        let influx_422 = r#"{"code":"conflict","message":"onboarding has already been completed"}"#;
        assert!(post_init_already_done(422, influx_422));
        assert!(post_init_already_done(409, "")); // 其他版本用 409
        // 真失败不得被吞掉
        assert!(!post_init_already_done(422, r#"{"code":"invalid","message":"password too short"}"#));
        assert!(!post_init_already_done(500, ""));
        assert!(!post_init_already_done(404, "not found"));
    }
}
