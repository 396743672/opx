export interface SoftwareMeta {
  key: string
  name: string
  description: string
  available_versions: string[]
  default_version: string
}

export enum SoftwareStatus {
  Running = 'Running',
  Stopped = 'Stopped',
  Error = 'Error',
  Unknown = 'Unknown',
}

export interface InstalledSoftware {
  id: string
  key: string
  name: string
  version: string
  install_path: string
  install_time: string
  status: SoftwareStatus
  port: number
  config: Record<string, any>
  is_custom: boolean
  auto_start_on_app_start: boolean
  startup_order: number
}

export interface InstallParams {
  key: string
  version: string
  install_path: string
  [key: string]: unknown
}
