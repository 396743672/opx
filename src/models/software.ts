export enum ArchiveFormat {
  Zip = 'Zip',
  TarGz = 'TarGz',
}

export interface ArchiveInfo {
  format: ArchiveFormat
  size: number | null
  sha256: string | null
}

export interface MirrorSource {
  name: string
  url: string
}

export interface CatalogVersion {
  version: string
  mirrors: MirrorSource[]
  archive: ArchiveInfo
}

export enum SoftwareCategory {
  Database = 'Database',
  Runtime = 'Runtime',
  Cache = 'Cache',
  WebServer = 'WebServer',
}

export interface CatalogEntry {
  key: string
  name: string
  description: string
  category: SoftwareCategory
  icon: string
  versions: CatalogVersion[]
  default_version: string
}

export interface Catalog {
  entries: CatalogEntry[]
  updated_at: string | null
}

export enum SoftwareStatus {
  Running = 'Running',
  Stopped = 'Stopped',
  Error = 'Error',
  Unknown = 'Unknown',
}

export type InstallSource =
  | { Mirror: { mirror_name: string; url: string } }
  | { Custom: { archive_name: string } }

export interface InstalledSoftware {
  id: string
  key: string
  version: string
  name: string
  install_path: string
  install_time: string
  status: SoftwareStatus
  port: number
  config: Record<string, any>
  is_custom: boolean
  auto_start_on_app_start: boolean
  startup_order: number
  source: InstallSource
}

export interface InstalledSoftwareList {
  software: InstalledSoftware[]
}

export interface InstallParams {
  key: string
  version: string
  mirror_index: number
  set_as_default_jre: boolean
}

export interface CustomInstallParams {
  name: string
  archive_path: string
}

// 兼容性保留
export interface SoftwareMeta {
  key: string
  name: string
  description: string
  available_versions: string[]
  default_version: string
}
