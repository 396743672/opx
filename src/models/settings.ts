export enum CloseWindowAction {
  CloseToTray = 'CloseToTray',
  Exit = 'Exit',
}

export type ThemeMode = 'auto' | 'light' | 'dark' | 'warm'
export type Language = 'zh-CN' | 'en-US'

export interface AppSettings {
  theme: string
  language: Language
  sidebar_collapsed: boolean
  software_root: string
  config_root: string
  mirror_url: string
  auto_check_update: boolean
  close_window_action: CloseWindowAction
  ask_on_close: boolean
  jre_default_id: string | null
  github_proxy_url: string
  proxy_url: string
  acme_use_staging: boolean
  alert_system_cpu: number
  alert_system_mem: number
  alert_process_cpu: number
  alert_process_mem: number
  /** 指标历史保留天数（监控趋势曲线），默认 7 */
  metrics_retain_days: number
  /** 每实例快照滚动保留数量（备份），默认 5 */
  snapshot_keep: number
  alert_webhook_url: string
  alert_webhook_format: string
  alert_webhook_secret: string
  smtp_host: string
  smtp_port: number
  smtp_user: string
  smtp_pass: string
  smtp_to: string
  smtp_enabled: boolean
  // --- DDNS 动态域名（与证书的 DNS 配置零耦合）---
  ddns_enabled: boolean
  /** 'cloudflare' | 'aliyun' | 'dnspod' | 'huawei' */
  ddns_provider: string
  ddns_cloudflare_token: string
  ddns_aliyun_access_key_id: string
  ddns_aliyun_access_key_secret: string
  ddns_dnspod_secret_id: string
  ddns_dnspod_secret_key: string
  ddns_huawei_access_key: string
  ddns_huawei_secret_key: string
  /** 每行一个完整子域名（如 home.example.com） */
  ddns_domains: string[]
  /** 开启后额外同步 AAAA 记录 */
  ddns_enable_ipv6: boolean
}
