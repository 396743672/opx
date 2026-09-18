//! 一次性实机验证：live 环境下把 TXT 写进用户真实 DNS，跑完自动清理。
//!
//! 不进 CI（`cargo test` 默认不编译此文件），因为 `#[ignore]` 的测试仍会编译，
//! 而本文件引用了只在集成测试可见的构造函数。用显式命令跑：
//!
//! ```text
//! set OPX_LIVE_DNSPOD_ID=<SecretId>
//! set OPX_LIVE_DNSPOD_KEY=<SecretKey>
//! set OPX_LIVE_DOMAIN=lonest.cloud
//! set OPX_LIVE_ACCOUNT_ID=<dns-accounts.json 里的 id>
//! cargo test --test live_dns -- --ignored --nocapture
//! ```
//!
//! 要验证的是**运行时才成立**的三件事，单元测试覆盖不到：
//! 1. 出站真的没被环境变量代理劫持（同进程内 `env::set_var` 复刻宿主）
//! 2. 签名/请求头/端点组合能过腾讯云鉴权（失败典型：AuthFailure.SignatureFailure）
//! 3. 写-读-删闭环（记录名 `_acme-challenge.<domain>` 的拼装与清理）

use opx_lib::models::dns_account::DnsAccount;
use opx_lib::services::acme::dns::provider_for_account;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("缺少环境变量 {}", name))
}

#[tokio::test]
#[ignore = "实机：会向真实 DNS 写入并删除一条 TXT"]
async fn dnspod_real_write_read_delete() {
    // 复刻宿主环境：Claude Code 进程里设了 ALL_PROXY 指向不支持的 HTTP 代理。
    // 若 provider 仍走环境变量，这里会立刻死成 10061 —— 正是线上那个报错。
    std::env::set_var("ALL_PROXY", "http://127.0.0.1:50077");
    std::env::set_var("HTTPS_PROXY", "http://127.0.0.1:50077");

    let domain = env("OPX_LIVE_DOMAIN");
    let account = DnsAccount {
        id: env("OPX_LIVE_ACCOUNT_ID"),
        name: "live".into(),
        provider: "dnspod".into(),
        token: String::new(),
        access_key_id: env("OPX_LIVE_DNSPOD_ID"),
        access_key_secret: env("OPX_LIVE_DNSPOD_KEY"),
        zones: vec![],
        tested_at: None,
    };

    // 1) 工厂 + zone 列举（验读权限与鉴权链路）
    let provider = provider_for_account(&account).expect("凭证齐全应能构造 provider");
    let zones = provider.list_zones().await.expect("列 zone 失败（鉴权或出站问题）");
    println!("list_zones -> {:?}", zones);
    assert!(
        zones.iter().any(|z| domain == *z || domain.ends_with(&format!(".{z}"))),
        "账户里应包含 {} 所属 zone，实际 {:?}",
        domain,
        zones
    );

    // 2) 写 TXT 再读回（验写权限）
    let fqdn = format!("_acme-challenge.{}", domain);
    let value = format!("opx-live-probe-{}", std::process::id());
    let before = provider
        .get_value(&fqdn, "TXT")
        .await
        .expect("读 TXT 失败");
    println!("before = {:?}", before);

    provider
        .set_value(&fqdn, "TXT", &value)
        .await
        .expect("写 TXT 失败（DNSPod 写权限或签名问题）");
    let after = provider
        .get_value(&fqdn, "TXT")
        .await
        .expect("写完再读失败");
    println!("after = {:?}", after);
    assert_eq!(after.as_deref(), Some(value.as_str()), "写进去的值应能读回");

    // 3) 清理：原本没有就删掉，原本有旧值就还原
    match &before {
        Some(old) => provider
            .set_value(&fqdn, "TXT", old)
            .await
            .expect("还原旧 TXT 失败"),
        None => provider
            .delete_value(&fqdn, "TXT")
            .await
            .expect("删除探测 TXT 失败"),
    }
    let final_state = provider.get_value(&fqdn, "TXT").await.expect("收尾读失败");
    println!("final = {:?}", final_state);
    assert_eq!(
        final_state.as_deref(),
        before.as_deref(),
        "清理后应与探测前一致（无残留）"
    );
}