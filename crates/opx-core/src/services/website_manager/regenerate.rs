//! nginx 配置重建：把站点列表同步成 `conf/sites/*.conf`，可选 reload。
//!
//! 原在壳层 `opx::commands::website`（`regenerate` / `run_nginx` / `sync_site_files`）。
//! 本模块**零 tauri 依赖**：只做文件 IO 与子进程调用，故搬入 core 供两个壳共用——
//! headless 壳同样需要「改了站点列表就重建 nginx 配置」的能力。
//!
//! ## 分层约定（重要）
//! 本模块**不认识 `SoftwareManager`**（它还在壳层，第3批才搬）。`regenerate` 原本收
//! `sm: &SoftwareManager` 且只用一个动作——`resolve_nginx(sm)`，即读 `get_installed()`。
//! 故按「按字段切而非按类型切」原则改为收 `&[InstalledSoftware]` 切片，
//! 使本模块与 `SoftwareManager` 解耦。壳层 `commands::website::regenerate` 保留薄封装，
//! 负责传入 `&sm.get_installed()` 维持原签名。
//!
//! ⚠️ 本模块**不得**调用壳层的 `resolve_nginx` 薄封装 —— 那会形成 core → 壳层的
//! 反向依赖。直接调 [`crate::utils::website::resolve_nginx`]（core 版，收切片）。

use std::path::{Path, PathBuf};

use crate::models::software::{InstalledSoftware, SoftwareStatus};
use crate::models::website::Site;
use crate::services::website_manager::{nginx_conf, WebsiteManager};
use crate::utils::{process, website};

/// 调用 nginx 二进制执行子命令（`-t` 校验 / `-s reload` 重载）。
///
/// ## 平台差异（**必须保留**，勿"顺手统一"）
/// - **Windows**：用 `install_path.join("nginx.exe")`。OPX 自装的 nginx 就在
///   `install_path` 下，走绝对/相对路径定位，不依赖 PATH。
/// - **非 Windows**：用裸 `"nginx"` 走 **PATH** 查找 —— 系统 nginx 由包管理器装在
///   PATH 里（如 `/usr/sbin/nginx`），`install_path` 下**没有** `nginx.exe`。
///   若把两分支统一成 `install_path.join("nginx.exe")`，Linux/macOS 上会因找不到程序
///   而静默失败，属跨平台回归。
///
/// ## 静默启动
/// 走 [`process::hidden`]（Windows `CREATE_NO_WINDOW`）避免弹出控制台窗口——与
/// 壳层原先手写的 `creation_flags(0x08000000)` 语义完全一致，但复用 core 的既有实现
/// 可让两个壳共享同一份标志位语义，杜绝各自硬编码写漂。
///
/// ## 为何 `pub`
/// 可重导出给壳层 `commands::website::set_site_conf`（手写 conf 落盘后也要
/// `nginx -t` + reload），避免壳层再抄一份平台分支各自漂移。
pub fn run_nginx(install_path: &Path, args: &[&str]) -> std::io::Result<std::process::Output> {
    #[cfg(windows)]
    let program = install_path.join("nginx.exe");
    #[cfg(not(windows))]
    let program = "nginx";

    process::hidden(program)
        .args(args)
        .current_dir(install_path)
        .output()
}

