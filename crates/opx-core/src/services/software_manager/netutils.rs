//! Windows 端口采集：解析 `netstat -ano` 的 LISTENING 行 → (pid, port, addr)。
//!
//! 仅采集监听态；命令失败/权限不足时降级为空集，不阻塞上层。

use crate::utils::process::hidden;

/// 一条监听记录。pid 缺失（权限不足等情况）记 None。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ListenEntry {
    pub pid: Option<u32>,
    pub port: u16,
    pub addr: String,
}

/// 运行 `netstat -ano` 并解析监听记录。失败返回空集（降级不崩）。
pub fn listen_entries() -> Vec<ListenEntry> {
    // 隐藏控制台窗口：GUI 进程起 netstat 会闪黑窗（本函数在运行态被周期调用）
    let Ok(out) = hidden("netstat").arg("-ano").output() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    // 中文 Windows 的 netstat 输出为 OEM 代码页，但 LISTENING/地址/pid 均为 ASCII，lossy 足够
    parse_netstat_listening(&String::from_utf8_lossy(&out.stdout))
}

/// 解析 netstat 输出中的 LISTENING 行。
/// 形如 `  TCP    0.0.0.0:80      0.0.0.0:0       LISTENING       1234`。
/// 畸形行/非 TCP/非监听态跳过。
pub fn parse_netstat_listening(output: &str) -> Vec<ListenEntry> {
    let mut result = Vec::new();
    for line in output.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        // 期望：TCP <local> <foreign> LISTENING <pid>
        if cols.len() < 5 || cols[0] != "TCP" || cols[3] != "LISTENING" {
            continue;
        }
        let Some(port) = port_of(cols[1]) else { continue };
        result.push(ListenEntry {
            pid: cols[4].parse().ok(),
            port,
            addr: cols[1].to_string(),
        });
    }
    result
}

/// 从本地地址取端口：`0.0.0.0:80` / `[::]:80` / `127.0.0.1:5432` → 端口号。
fn port_of(addr: &str) -> Option<u16> {
    addr.rsplit(':').next()?.parse().ok()
}

// ===================== 实际监听地址 =====================
//
// 「服务绑在哪」只有内核知道：配置里写 `bind-address=127.0.0.1`、命令行传
// `--host 127.0.0.1`、或者代码里硬编码，最终都体现在监听套接字上。
// 只看配置文件推断会漏（MySQL 把 bind 写在 my.cnf、Consul 写在 schema 默认值……），
// 而「放行了防火墙却连不上」几乎都是这里出的问题。

/// 一条监听记录：端口 + 实际绑定的地址。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenBind {
    pub port: u16,
    /// 展示用地址：`0.0.0.0` / `127.0.0.1` / 具体 IP / `::`
    pub addr: String,
    /// true = 只绑回环，外部一律不可达（此时防火墙放行再对也没用）
    pub local_only: bool,
}

/// 采集当前全部 TCP 监听地址。失败降级为空集（不阻断端口页）。
#[cfg(target_os = "linux")]
pub fn listening_binds() -> Vec<ListenBind> {
    // 直接读 /proc/net/tcp*：无外部依赖（ss 在精简镜像里常缺），且不需要 root
    let mut out = Vec::new();
    for path in ["/proc/net/tcp", "/proc/net/tcp6"] {
        if let Ok(text) = std::fs::read_to_string(path) {
            out.extend(parse_proc_net_tcp(&text));
        }
    }
    out
}

/// Windows：复用 netstat 的 LISTENING 行，把 `addr:port` 拆成地址。
#[cfg(target_os = "windows")]
pub fn listening_binds() -> Vec<ListenBind> {
    listen_entries()
        .into_iter()
        .filter_map(|e| {
            let host = split_host(&e.addr)?;
            Some(ListenBind {
                port: e.port,
                local_only: host_is_loopback(&host),
                addr: host,
            })
        })
        .collect()
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn listening_binds() -> Vec<ListenBind> {
    Vec::new()
}

/// 从 `0.0.0.0:80` / `[::]:80` 里取主机部分。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn split_host(addr: &str) -> Option<String> {
    let (host, port) = addr.rsplit_once(':')?;
    if port.parse::<u16>().is_err() {
        return None;
    }
    Some(host.trim_matches(['[', ']']).to_string())
}

