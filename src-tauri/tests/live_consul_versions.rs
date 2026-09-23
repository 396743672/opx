//! 一次性实机验证：Consul 的**动态版本发现**确实能从 HashiCorp 官方 API 拿到可安装版本。
//!
//! 不进常规测试（用 `--ignored` 显式跑），因为需要真实联网：
//!
//! ```text
//! cargo test --test live_consul_versions -- --ignored --nocapture
//! ```
//!
//! 为什么要这条实机测试：单测只覆盖「合成 JSON 的过滤规则」，而这里要证明两件事
//! 只有真联网才能证伪的事实——
//! 1. `api.releases.hashicorp.com` 的响应结构仍与解析器假设一致（版本号字段 / builds 数组）；
//! 2. `releases.hashicorp.com` 的 `consul_<ver>_SHA256SUMS` 仍可取到并解析出 64 位哈希。
//!
//! 断网环境下用 `--ignored` 才会跑，因此不会影响 CI（CI 不跑 ignored 测试）。

use opx_lib::services::software_manager::providers::consul::ConsulProvider;
use opx_lib::services::software_manager::providers::SoftwareProvider;

#[test]
#[ignore = "需要真实联网访问 HashiCorp 官方 API"]
fn consul_remote_versions_are_installable() {
    let versions = ConsulProvider::new()
        .fetch_remote_versions()
        .expect("应从 HashiCorp releases API 拿到社区版版本列表");

    assert!(!versions.is_empty(), "版本列表不应为空");
    for v in &versions {
        // 企业版 / 预发布必须被过滤掉，否则会下到 +ent 包而启动失败
        assert!(!v.version.contains('+'), "不应包含企业版: {}", v.version);
        assert!(!v.version.contains('-'), "不应包含预发布: {}", v.version);

        assert_eq!(v.mirrors.len(), 1, "每个版本应只有官方一个源");
        let url = &v.mirrors[0].url;
        assert!(
            url.starts_with("https://releases.hashicorp.com/consul/"),
            "非官方源: {}",
            url
        );
        assert!(
            url.ends_with(&format!(
                "consul_{}_windows_amd64.zip",
                v.version
            )),
            "URL 与版本不匹配: {}",
            url
        );
        let sha = v.archive.sha256.as_deref().unwrap_or("");
        assert_eq!(sha.len(), 64, "sha256 应可取到: {:?}", v.archive.sha256);
        assert!(sha.chars().all(|c| c.is_ascii_hexdigit()), "sha256 非法: {}", sha);
    }

    println!(
        "发现 {} 个可安装版本: {}",
        versions.len(),
        versions
            .iter()
            .map(|v| v.version.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
}
