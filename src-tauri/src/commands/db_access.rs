//! 数据库「能不能从外部连上」的辅助命令。
//!
//! 监听地址只是第一道门：即使 `bind-address=0.0.0.0`、防火墙也放行了，
//! - MySQL 还会按**账号来源主机**拒绝（`ERROR 1130 Host '192.168.200.1' is not
//!   allowed to connect`）——opx 初始化出来的账号只有 `root@localhost`；
//! - Redis 的保护模式会拒掉所有非回环连接（见 redis provider 的保护模式说明）。
//!
//! 本模块只解决 MySQL 这一层：用安装时保存的 root 密码，在实例内部执行一次
//! 授权，建出允许远程登录的同名账号。命令必须由用户显式点击触发（不静默授权：
//! 建一个任意来源可登录的 root 是明确的安全权衡，得让用户自己按下这个按钮）。

use tauri::State;
use opx_core::services::software_manager::netutils;
use opx_core::services::software_manager::SoftwareManager;
use opx_core::utils::process::hidden;
use opx_core::audited;

/// MySQL 客户端可执行文件名（Windows 带 .exe）。
fn mysql_client(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let exe = if cfg!(windows) { "mysql.exe" } else { "mysql" };
    let direct = dir.join("bin").join(exe);
    if direct.is_file() {
        return Some(direct);
    }
    // Linux/macOS 的 tar 包保留顶层目录（mysql-8.4.11-linux-glibc2.28-x86_64/bin）
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let candidate = entry.path().join("bin").join(exe);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// SQL 字符串字面量转义（先反斜杠再单引号）。
fn sql_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

/// 允许 MySQL 实例从任意主机以 root 登录（创建/更新 `root@'%'`）。
///
/// 前置条件：实例正在运行，且配置里保存了初始化密码（`init_password`）。
/// 返回给前端的是一句可直接展示的说明，失败时错误里带 mysql 客户端的 stderr。
#[tauri::command]
pub fn allow_mysql_remote(
    sm: State<'_, std::sync::Arc<SoftwareManager>>,
    installed_id: String,
) -> Result<String, String> {
    let software = sm
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {installed_id}"))?;
    if software.key != "mysql" {
        return Err("该命令仅适用于 MySQL 实例".to_string());
    }
    if software.status != opx_core::models::software::SoftwareStatus::Running {
        return Err("请先启动 MySQL 再执行远程授权".to_string());
    }
    let password = software
        .config
        .get("init_password")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if password.is_empty() {
        return Err(
            "该实例没有保存 root 密码（老版本安装），无法自动授权。请在服务器上手动执行：\n\
             CREATE USER IF NOT EXISTS 'root'@'%' IDENTIFIED BY '<你的密码>';\n\
             GRANT ALL PRIVILEGES ON *.* TO 'root'@'%' WITH GRANT OPTION;\n\
             FLUSH PRIVILEGES;"
                .to_string(),
        );
    }

    let install_path = std::path::PathBuf::from(&software.install_path);
    let client = mysql_client(&install_path)
        .ok_or_else(|| format!("找不到 mysql 客户端：{}", install_path.display()))?;
    let port = software
        .config
        .get("port")
        .and_then(|v| v.as_u64())
        .map(|p| p as u16)
        .unwrap_or(3306);
    // 连接地址跟随实际 bind：只绑网卡时 127.0.0.1 连不上
    let host = netutils::bind_from_config(&software.config)
        .map(|b| netutils::probe_host_for_bind(&b))
        .unwrap_or_else(|| "127.0.0.1".to_string());

    let pw = sql_escape(&password);
    let sql = format!(
        "CREATE USER IF NOT EXISTS 'root'@'%' IDENTIFIED BY '{pw}';\
         ALTER USER 'root'@'%' IDENTIFIED BY '{pw}';\
         GRANT ALL PRIVILEGES ON *.* TO 'root'@'%' WITH GRANT OPTION;\
         FLUSH PRIVILEGES;"
    );

    // 授权必须从「本机身份」连进去：服务只绑网卡时 TCP 连 192.168.x.x 会被当成
    // 远程主机（Host '192.168.200.128' is not allowed —— root 只有 localhost），
    // 连自己都授不了权。故按「socket → 回环 → 实际 bind 地址」依次尝试。
    let port_arg = port.to_string();
    let mut attempts: Vec<Vec<String>> = Vec::new();
    #[cfg(unix)]
    attempts.push(vec!["--protocol=SOCKET".to_string()]);
    attempts.push(vec![
        "--protocol=TCP".to_string(),
        "-h".to_string(),
        "127.0.0.1".to_string(),
        "-P".to_string(),
        port_arg.clone(),
    ]);
    if host != "127.0.0.1" {
        attempts.push(vec![
            "--protocol=TCP".to_string(),
            "-h".to_string(),
            host.clone(),
            "-P".to_string(),
            port_arg.clone(),
        ]);
    }

    let mut errors: Vec<String> = Vec::new();
    for args in attempts {
        let out = hidden(&client)
            .args(&args)
            .args(["-u", "root", "-e", &sql])
            .env("MYSQL_PWD", &password)
            .output()
            .map_err(|e| format!("无法执行 mysql 客户端：{e}"))?;
        if out.status.success() {
            let target = software.name.clone();
            let detail = format!("root@% (via {})", args.join(" "));
            let _ = audited!("allow_mysql_remote", target, detail, {
                Ok::<(), String>(())
            });
            return Ok(format!(
                "已允许 root 从任意主机登录（root@'%'，密码同初始化密码，端口 {port}）"
            ));
        }
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let err = if err.is_empty() {
            format!("退出码 {:?}", out.status.code())
        } else {
            err
        };
        errors.push(format!("{}: {err}", args.join(" ")));
    }

    Err(format!("授权失败：{}", errors.join("；")))
}
