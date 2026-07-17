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
  register_as_system_service: boolean
  auto_start_managed_services: boolean
  jre_default_id: string | null
  github_proxy_url: string
  proxy_url: string
}