/// 地址是否只在本机可达。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn host_is_loopback(host: &str) -> bool {
    host == "::1" || host.starts_with("127.") || host.eq_ignore_ascii_case("localhost")
}

/// 解析 `/proc/net/tcp` 或 `/proc/net/tcp6`（两者格式一致，只有地址长度不同）。
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_proc_net_tcp(text: &str) -> Vec<ListenBind> {
    let mut out = Vec::new();
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        // 期望：sl local_address rem_address st ...；st 必须为 0A（LISTEN）
        if cols.len() < 4 || cols[3] != "0A" {
            continue;
        }
        let Some((ip_hex, port_hex)) = cols[1].split_once(':') else {
            continue;
        };
        let Ok(port) = u16::from_str_radix(port_hex, 16) else {
            continue;
        };
        let Some((addr, local_only)) = decode_proc_addr(ip_hex) else {
            continue;
        };
        out.push(ListenBind {
            port,
            addr,
            local_only,
        });
    }
    out
}

/// 把 `/proc/net/tcp` 的十六进制地址解成可读地址。
///
/// 坑：字节序是**按 32 位字小端**。`0100007F` = 127.0.0.1（不是 1.0.0.127），
/// `::1` 写作 `...01000000`。解错会得出一个看起来像公网 IP 的假地址。
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn decode_proc_addr(hex: &str) -> Option<(String, bool)> {
    match hex.len() {
        8 => {
            let mut bytes = [0u8; 4];
            for (i, b) in bytes.iter_mut().enumerate() {
                *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
            }
            bytes.reverse();
            let addr = format!("{}.{}.{}.{}", bytes[0], bytes[1], bytes[2], bytes[3]);
            let local = host_is_loopback(&addr);
            Some((addr, local))
        }
        32 => {
            // 全零 = ::（所有接口）
            if hex.chars().all(|c| c == '0') {
                return Some(("::".to_string(), false));
            }
            // ::1 = 前 24 位全零 + 最后一字 0x00000001（小端即 01000000）
            if hex[..24].chars().all(|c| c == '0') && &hex[24..] == "01000000" {
                return Some(("::1".to_string(), true));
            }
            // v4-mapped（::ffff:a.b.c.d）：取最后 8 位当 IPv4 解，Docker/Java 常这么绑
            if hex[16..20].eq_ignore_ascii_case("FFFF") {
                return decode_proc_addr(&hex[24..]);
            }
            // 其余 IPv6 只关心「不是回环」——具体地址对判定无用，归一成 ::
            Some(("::".to_string(), false))
        }
        _ => None,
    }
}

// ===================== 本机地址与「对外访问地址」 =====================
//
// 服务默认绑 `127.0.0.1` 时，防火墙放行也没用（外部一律不可达）。把「本机有哪些
// 地址」探测出来交给用户选，比让用户自己 `ip addr` 抄 IP 更不容易出错；选定值存
// 系统配置，所有新装服务的 bind 默认值都用它。

/// 本机一块网卡上的一个地址。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LocalAddress {
    pub address: String,
    pub interface: String,
    /// true = 回环（127.x / ::1）
    pub loopback: bool,
    pub ipv6: bool,
}

