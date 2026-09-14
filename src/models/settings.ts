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
}
