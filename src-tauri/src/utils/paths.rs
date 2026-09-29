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

/// 落盘敏感文件后收紧权限，缓解「权限不当可被读」（P2-6 明文凭据持久化）。
///
/// - Unix/macOS：设为 `0600`（仅 owner 可读写，group/other 无任何权限）。
/// - Windows：best-effort 设为只读位。NTFS 默认 ACL 已限制其他用户读取，
///   真正的 owner-only ACL 需调用 Windows API，留待跨平台（P2-1）统一处理。
///
/// 失败不致命，仅静默忽略——权限收紧是加固项，不应阻断主流程。
pub fn restrict_file_permissions(path: &Path) {
    let _ = restrict_file_permissions_inner(path);
}

#[cfg(unix)]
fn restrict_file_permissions_inner(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms)
}

#[cfg(windows)]
fn restrict_file_permissions_inner(path: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileAttributesW, SetFileAttributesW, FILE_ATTRIBUTE_READONLY,
    };
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // best-effort：保留其他属性位，仅置只读位（防误改）。
    // 真正 owner-only ACL（防同用户读）需 windows-sys DACL，留待 P2-1 跨平台统一处理。
    let attrs = unsafe { GetFileAttributesW(wide.as_ptr()) };
    let target = if attrs == u32::MAX {
        FILE_ATTRIBUTE_READONLY
    } else {
        attrs | FILE_ATTRIBUTE_READONLY
    };
    let _ = unsafe { SetFileAttributesW(wide.as_ptr(), target) };
    Ok(())
}

/// 写敏感文件并「落盘即收紧权限」，且**支持重写**（修复 P2-6 只读位阻断覆盖写）。
///
/// 顺序（关键）：
/// 1. 写前清除只读位（仅 Windows 需要；NTFS 只读属性会让 `std::fs::write` 覆盖既有文件失败）。
/// 2. 写入内容。
/// 3. 收紧权限（Unix 0600 / Windows 只读位）。
///
/// 这样首次写入与重启重写走同一落点，避免 P2-6 的只读位把 `installed.json`、
/// influxdb3 `admin-token.json` 等**会被反复重写**的文件锁死为不可写。
pub fn write_file_restricted<C: AsRef<[u8]>>(path: &Path, content: C) -> std::io::Result<()> {
    clear_readonly_if_windows(path);
    std::fs::write(path, content)?;
    restrict_file_permissions(path);
    Ok(())
}

#[cfg(windows)]
fn clear_readonly_if_windows(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileAttributesW, SetFileAttributesW, FILE_ATTRIBUTE_READONLY,
    };
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let attrs = unsafe { GetFileAttributesW(wide.as_ptr()) };
    // 仅当文件已存在且带只读位时才清除；文件不存在（首次创建）时 GetFileAttributesW 返回 MAX，跳过。
    if attrs != u32::MAX && attrs & FILE_ATTRIBUTE_READONLY != 0 {
        let _ = unsafe { SetFileAttributesW(wide.as_ptr(), attrs & !FILE_ATTRIBUTE_READONLY) };
    }
}

#[cfg(not(windows))]
fn clear_readonly_if_windows(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn restrict_file_permissions_tightens_access() {
        let dir = std::env::temp_dir().join(format!("opx-perm-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let p = dir.join("secret.txt");
        {
            let mut f = fs::File::create(&p).unwrap();
            f.write_all(b"secret").unwrap();
        }
        // 先放宽，确保测试前是宽松权限
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = fs::metadata(&p).unwrap().permissions();
            perm.set_mode(0o644);
            fs::set_permissions(&p, perm).unwrap();
        }
        restrict_file_permissions(&p);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&p).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "group/other 不应有任何权限");
        }
        #[cfg(windows)]
        {
            assert!(
                fs::metadata(&p).unwrap().permissions().readonly(),
                "Windows 上应设为只读"
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_file_restricted_allows_rewrite() {
        // 复现 P2-6 只读位阻断重写：首次写后收紧权限，第二次写（如 influxdb3 每次启动重写 token）应成功。
        let dir = std::env::temp_dir().join(format!("opx-perm-rewrite-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let p = dir.join("secret.txt");
        write_file_restricted(&p, b"first").unwrap();
        // 第二次写入不应因只读位失败（Windows 上无 clear_readonly 会 ACCESS_DENIED）。
        write_file_restricted(&p, b"second").unwrap();
        let content = fs::read_to_string(&p).unwrap();
        assert_eq!(content, "second");
        // 权限仍被收紧
        #[cfg(windows)]
        assert!(
            fs::metadata(&p).unwrap().permissions().readonly(),
            "重写后应为只读"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