/// 采集本机所有网卡地址，按「可用性 → 网卡真实性」排序，回环垫底。
///
/// 排序理由：列表第一项会被前端当作「推荐值」。多网卡机器上 VMware /
/// VirtualBox / Hyper-V / Docker 的私有 IPv4 与真实以太网同级，若不把虚拟
/// 网卡降级，平局会按网卡名字节序比较（`VMware…` < `以太网`）而选错；同时
/// 有线以太网要排在 Wi-Fi 前面（「有以太网就用以太网」）。
pub fn local_addresses() -> Vec<LocalAddress> {
    let mut out = Vec::new();
    let networks = sysinfo::Networks::new_with_refreshed_list();
    for (name, data) in &networks {
        for ip_net in data.ip_networks() {
            let addr = ip_net.addr;
            // 链路本地（fe80::）需要 scope id 才能用，绑上去必失败，直接排除
            if let std::net::IpAddr::V6(v6) = addr {
                if (v6.segments()[0] & 0xffc0) == 0xfe80 {
                    continue;
                }
            }
            out.push(LocalAddress {
                address: addr.to_string(),
                interface: name.to_string(),
                loopback: addr.is_loopback(),
                ipv6: addr.is_ipv6(),
            });
        }
    }
    // 去重（同一地址可能挂在多块网卡上）
    out.sort_by(|a, b| {
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.interface.cmp(&b.interface))
            .then_with(|| a.address.cmp(&b.address))
    });
    out.dedup_by(|a, b| a.address == b.address);
    out
}

/// 排序权重：先按「地址能不能被外部连上」，再按「网卡是不是真的」。
///
/// 两级而非单一分值，是为了让「虚拟网卡的可用 IPv4」仍排在「真实网卡的
/// IPv6 / 自分配地址」之前 —— 前者至少能用，后者绑上去大概率不通。
///
/// - 地址级：局域网/公网 IPv4 = 0，其它 IPv4 = 1，IPv6 = 2，链路本地 = 3
/// - 网卡级：有线以太网 = 0，其它物理网卡（Wi-Fi / 未知）= 1，虚拟网卡 = 2
/// - 回环：`(9, 9)` 永远垫底
fn rank(a: &LocalAddress) -> (u8, u8) {
    if a.loopback {
        return (9, 9);
    }
    (addr_rank(a), link_rank(&a.interface))
}

fn addr_rank(a: &LocalAddress) -> u8 {
    if a.ipv6 {
        2
    } else if is_link_local_v4(&a.address) {
        3
    } else if is_private_v4(&a.address) {
        0
    } else {
        1
    }
}

fn link_rank(interface: &str) -> u8 {
    if crate::utils::local_ip::is_virtual_adapter(interface) {
        2
    } else if is_ethernet_iface(interface) {
        0
    } else {
        1
    }
}

/// 有线以太网。优先它是因为「有以太网就用以太网」的直觉：有线通常更稳定，
/// 服务器场景下也才是真正对外的链路。
///
/// 虚拟网卡的关键词判定不在这里 —— 统一走
/// [`crate::utils::local_ip::is_virtual_adapter`]，避免两套规则各改一半。
fn is_ethernet_iface(interface: &str) -> bool {
    let n = interface.to_ascii_lowercase();
    const MARKERS: &[&str] = &["以太网", "ethernet", "eth", "enp", "eno", "ens", "enx"];
    MARKERS.iter().any(|m| n.contains(m))
}

fn is_private_v4(addr: &str) -> bool {
    let Ok(ip) = addr.parse::<std::net::Ipv4Addr>() else {
        return false;
    };
    ip.is_private()
}

fn is_link_local_v4(addr: &str) -> bool {
    addr.parse::<std::net::Ipv4Addr>()
        .map(|ip| ip.is_link_local())
        .unwrap_or(false)
}

/// 从地址列表里挑「对外访问」首选值：回环排除，按 [`rank`] 取最小。
///
/// 不依赖入参顺序，纯函数便于测试本机多网卡/多地址时的取舍。
pub fn pick_preferred(addrs: &[LocalAddress]) -> Option<String> {
    addrs
        .iter()
        .filter(|a| !a.loopback)
        .min_by(|a, b| rank(a).cmp(&rank(b)))
        .map(|a| a.address.clone())
}

/// 本机首选对外地址（有线以太网的局域网 IPv4 优先，没网卡时返回 None）。
pub fn preferred_local_address() -> Option<String> {
    pick_preferred(&local_addresses())
}

