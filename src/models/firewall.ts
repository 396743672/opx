/**
 * 防火墙端口规则页的传输模型（与 `src-tauri/src/commands/firewall.rs` 对应）。
 *
 * 语义约定：
 * - `allowed` 是**系统防火墙的真实状态**，不是 opx 的期望状态；
 * - `bind_local_only` 是「第二道门」：即便 allowed=true，服务只监听回环地址时
 *   外部仍连不上，UI 必须显式提示，否则用户会反复排查防火墙。
 */

export type FirewallBackendKind = 'ufw' | 'firewalld' | 'nftables' | 'windows' | 'none'

/**
 * 判定依据（后端 `Reason::as_str()` 的 code）。
 * 按它取词条而不是按 `kind`：同一个 ufw 在「正在生效」与「装了没启用」两种状态下
 * 必须说不同的话，否则用户会以为端口已经放开。
 */
export type FirewallBackendReason =
  | 'windows'
  | 'unsupported_os'
  | 'ufw_active'
  | 'ufw_inactive'
  | 'firewalld_running'
  | 'firewalld_stopped'
  | 'nftables'
  | 'nft_blocking'
  | 'nft_unreadable'
  | 'no_tool'

export type FirewallEnforcement = 'enforced' | 'bookkeeping_only' | 'system_blocking' | 'disabled' | 'unknown'

export interface FirewallBackendInfo {
  kind: FirewallBackendKind
  /** false 时前端只展示人工放行指引，不展示放行开关 */
  available: boolean
  /** 防火墙当前是否正在生效；已安装未启用时为 false（规则要等启用后才起作用） */
  active: boolean
  /**
   * 规则是否**真的在拦截**入站流量。与 `available`/`active` 不同：
   * - `bookkeeping_only`：只有 opx 建的 policy accept 表，系统不拦任何入站 ——
   *   此时所有端口本来就能访问，页面上的开关只登记、不改变可达性；
   * - `disabled`：防火墙装了但没启用，同样没有任何拦截。
   */
  enforcement: FirewallEnforcement
  reason: FirewallBackendReason
  /** 后端兜底说明（前端有 i18n 词条时优先用词条） */
  note: string
}

export type FirewallPortSource = 'opx' | 'software' | 'jar' | 'site' | 'manual'

export interface FirewallPortEntry {
  port: number
  proto: string
  source: FirewallPortSource
  /** 来源显示名；opx 自身为空，由前端用词条渲染 */
  label: string
  detail: string
  instance_id: string | null
  allowed: boolean
  bind: string | null
  bind_local_only: boolean
  /** 监听地址从哪来：真实监听套接字 / 配置推断 / 未知 */
  bind_source: 'listen' | 'config' | 'unknown'
  /** opx 能否一键改成 0.0.0.0（配置或 schema 里有可写的地址字段） */
  bind_fixable: boolean
}

export interface FirewallOverview {
  backend: FirewallBackendInfo
  ports: FirewallPortEntry[]
  /** 读取规则失败原因（如非 root 运行）：此时 allowed 全为 false 不可信 */
  error: string | null
  web_enabled: boolean
  web_port: number
}

/** 端口条目的稳定标识（同端口可能被多个来源声明，必须带上来源才能唯一定位） */
export function portKey(p: FirewallPortEntry): string {
  return `${p.source}:${p.port}/${p.proto}:${p.label}:${p.detail}`
}
