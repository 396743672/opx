//! 站点 / nginx 的纯逻辑片段（阶段 3 搬迁中，临时寄居于 `utils`）。
//!
//! 原在壳层 `opx::commands::website`。这三个函数被 `services::acme::renew_scheduler`
//! 反向引用，是 acme 搬进 core 的最后障碍，故先把**纯逻辑部分**下沉。
//!
//! # 为什么寄居 `utils/` 而非 `services/`
//! 最终归属应是 `opx_core::services::website_manager`（与 `WebsiteManager`、
//! `nginx_conf` 同处）。但这两个宿主件尚未搬入 core（见下），此刻在 core 里新建
//! `services/` 树会被后续批次重塑两次。故先放`utils/`，待批次 3/4 把
//! `WebsiteManager` + `nginx_conf` 搬进来时一次性归位。
//!
//! # 已下沉 / 未下沉
//! - [`sanitize_domain`] —— 纯字符串校验，完全下沉。
//! - [`resolve_nginx`] —— 原本收`&SoftwareManager`，改为收`&[InstalledSoftware]`
//!   切片后即为纯函数（只需读 `key` 与 `install_path`），从而**不必等
//!   `SoftwareManager` 搬进 core** 即可下沉。壳层保留同名薄封装维持原签名。
//! - `regenerate` —— **本轮不下沉**。它虽是纯文件 IO + 子进程调用（无 tauri），
//!   但依赖 `nginx_conf::{site_conf_filename, generate_server_block, ensure_include}`
//!   与 `WebsiteManager`，三者都还在壳层services。强行下沉要么把 `nginx_conf`
//!   一并搬走（超出本轮范围），要么把 `nginx_conf` 的调用反注入进来（更脏）。
//!   留待批次 3/4 与 `website_manager` 一并归位。

use crate::models::software::InstalledSoftware;
use crate::utils::paths;

/// 解析目标 nginx（软件管理里已安装的第一个 nginx 实例）。
///
/// ponytail: 单 nginx 假设；多实例选择留待后续（Site 加 nginx_id）。
///
/// 参数是**已装软件切片**而非 `SoftwareManager` —— 本函数只用到 `key` 与
/// `install_path` 两个字段，收切片后即与 `SoftwareManager` 解耦，可在
/// `SoftwareManager` 尚处壳层时就搬进 core。调用方（`regenerate` 与 acme
/// 续期）经壳层薄封装传入 `&sm.get_installed()`，调用点零改动。
pub fn resolve_nginx(installed: &[InstalledSoftware]) -> Result<InstalledSoftware, String> {
    let mut nginx = installed
        .iter()
        .find(|s| s.key == "nginx")
        .cloned()
        .ok_or_else(|| "请先在软件管理中安装 nginx".to_string())?;
    nginx.install_path = paths::resolve_install_path(&nginx.install_path)
        .to_string_lossy()
        .to_string();
    Ok(nginx)
}

/// 校验并规范化域名：去空白；空、含空白或路径分隔符则拒绝。
pub fn sanitize_domain(domain: &str) -> Result<String, String> {
    let d = domain.trim();
    if d.is_empty() {
        return Err("请先填写 server_name（域名）".to_string());
    }
    if d
        .chars()
        .any(|c| c.is_whitespace() || c == '/' || c == '\\')
    {
        return Err(format!("域名不合法: {}", d));
    }
    Ok(d.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 无nginx 实例时必须给出可操作的提示（而非 unwrap panic）——
    /// 这是 acme 续期轮询里唯一的失败出口。
    #[test]
    fn resolve_nginx_requires_installed_nginx() {
        let err = resolve_nginx(&[]).unwrap_err();
        assert!(err.contains("nginx"), "错误信息应指明缺 nginx：{}", err);
    }

    /// 域名校验的三类拒绝路径 + 规范化。
    #[test]
    fn sanitize_domain_trims_and_rejects_bad() {
        assert_eq!(sanitize_domain("  example.com ").unwrap(), "example.com");
        assert!(sanitize_domain("").is_err());
        assert!(sanitize_domain("   ").is_err());
        // 空白 / 正斜杠 / 反斜杠均须拒绝（防注入到 nginx 配置的 server_name）
        assert!(sanitize_domain("a b.com").is_err());
        assert!(sanitize_domain("a/b.com").is_err());
        assert!(sanitize_domain("a\\b.com").is_err());
    }
}