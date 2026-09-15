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
  dns_provider: string
  cloudflare_api_token: string
  acme_use_staging: boolean
  alert_system_cpu: number
  alert_system_mem: number
  alert_process_cpu: number
  alert_process_mem: number
  /** 指标历史保留天数（监控趋势曲线），默认 7 */
  metrics_retain_days: number
  alert_webhook_url: string
  alert_webhook_format: string
  alert_webhook_secret: string
  smtp_host: string
  smtp_port: number
  smtp_user: string
  smtp_pass: string
  smtp_to: string
  smtp_enabled: boolean
}
