//! 一次性实机验证：新增动态版本发现的 4 个 provider（mongodb / postgresql / rustfs / influxdb3）
//! 确实能从各自官方源拿到**可安装**的版本。
//!
//! 不进常规测试（用 `--ignored` 显式跑），因为需要真实联网：
//!
//! ```text
//! cargo test --test live_remote_versions -- --ignored --nocapture
//! ```
//!
//! 为什么需要这条实机测试：单测只覆盖「合成 JSON 的过滤规则」，而这里要证伪的是
//! 只有真联网才能发现的事实——上游响应结构是否仍与解析器假设一致（字段名、tag 规范、
//! 平台/版本过滤条件）。上游改结构时，单测依然全绿，只有这条会红。
//!
//! 断言口径刻意宽松：只要求「拿到非空且包 URL 与版本号自洽」；
//! 具体有几个版本、是哪些版本随上游发布变化，不写死（否则每次上游发版 CI 都要改测试）。

use opx_lib::services::software_manager::providers::SoftwareProvider;
use opx_lib::services::software_manager::providers::mongodb::MongoDbProvider;
use opx_lib::services::software_manager::providers::postgresql::PostgreSqlProvider;
use opx_lib::services::software_manager::providers::rustfs::RustfsProvider;
use opx_lib::services::software_manager::providers::influxdb3::Influxdb3Provider;

/// 通用校验：每个版本必须只有官方一个源，且 URL 里出现该版本号（防止「版本与包错配」）。
fn assert_versions_self_consistent(label: &str, versions: &[opx_lib::models::software::CatalogVersion]) {
    assert!(!versions.is_empty(), "{} 版本列表不应为空", label);
    for v in versions {
        assert_eq!(v.mirrors.len(), 1, "{} 每个版本应只有官方一个源", label);
        assert!(
            v.mirrors[0].url.contains(&v.version),
            "{} URL 与版本号不匹配: {} vs {}",
            label,
            v.mirrors[0].url,
            v.version
        );
    }
    println!(
        "{} 发现 {} 个版本: {}",
        label,
        versions.len(),
        versions
            .iter()
            .map(|v| v.version.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

#[test]
#[ignore = "需要真实联网访问 MongoDB 官方版本清单"]
fn mongodb_remote_versions_are_installable() {
    let versions = MongoDbProvider::new()
        .fetch_remote_versions()
        .expect("应从 downloads.mongodb.org/current.json 拿到版本列表");
    assert_versions_self_consistent("mongodb", &versions);
    for v in &versions {
        assert!(
            v.mirrors[0].url.starts_with("https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-"),
            "应是 Windows x86_64 社区版包: {}",
            v.mirrors[0].url
        );
        assert!(!v.mirrors[0].url.contains("-enterprise-"), "不应是企业版: {}", v.mirrors[0].url);
        // current.json 自带 sha256，取不到说明字段名变了
        let sha = v.archive.sha256.as_deref().unwrap_or("");
        assert_eq!(sha.len(), 64, "sha256 应可取到: {:?}", v.archive.sha256);
        assert!(sha.chars().all(|c| c.is_ascii_hexdigit()), "sha256 非法: {}", sha);
    }
}

#[test]
#[ignore = "需要真实联网访问 PostgreSQL 官方 Git 镜像标签 API"]
fn postgresql_remote_versions_are_installable() {
    let versions = PostgreSqlProvider::new()
        .fetch_remote_versions()
        .expect("应从 postgres/postgres 标签 API 拿到版本列表");
    assert_versions_self_consistent("postgresql", &versions);
    for v in &versions {
        assert!(
            v.mirrors[0].url.starts_with("https://get.enterprisedb.com/postgresql/postgresql-"),
            "应是 EDB 官方 Windows 包: {}",
            v.mirrors[0].url
        );
        // 预发布（beta / rc）绝不能被当成正式版推给用户
        assert!(
            !v.version.contains("beta" ) && !v.version.contains("rc") && !v.version.contains("alpha"),
            "不应包含预发布版本: {}",
            v.version
        );
    }
}

#[test]
#[ignore = "需要真实联网访问 RustFS GitHub Release"]
fn rustfs_remote_versions_are_stable_only() {
    // 上游现状：正式版只有 1.0.0，其后全是 1.0.1-preview.x 预发布。
    // 因此这里允许「拿到与内置相同的 1.0.0」，但**绝不能**出现 preview / rc / beta。
    let versions = RustfsProvider::new()
        .fetch_remote_versions()
        .expect("应从 rustfs/rustfs Release 拿到正式版列表");
    assert_versions_self_consistent("rustfs", &versions);
    for v in &versions {
        assert!(
            !v.version.contains("preview")
                && !v.version.contains("rc")
                && !v.version.contains("beta"),
            "不应把预发布当正式版推给用户: {}",
            v.version
        );
        assert!(
            !v.mirrors[0].url.contains("-latest.zip"),
            "不应使用会移动的 latest 资产: {}",
            v.mirrors[0].url
        );
        let sha = v.archive.sha256.as_deref().unwrap_or("");
        assert_eq!(sha.len(), 64, "sha256 应取自 GitHub 资产 digest: {:?}", v.archive.sha256);
    }
}

#[test]
#[ignore = "需要真实联网访问 InfluxDB GitHub Release"]
fn influxdb3_remote_versions_are_v3_only() {
    let versions = Influxdb3Provider::new()
        .fetch_remote_versions()
        .expect("应从 influxdata/influxdb Release 拿到 v3 版本列表");
    assert_versions_self_consistent("influxdb3", &versions);
    for v in &versions {
        assert!(v.version.starts_with("3."), "只应取 InfluxDB 3 系列: {}", v.version);
        assert!(
            v.mirrors[0].url.contains("influxdb3-core-"),
            "应是 influxdb3-core 包: {}",
            v.mirrors[0].url
        );
    }
}
