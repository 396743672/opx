/**
 * 防火墙端口规则页的传输模型（与 `src-tauri/src/commands/firewall.rs` 对应）。
 *
 * 语义约定：
 * - `allowed` 是**系统防火墙的真实状态**，不是 opx 的期望状态；
 * - `bind_local_only` 是「第二道门」：即便 allowed=true，服务只监听回环地址时
 *   外部仍连不上，UI 必须显式提示，否则用户会反复排查防火墙。
 */

export type FirewallBackendKind = 'ufw' | 'firewalld' | 'nftables' | 'windows' | 'none'

export interface FirewallBackendInfo {
  kind: FirewallBackendKind
  /** false 时前端只展示人工放行指引，不展示放行开关 */
  available: boolean
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
