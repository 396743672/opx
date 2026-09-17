/** 与 Rust src-tauri/src/models/dns_account.rs 对齐 */
export interface DnsAccount {
  id: string
  name: string
  /** 'cloudflare' | 'aliyun' | 'dnspod' | 'huawei' */
  provider: string
  token: string
  access_key_id: string
  access_key_secret: string
  zones: string[]
  tested_at?: string | null
}

export const PROVIDERS = ['cloudflare', 'aliyun', 'dnspod', 'huawei'] as const
export type ProviderId = (typeof PROVIDERS)[number]

/** 各服务商的凭证字段标签（UI 按 provider 切换） */
export const CRED_FIELDS: Record<ProviderId, { idLabel: string; secretLabel: string }> = {
  cloudflare: { idLabel: '', secretLabel: 'token' },
  aliyun: { idLabel: 'AccessKey ID', secretLabel: 'AccessKey Secret' },
  dnspod: { idLabel: 'SecretId', secretLabel: 'SecretKey' },
  huawei: { idLabel: 'AccessKey', secretLabel: 'SecretKey' },
}

export function emptyAccount(): DnsAccount {
  return {
    id: crypto.randomUUID(),
    name: '',
    provider: 'cloudflare',
    token: '',
    access_key_id: '',
    access_key_secret: '',
    zones: [],
    tested_at: null,
  }
}