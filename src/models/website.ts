// 与 Rust src-tauri/src/models/website.rs 对齐（serde 无 tag 的 unit 枚举 → 字符串）
export type LocationKind = 'Static' | 'Proxy'
export type StaticSource = 'Dir' | 'Upload'

export interface SiteLocation {
  path: string
  kind: LocationKind
  source?: StaticSource | null
  root?: string | null
  spa_fallback: boolean
  target?: string | null
}

export interface SslConfig {
  enabled: boolean
  cert_path?: string | null
  key_path?: string | null
}

export interface Site {
  id: string
  name: string
  server_name?: string | null
  listen: number
  ssl: SslConfig
  enabled: boolean
  locations: SiteLocation[]
}

/** 新建空站点（前端生成 id） */
export function emptySite(): Site {
  return {
    id: crypto.randomUUID(),
    name: '',
    server_name: '',
    listen: 80,
    ssl: { enabled: false, cert_path: null, key_path: null },
    enabled: true,
    locations: [
      { path: '/', kind: 'Static', source: 'Upload', root: '', spa_fallback: true, target: null },
    ],
  }
}