/// 系统配置里的「对外访问地址」；未配置（空/auto）时取本机首选地址。
///
/// 兜底顺序：用户配置 → 本机首选非回环地址 → `0.0.0.0`。
/// 最后兜底不用 `127.0.0.1`：机器完全没有非回环地址时（离线容器），
/// 绑 `0.0.0.0` 与绑回环对「外部不可达」这件事没有区别，但前者不会让本机
/// 其它网卡起来后还要再改一次配置。
pub fn default_service_bind() -> String {
    let configured = crate::utils::settings::read_settings()
        .map(|s| s.service_bind_address.trim().to_string())
        .unwrap_or_default();
    if !configured.is_empty() && !configured.eq_ignore_ascii_case("auto") {
        return configured;
    }
    preferred_local_address().unwrap_or_else(|| "0.0.0.0".to_string())
}

/// 由「服务监听地址」推导健康检查 / 客户端应连的地址。
///
/// - `0.0.0.0` / `::` / `*` / 空 → 走回环 `127.0.0.1`（通配地址不是可连地址）
/// - 具体地址 → 原样使用（服务只绑该网卡时，回环是连不上的）
/// - Redis 的 `bind 127.0.0.1 -::1` 形式带 `-` 前缀（表示「可选」），需剥掉；
///   多个地址按空格分隔时取第一个可连的。
pub fn probe_host_for_bind(bind: &str) -> String {
    for token in bind.split_whitespace() {
        let t = token.trim().trim_start_matches('-');
        if t.is_empty() {
            continue;
        }
        let is_wildcard = matches!(t, "*" | "::*")
            || t.parse::<std::net::IpAddr>()
                .map(|ip| ip.is_unspecified())
                .unwrap_or(false);
        if is_wildcard {
            return "127.0.0.1".to_string();
        }
        if t.eq_ignore_ascii_case("localhost") {
            return "127.0.0.1".to_string();
        }
        if t.parse::<std::net::IpAddr>().is_ok() {
            return t.to_string();
        }
        // 主机名等非常规值：交给连接层解析
        return t.to_string();
    }
    "127.0.0.1".to_string()
}

/// 各 provider 表示监听地址的字段名（与端口页 `BIND_KEYS` 保持一致）。
const BIND_KEYS: [&str; 6] = [
    "bind",
    "bind_address",
    "bind-address",
    "bind_ip",
    "listen_host",
    "host",
];

