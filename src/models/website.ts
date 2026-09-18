// 与 Rust src-tauri/src/models/website.rs 对齐（serde 无 tag 的 unit 枚举 → 字符串）
export type LocationKind = 'Static' | 'Proxy'
export type StaticSource = 'Dir' | 'Upload'

export interface UpstreamTarget {
  addr: string
}
export interface ProxyHeader {
  name: string
  value: string
}

export interface SiteLocation {
  path: string
  kind: LocationKind
  source?: StaticSource | null
  root?: string | null
  spa_fallback: boolean
  target?: string | null
  upstreams?: UpstreamTarget[] | null
  proxy_headers?: ProxyHeader[] | null
  proxy_subpath?: string | null
}

export interface SslConfig {
  enabled: boolean
  cert_path?: string | null
  key_path?: string | null
  /** 是否由 ACME（Let's Encrypt）签发 */
  acme: boolean
  /** ACME 证书到期时间（RFC3339 本地时间）；自签为空 */
  cert_expires_at?: string | null
  /** 签发/续期该站点证书所用的 DNS 账号 id */
  dns_account_id?: string | null
}

export interface Site {
  id: string
  name: string
  server_name?: string | null
  listen: number
  ssl: SslConfig
  enabled: boolean
  locations: SiteLocation[]
  /** 手写模式：true 时该站点 conf 由源码视图维护，不再被表单自动重建覆盖 */
  custom_conf?: boolean
}

/** 新建空站点（前端生成 id），默认含前端静态路由 + API 代理路由，适配前后端分离部署 */
export function emptySite(): Site {
  return {
    id: crypto.randomUUID(),
    name: '',
    server_name: '',
    listen: 80,
    ssl: { enabled: false, cert_path: null, key_path: null, acme: false, cert_expires_at: null, dns_account_id: null },
    enabled: false,
    locations: [
      { path: '/', kind: 'Static', source: 'Upload', root: '', spa_fallback: true, target: null, upstreams: [], proxy_headers: [], proxy_subpath: null },
      { path: '/api', kind: 'Proxy', source: null, root: null, spa_fallback: false, target: null, upstreams: [], proxy_headers: [], proxy_subpath: null },
    ],
    custom_conf: false,
  }
}
