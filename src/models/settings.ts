export interface AppSettings {
  theme: 'light' | 'dark' | 'auto'
  language: 'zh-CN' | 'en-US'
  sidebarCollapsed: boolean
  register_as_system_service: boolean
  auto_start_managed_services: boolean
  close_window_action: 'minimize-to-tray' | 'exit' | 'background-service'
}