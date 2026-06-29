export enum CloseWindowAction {
  MinimizeToTray = 'MinimizeToTray',
  Exit = 'Exit',
  BackgroundService = 'BackgroundService',
}

export type ThemeMode = 'auto' | 'light' | 'dark'

export interface AppSettings {
  theme: string
  language: string
  sidebar_collapsed: boolean
  software_root: string
  config_root: string
  mirror_url: string
  auto_check_update: boolean
  close_window_action: CloseWindowAction
  register_as_system_service: boolean
  auto_start_managed_services: boolean
}