/// 从实例 config 里取监听地址字段（首个非空值）。
pub fn bind_from_config(config: &serde_json::Value) -> Option<String> {
    BIND_KEYS.iter().find_map(|key| {
        config
            .get(*key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_listening_skips_malformed() {
        let sample = "\
活动连接

  协议  本地地址          外部地址        状态           PID
  TCP    0.0.0.0:80             0.0.0.0:0              LISTENING       1234
  TCP    [::]:443               [::]:0                 LISTENING       1234
  TCP    127.0.0.1:5432         0.0.0.0:0              LISTENING       5678
  TCP    192.168.1.5:51000      93.184.216.34:443      ESTABLISHED     9999
  UDP    0.0.0.0:53             *:*                                    4321
  TCP    0.0.0.0:8080           0.0.0.0:0              LISTENING       notapid
  垃圾行
";
        let got = parse_netstat_listening(sample);
        assert_eq!(got.len(), 4);
        assert_eq!(got[0], ListenEntry { pid: Some(1234), port: 80, addr: "0.0.0.0:80".into() });
        assert_eq!(got[1], ListenEntry { pid: Some(1234), port: 443, addr: "[::]:443".into() });
        assert_eq!(got[2], ListenEntry { pid: Some(5678), port: 5432, addr: "127.0.0.1:5432".into() });
        // pid 解析失败降级为 None，端口仍保留
        assert_eq!(got[3], ListenEntry { pid: None, port: 8080, addr: "0.0.0.0:8080".into() });
    }

    #[test]
    fn empty_or_garbage_returns_empty() {
        assert!(parse_netstat_listening("").is_empty());
        assert!(parse_netstat_listening("hello world").is_empty());
    }

    /// /proc/net/tcp 的地址是「32 位字小端」，解错的后果是把 127.0.0.1 认成
    /// 1.0.0.127（看起来像公网 IP）→ 页面不再提示「仅监听本机」→ 用户永远连不上。
    #[test]
    fn decodes_proc_net_tcp_addresses() {
        assert_eq!(decode_proc_addr("0100007F"), Some(("127.0.0.1".to_string(), true)));
        assert_eq!(decode_proc_addr("00000000"), Some(("0.0.0.0".to_string(), false)));
        // 192.168.200.128 → C0 A8 C8 80
        assert_eq!(decode_proc_addr("80C8A8C0"), Some(("192.168.200.128".to_string(), false)));
        // :: 与 ::1
        assert_eq!(
            decode_proc_addr("00000000000000000000000000000000"),
            Some(("::".to_string(), false))
        );
        assert_eq!(
            decode_proc_addr("00000000000000000000000001000000"),
            Some(("::1".to_string(), true))
        );
        // v4-mapped ::ffff:127.0.0.1 应还原成回环
        assert_eq!(
            decode_proc_addr("0000000000000000FFFF00000100007F"),
            Some(("127.0.0.1".to_string(), true))
        );
        assert_eq!(decode_proc_addr("zz"), None);
    }

    #[test]
    fn parses_proc_net_tcp_listen_rows() {
        let sample = "\
  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 12345 1
   1: 0100007F:0CEA 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 23456 1
   2: 80C8A8C0:0050 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 34567 1
   3: 0100007F:1F90 0100007F:D4C2 01 00000000:00000000 00:00000000 00000000     0        0 45678 1
";
        let got = parse_proc_net_tcp(sample);
        // 第 4 行是 ESTABLISHED（01），不能算监听
        assert_eq!(
            got,
            vec![
                ListenBind { port: 22, addr: "0.0.0.0".into(), local_only: false },
                ListenBind { port: 3306, addr: "127.0.0.1".into(), local_only: true },
                ListenBind { port: 80, addr: "192.168.200.128".into(), local_only: false },
            ]
        );
    }

    #[test]
    fn splits_netstat_host_port() {
        assert_eq!(split_host("0.0.0.0:80").as_deref(), Some("0.0.0.0"));
        assert_eq!(split_host("[::]:443").as_deref(), Some("::"));
        assert_eq!(split_host("[::1]:17580").as_deref(), Some("::1"));
        assert_eq!(split_host("127.0.0.1:3306").as_deref(), Some("127.0.0.1"));
        assert_eq!(split_host("nonsense"), None);
        assert!(host_is_loopback("::1"));
        assert!(host_is_loopback("127.0.0.1"));
        assert!(!host_is_loopback("0.0.0.0"));
    }

    /// 绑到具体网卡地址时，健康检查必须探那个地址：探 127.0.0.1 会连接被拒，
    /// 于是服务明明已起来却被判「启动失败」（实测 MySQL/Redis 绑 192.168.200.128）。
    #[test]
    fn probe_host_follows_bind() {
        assert_eq!(probe_host_for_bind("192.168.200.128"), "192.168.200.128");
        assert_eq!(probe_host_for_bind("0.0.0.0"), "127.0.0.1");
        assert_eq!(probe_host_for_bind("::"), "127.0.0.1");
        assert_eq!(probe_host_for_bind("*"), "127.0.0.1");
        assert_eq!(probe_host_for_bind(""), "127.0.0.1");
        assert_eq!(probe_host_for_bind("localhost"), "127.0.0.1");
        // Redis 默认配置 `bind 127.0.0.1 -::1`：`-` 前缀表示「可选」，不能当主机名
        assert_eq!(probe_host_for_bind("127.0.0.1 -::1"), "127.0.0.1");
        assert_eq!(probe_host_for_bind("-::1"), "::1");
        assert_eq!(probe_host_for_bind("10.0.0.5 -::1"), "10.0.0.5");
    }

    #[test]
    fn bind_from_config_reads_first_known_key() {
        assert_eq!(
            bind_from_config(&serde_json::json!({ "bind": "0.0.0.0" })).as_deref(),
            Some("0.0.0.0")
        );
        assert_eq!(
            bind_from_config(&serde_json::json!({ "bind": " ", "bind-address": "192.168.1.9" }))
                .as_deref(),
            Some("192.168.1.9")
        );
        assert_eq!(bind_from_config(&serde_json::json!({})), None);
    }

    /// 局域网 IPv4 必须排在 Docker/VPN 地址与回环之前，否则前端默认值是错的。
    #[test]
    fn local_address_rank_prefers_lan_v4() {
        let mk = |address: &str, loopback: bool, ipv6: bool| LocalAddress {
            address: address.to_string(),
            interface: "eth0".to_string(),
            loopback,
            ipv6,
        };
        let mut list = vec![
            mk("127.0.0.1", true, false),
            mk("::1", true, true),
            mk("203.0.113.9", false, false),
            mk("192.168.200.128", false, false),
        ];
        list.sort_by(|a, b| rank(a).cmp(&rank(b)));
        assert_eq!(list[0].address, "192.168.200.128");
        assert_eq!(list[1].address, "203.0.113.9");
        assert_eq!(list.last().unwrap().address, "::1");
    }

    fn la(address: &str, iface: &str) -> LocalAddress {
        LocalAddress {
            address: address.to_string(),
            interface: iface.to_string(),
            loopback: address.starts_with("127.") || address == "::1",
            ipv6: address.contains(':'),
        }
    }

    /// 有线以太网判定。虚拟网卡的关键词表统一在 `utils::local_ip`，
    /// 其识别细节由那边的测试覆盖，这里只钉住「以太网 vs Wi-Fi」这条线。
    #[test]
    fn classifies_ethernet_interfaces() {
        for n in ["以太网", "Ethernet", "以太网 2", "eth0", "enp3s0", "eno1", "ens33"] {
            assert!(is_ethernet_iface(n), "{n} 应判为有线以太网");
        }
        for n in ["Wi-Fi", "WLAN", "wlp2s0", "wlan0", "en0"] {
            assert!(!is_ethernet_iface(n), "{n} 不是有线以太网");
        }
    }

    /// 真实场景：VMnet8 / WSL / 以太网 都是私有 IPv4，默认必须选以太网。
    #[test]
    fn prefers_physical_ethernet_over_virtual_nics() {
        let addrs = vec![
            la("192.168.23.1", "VMware Network Adapter VMnet8"),
            la("172.28.96.1", "vEthernet (WSL (Hyper-V firewall))"),
            la("192.168.0.7", "以太网"),
        ];
        assert_eq!(pick_preferred(&addrs).as_deref(), Some("192.168.0.7"));
    }

    /// 网线插着但没拿到 DHCP：以太网只剩 169.254 自分配地址，应让位给
    /// Wi-Fi 的可用地址，否则绑上去外部一样连不通。
    #[test]
    fn prefers_usable_address_over_link_local() {
        let addrs = vec![la("169.254.10.20", "以太网"), la("192.168.1.42", "Wi-Fi")];
        assert_eq!(pick_preferred(&addrs).as_deref(), Some("192.168.1.42"));
    }

    /// 没有以太网时退到 Wi-Fi，而不是 Docker/VPN 虚拟网卡。
    #[test]
    fn falls_back_to_wifi_before_virtual() {
        let addrs = vec![
            la("172.17.0.1", "docker0"),
            la("10.8.0.6", "tailscale0"),
            la("192.168.1.42", "Wi-Fi"),
        ];
        assert_eq!(pick_preferred(&addrs).as_deref(), Some("192.168.1.42"));
    }

    /// 只有虚拟网卡时仍要给出地址（返回 None 会掉到 0.0.0.0，用户更难排查）。
    #[test]
    fn keeps_virtual_address_as_last_resort() {
        let addrs = vec![la("192.168.23.1", "VMware Network Adapter VMnet8")];
        assert_eq!(pick_preferred(&addrs).as_deref(), Some("192.168.23.1"));
    }

    /// 实机诊断（多网卡机器上核对推荐值是否落在真实网卡）：
    /// `cargo test --lib live_local_address_order -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_local_address_order() {
        for a in local_addresses() {
            println!(
                "{:<44} {:<40} {}",
                a.interface,
                a.address,
                if a.ipv6 { "v6" } else { "v4" }
            );
        }
        println!("preferred = {:?}", preferred_local_address());
    }
}
