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

/// 缓存目录：<app_root>/cache
/// 持久保留下载的压缩包，避免相同版本重复下载
pub fn cache_dir() -> PathBuf {
    ensure_dir(&app_root(), "cache", "cache")
}

/// 日志目录：<app_root>/logs
pub fn logs_dir() -> PathBuf {
    ensure_dir(&app_root(), "logs", "logs")
}

/// 内置 zip 的相对路径（相对于 resource_dir）
/// 返回 "software/{key}/{version}.zip"
pub fn builtin_zip_relative(key: &str, version: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("software/{}/{}.zip", key, version))
}

/// 解析内置资源文件路径，跨平台兼容。
/// Windows 上 resource_dir() 返回 exe 目录，资源实际在 resources/ 子目录下；
/// macOS 上 resource_dir() 返回 .app/Contents/Resources/，资源直接在其下；
/// Linux 上路径形如 /usr/lib/<exe>/，资源在其中的 resources/ 或根目录。
/// 该函数尝试多个候选路径，返回第一个存在的。
pub fn resolve_builtin_resource(resource_dir: &Path, rel: &str) -> Option<PathBuf> {
    // 候选 1: resource_dir/resources/{rel}（Windows 打包后 + dev 模式）
    let p1 = resource_dir.join("resources").join(rel);
    if p1.exists() {
        return Some(p1);
    }
    // 候选 2: resource_dir/{rel}（macOS / Linux / 旧式布局）
    let p2 = resource_dir.join(rel);
    if p2.exists() {
        return Some(p2);
    }
    None
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

    #[test]
    fn builtin_zip_relative_returns_correct_path() {
        let p = builtin_zip_relative("jre", "17.0.15");
        assert_eq!(p.to_string_lossy(), "software/jre/17.0.15.zip");

        let p2 = builtin_zip_relative("mysql", "8.4.0");
        assert_eq!(p2.to_string_lossy(), "software/mysql/8.4.0.zip");

        let p3 = builtin_zip_relative("nginx", "1.31.2");
        assert_eq!(p3.to_string_lossy(), "software/nginx/1.31.2.zip");
    }

    #[test]
    fn resolve_builtin_resource_finds_under_resources_subdir() {
        // Windows 风格：资源在 resource_dir/resources/software/ 下
        let tmp = std::env::temp_dir().join(format!(
            "opx_paths_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let sw_dir = tmp.join("resources").join("software").join("mysql");
        fs::create_dir_all(&sw_dir).unwrap();
        let zip = sw_dir.join("8.4.10.zip");
        fs::write(&zip, b"test").unwrap();

        let result = resolve_builtin_resource(&tmp, "software/mysql/8.4.10.zip");
        assert!(result.is_some(), "应能在 resources/ 子目录下找到资源");
        assert_eq!(result.unwrap(), zip);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn resolve_builtin_resource_finds_under_root() {
        // macOS 风格：资源直接在 resource_dir 下
        let tmp = std::env::temp_dir().join(format!(
            "opx_paths_test2_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let sw_dir = tmp.join("software").join("nginx");
        fs::create_dir_all(&sw_dir).unwrap();
        let zip = sw_dir.join("1.31.2.zip");
        fs::write(&zip, b"test").unwrap();

        let result = resolve_builtin_resource(&tmp, "software/nginx/1.31.2.zip");
        assert!(result.is_some(), "应能在根目录下找到资源");
        assert_eq!(result.unwrap(), zip);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn resolve_builtin_resource_returns_none_when_missing() {
        let tmp = std::env::temp_dir().join(format!(
            "opx_paths_test3_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&tmp).unwrap();
        let result = resolve_builtin_resource(&tmp, "software/nonexistent/1.0.zip");
        assert!(result.is_none(), "资源不存在时应返回 None");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn resolve_builtin_resource_prefers_resources_subdir() {
        // 两个候选都存在时优先 resources/ 子目录
        let tmp = std::env::temp_dir().join(format!(
            "opx_paths_test4_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let in_resources = tmp.join("resources").join("software").join("jre");
        fs::create_dir_all(&in_resources).unwrap();
        fs::write(in_resources.join("1.8.zip"), b"in-resources").unwrap();

        let in_root = tmp.join("software").join("jre");
        fs::create_dir_all(&in_root).unwrap();
        fs::write(in_root.join("1.8.zip"), b"in-root").unwrap();

        let result = resolve_builtin_resource(&tmp, "software/jre/1.8.zip");
        assert!(result.is_some());
        assert!(result.unwrap().starts_with(tmp.join("resources")));

        let _ = fs::remove_dir_all(&tmp);
    }
}
