//! 本机「真实」IPv4 探测。
//!
//! 用于 Nacos / Spring Boot 等依赖 Spring Cloud `InetUtils` 自探测 IP 的软件：
//! 它们默认取「第一个非回环网卡」作为本机 IP，在装有 VMware / VirtualBox / Hyper-V
//! 的机器上会误取虚拟网卡 IP（如 `192.168.200.1`），导致 gRPC / 服务发现地址不可达。
//!
//! 本模块提供：
//! - [`preferred_local_ip`]：返回真实物理网卡 IPv4（剔除虚拟/回环/链路本地）。
//! - [`IGNORED_INTERFACES`]：Spring Cloud `InetUtils` 的 `ignored-interfaces` 正则，
//!   用于直接让框架跳过虚拟网卡做自探测。

/// 需排除的虚拟/隧道网卡名称关键字（大小写不敏感，子串匹配）。
///
/// 跨平台共用：`ipconfig` 回退解析、Nacos / Spring Boot 广播 IP 的
/// `ignored-interfaces`、以及「服务默认监听地址」的排序都引用这一份，
/// 避免各模块各养一套关键词、改一处漏一处。
const VIRTUAL_ADAPTER_HINTS: &[&str] = &[
    // 虚拟机平台
    "vmware",
    "vmnet",
    "virtualbox",
    "vboxnet",
    "virtual",
    "hyper-v",
    "hyperv",
    "vethernet",
    // 容器 / 编排
    "docker",
    "br-",
    "veth",
    "virbr",
    "cni",
    "flannel",
    "kube",
    "wsl",
    // 隧道 / VPN / 抓包
    "tap",
    "tun",
    "tailscale",
    "wireguard",
    "zerotier",
    "teredo",
    "isatap",
    "6to4",
    "npcap",
    "loopback",
    // 其它非对外的适配器
    "bluetooth",
    "蓝牙",
    "vpn",
];

/// 网卡名前缀（WireGuard `wg0`、ZeroTier `zt…` 的完整名不固定）。
const VIRTUAL_ADAPTER_PREFIXES: &[&str] = &["wg", "zt"];

/// 返回本机真实 IPv4（不含回环 `127.*` / 链路本地 `169.254.*` / 虚拟网卡）。
///
/// 探测策略（按优先级）：
/// 1. UDP connect 到公网地址（不发包，仅查路由表）→ 取出口网卡地址。
///    该地址天然排除虚拟网卡（虚拟网卡不在默认路由上），是最可靠的方式。
/// 2. 回退：Windows 解析 `ipconfig`，过滤虚拟/回环/链路本地网卡后取首个 IPv4。
///
/// 都不成功返回 `None`，调用方应降级为「不注入 IP 参数」（维持软件默认行为）。
pub fn preferred_local_ip() -> Option<String> {
    if let Some(ip) = via_udp_route() {
        return Some(ip);
    }
    fallback_ipconfig()
}

/// 策略 1：UDP connect 探测出口网卡地址（跨平台，不实际发包）。
fn via_udp_route() -> Option<String> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    // 连到公网地址仅在本地路由表查找出口，不会发送任何数据包
    sock.connect("8.8.8.8:443").ok()?;
    let addr = sock.local_addr().ok()?;
    match addr.ip() {
        std::net::IpAddr::V4(v4)
            if !v4.is_loopback() && !v4.is_link_local() && !v4.is_unspecified() =>
        {
            Some(v4.to_string())
        }
        _ => None,
    }
}