/// 从当前站点列表重建 conf/sites/*.conf（幂等，天然处理删除/下线）。
///
/// 文件处理规则见 [`sync_site_files`]。
///
/// `installed` 为已装软件列表（用于定位 nginx 实例）；`reload` 为 true 且 nginx
/// 正在运行时，先 `nginx -t` 校验配置，通过后 `nginx -s reload`。
pub fn regenerate(
    installed: &[InstalledSoftware],
    wm: &WebsiteManager,
    reload: bool,
) -> Result<(), String> {
    let nginx = website::resolve_nginx(installed)?;
    let base = PathBuf::from(&nginx.install_path);
    let conf_dir = base.join("conf");
    let sites_dir = conf_dir.join("sites");
    std::fs::create_dir_all(&sites_dir).map_err(|e| e.to_string())?;

    sync_site_files(&sites_dir, &wm.list()).map_err(|e| e.to_string())?;

    // 确保主配置 include（幂等）
    let main_conf = conf_dir.join("nginx.conf");
    if let Ok(content) = std::fs::read_to_string(&main_conf) {
        let updated = nginx_conf::ensure_include(&content);
        if updated != content {
            std::fs::write(&main_conf, updated).map_err(|e| e.to_string())?;
        }
    }

    if reload && nginx.status == SoftwareStatus::Running {
        let test = run_nginx(&base, &["-t"]).map_err(|e| e.to_string())?;
        if !test.status.success() {
            tracing::warn!(stderr = %String::from_utf8_lossy(&test.stderr), "nginx 配置校验失败");
            return Err("i18n:nginxConfInvalid".to_string());
        }
        let rl = run_nginx(&base, &["-s", "reload"]).map_err(|e| e.to_string())?;
        if !rl.status.success() {
            tracing::warn!(stderr = %String::from_utf8_lossy(&rl.stderr), "nginx reload 失败");
            return Err("i18n:nginxReloadFailed".to_string());
        }
    }
    Ok(())
}

/// 同步 sites 目录下的 conf 文件到与站点列表一致的状态（纯文件 IO，便于单测）。
///
/// # 规则（全部站点统一）
/// - **启用** → 确保 `.conf` 存在（若当前为 `.conf.disabled` 则改回；不存在则从数据生成）
/// - **停用** → `.conf` 改名为 `.conf.disabled`（内容保留，nginx 不加载）
/// - **已删除** → 清理阶段删掉不再属于任何站点的 `.conf` / `.conf.disabled`
/// - **手写站点** 的 `.conf` 通过源码视图写入，此处仅改名不覆盖
pub fn sync_site_files(sites_dir: &Path, sites: &[Site]) -> std::io::Result<()> {
    // 所有站点的文件名（启用态 .conf 与停用态 .conf.disabled）都要保护，清理阶段不得删除
    let protected: std::collections::HashSet<String> = sites
        .iter()
        .flat_map(|s| {
            let f = nginx_conf::site_conf_filename(s);
            [format!("{}.disabled", f), f]
        })
        .collect();

    // 清理：删除不属于任何站点的残留 .conf / .conf.disabled（如已删除站点）
    if let Ok(rd) = std::fs::read_dir(sites_dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let is_conf_like = name.ends_with(".conf") || name.ends_with(".conf.disabled");
            if is_conf_like && !protected.contains(name) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    for site in sites {
        let fname = nginx_conf::site_conf_filename(site);
        let conf_path = sites_dir.join(&fname);
        let disabled_path = sites_dir.join(format!("{}.disabled", fname));

        if site.enabled {
            // 启用态：确保 .conf 存在
            if disabled_path.exists() && !conf_path.exists() {
                // 从停用恢复：只改名，不写内容
                std::fs::rename(&disabled_path, &conf_path)?;
            } else if !conf_path.exists() && !disabled_path.exists() {
                // 全新启用：从数据生成
                let block = nginx_conf::generate_server_block(site);
                std::fs::write(&conf_path, block)?;
            } else if !site.custom_conf {
                // ponytail: 表单模式 → 每次保存都从表单数据重建 .conf，确保路由更改生效
                let block = nginx_conf::generate_server_block(site);
                std::fs::write(&conf_path, block)?;
            } // 手写模式（custom_conf=true）→ 跳过，保留源码视图写入的内容
        } else {
            // 停用态：改为 .disabled（nginx 不加载）
            if conf_path.exists() {
                std::fs::rename(&conf_path, &disabled_path)?;
            }
            // ponytail: 表单模式 → 停用态也随表单重建 .disabled，
            // 避免站点停用时编辑（如切换 SSL/路由）后配置残留旧内容，下次启用备份失效
            if !site.custom_conf && disabled_path.exists() {
                let block = nginx_conf::generate_server_block(site);
                std::fs::write(&disabled_path, block)?;
            }
        }
    }
    Ok(())
}