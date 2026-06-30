use std::fs;
use std::path::{Path, PathBuf};

/// 应用根目录 = exe 所在目录。
/// current_exe 失败回退 current_dir，再失败回退 ".".
pub fn app_root() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            return parent.to_path_buf();
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        return cwd;
    }
    PathBuf::from(".")
}

/// settings.json 路径（exe 同级）。父目录即 app_root，确保存在。
pub fn settings_path() -> PathBuf {
    let root = app_root();
    let _ = fs::create_dir_all(&root);
    root.join("settings.json")
}

/// 通用：在 root 下解析子目录名，自动创建。
/// name 为空时使用 fallback 兜底。
fn ensure_dir(root: &Path, name: &str, fallback: &str) -> PathBuf {
    let n = if name.trim().is_empty() { fallback } else { name };
    let p = root.join(n);
    let _ = fs::create_dir_all(&p);
    p
}

/// 用户安装的软件目录：<app_root>/apps
pub fn apps_dir() -> PathBuf {
    ensure_dir(&app_root(), "apps", "apps")
}

/// 用户软件配置目录：<app_root>/config
pub fn config_dir() -> PathBuf {
    ensure_dir(&app_root(), "config", "config")
}

/// OPX 运行数据目录：<app_root>/data
pub fn data_dir() -> PathBuf {
    ensure_dir(&app_root(), "data", "data")
}

/// 临时目录：<app_root>/tmp
pub fn tmp_dir() -> PathBuf {
    ensure_dir(&app_root(), "tmp", "tmp")
}

/// 日志目录：<app_root>/logs
pub fn logs_dir() -> PathBuf {
    ensure_dir(&app_root(), "logs", "logs")
}

/// 通用辅助：在指定 root 下解析 name。
/// name 空时返回 root 本身，否则返回 root.join(name)。
pub fn resolve_under(root: &Path, name: &str) -> PathBuf {
    if name.is_empty() {
        root.to_path_buf()
    } else {
        root.join(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_root_is_directory() {
        let root = app_root();
        assert!(root.is_dir() || root.to_string_lossy() == ".", "app_root 应为目录");
    }

    #[test]
    fn settings_path_ends_with_settings_json() {
        let p = settings_path();
        assert!(p.ends_with("settings.json"));
        assert!(p.parent().is_some());
    }

    #[test]
    fn dir_funcs_return_app_root_subdir() {
        let root = app_root();
        assert_eq!(apps_dir(), root.join("apps"));
        assert_eq!(config_dir(), root.join("config"));
        assert_eq!(data_dir(), root.join("data"));
        assert_eq!(tmp_dir(), root.join("tmp"));
        assert_eq!(logs_dir(), root.join("logs"));
    }

    #[test]
    fn dir_funcs_create_directory() {
        let _ = apps_dir();
        let _ = config_dir();
        let _ = data_dir();
        let _ = tmp_dir();
        let _ = logs_dir();
        assert!(apps_dir().is_dir());
        assert!(config_dir().is_dir());
        assert!(data_dir().is_dir());
        assert!(tmp_dir().is_dir());
        assert!(logs_dir().is_dir());
    }

    #[test]
    fn resolve_under_handles_empty_and_normal() {
        let root = app_root();
        assert_eq!(resolve_under(&root, ""), root);
        assert_eq!(resolve_under(&root, "foo"), root.join("foo"));
    }
}