/// 策略 2：Windows `ipconfig` 解析（仅 Windows 回退）。
#[cfg(windows)]
fn fallback_ipconfig() -> Option<String> {
    let Ok(out) = std::process::Command::new("ipconfig").output() else {
        return None;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut current_adapter = String::new();
    let mut candidates: Vec<String> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        // 适配器标题行：以 ":" 结尾且含 "adapter"，如 "Ethernet adapter Ethernet:"
        if let Some(rest) = trimmed.strip_suffix(':') {
            if rest.contains("adapter") {
                current_adapter = rest.to_lowercase();
                continue;
            }
        }
        // IPv4 行（中英文 Windows 均含 "IPv4" 字样）
        if trimmed.contains("IPv4") {
            if let Some(ip) = extract_ipv4(trimmed) {
                if is_virtual_adapter(&current_adapter) {
                    continue;
                }
                if ip.starts_with("127.") || ip.starts_with("169.254.") {
                    continue;
                }
                candidates.push(ip);
            }
        }
    }
    candidates.into_iter().next()
}

#[cfg(not(windows))]
fn fallback_ipconfig() -> Option<String> {
    None
}

/// 从一行中提取首个 IPv4 地址（兼容中英文行尾标点）。
#[cfg(windows)]
fn extract_ipv4(line: &str) -> Option<String> {
    line.split_whitespace()
        .find(|tok| tok.trim_end_matches(',').parse::<std::net::Ipv4Addr>().is_ok())
        .map(|tok| tok.trim_end_matches(',').to_string())
}

/// 判断网卡名是否命中虚拟/隧道网卡关键字（跨平台，纯字符串判定）。
///
/// 公开给「服务默认监听地址」排序用：多网卡机器上虚拟网卡的私有 IPv4 与真实
/// 以太网同级，必须降级才不会把默认值选到 VMware / WSL / Docker 上。
pub fn is_virtual_adapter(name: &str) -> bool {
    let n = name.to_lowercase();
    VIRTUAL_ADAPTER_HINTS.iter().any(|h| n.contains(h))
        || VIRTUAL_ADAPTER_PREFIXES.iter().any(|p| n.starts_with(p))
}

/// Spring Cloud `InetUtils` 的 `ignored-interfaces` 正则（默认剔除虚拟网卡）。
pub const IGNORED_INTERFACES: &str = "VMware.*,VirtualBox.*,vEthernet.*,.*Hyper-V.*";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_adapter_names_are_detected() {
        // Windows
        assert!(is_virtual_adapter(
            "Ethernet adapter VMware Network Adapter VMnet8"
        ));
        assert!(is_virtual_adapter("vEthernet (WSL)"));
        assert!(is_virtual_adapter("Hyper-V Virtual Ethernet Adapter"));
        assert!(is_virtual_adapter("VirtualBox Host-Only Network"));
        // Linux / 容器 / 隧道
        assert!(is_virtual_adapter("docker0"));
        assert!(is_virtual_adapter("br-1a2b3c4d5e6f"));
        assert!(is_virtual_adapter("virbr0"));
        assert!(is_virtual_adapter("vboxnet0"));
        assert!(is_virtual_adapter("tailscale0"));
        assert!(is_virtual_adapter("wg0"));
        assert!(is_virtual_adapter("ztabcdef"));
        // 真实网卡不能被误杀
        assert!(!is_virtual_adapter("Ethernet adapter Ethernet"));
        assert!(!is_virtual_adapter("以太网"));
        assert!(!is_virtual_adapter("以太网 2"));
        assert!(!is_virtual_adapter("Wi-Fi"));
        assert!(!is_virtual_adapter("wlp2s0"));
        assert!(!is_virtual_adapter("enp3s0"));
        assert!(!is_virtual_adapter("ens33"));
        assert!(!is_virtual_adapter("eth0"));
    }

    #[cfg(windows)]
    #[test]
    fn extracts_ipv4_from_line() {
        assert_eq!(
            extract_ipv4("IPv4 Address. . . . . . . . . . . : 192.168.1.5").as_deref(),
            Some("192.168.1.5")
        );
        assert_eq!(
            extract_ipv4("   IPv4 地址 . . . . . . . . . . . . : 10.0.0.2")
                .as_deref(),
            Some("10.0.0.2")
        );
    }

    #[test]
    fn preferred_local_ip_does_not_panic() {
        // 不崩溃即可；CI / 无网络环境可能返回 None
        let _ = preferred_local_ip();
    }
}
