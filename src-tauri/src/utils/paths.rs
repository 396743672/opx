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

/// 解析软件安装路径：若为相对路径则拼接 apps_dir()，否则直接返回（兼容旧版绝对路径）。
/// 存储时存 `{key}/{version}` 或 `custom/{name}`，运行时解析为完整路径。
pub fn resolve_install_path(rel_or_abs: &str) -> PathBuf {
    let p = Path::new(rel_or_abs);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        apps_dir().join(p)
    }
}

/// 解析应用数据路径：若为相对路径则拼接 data_dir()，否则直接返回。
/// 用于 SpringBoot 的 jar_path/log_path 等数据目录下的路径。
pub fn resolve_data_path(rel_or_abs: &str) -> PathBuf {
    let p = Path::new(rel_or_abs);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        data_dir().join(p)
    }
}
