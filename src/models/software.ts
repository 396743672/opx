export enum ArchiveFormat {
  Zip = 'Zip',
  TarGz = 'TarGz',
  Executable = 'Executable',
}

export interface ArchiveInfo {
  format: ArchiveFormat
  size: number | null
  sha256: string | null
}

export interface BuiltinInfo {
  version: string
  sha256: string
  size: number
}

export interface MirrorSource {
  name: string
  url: string
  builtin?: BuiltinInfo
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
  description_i18n?: string
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
  Starting = 'Starting',
  Stopping = 'Stopping',
  Initializing = 'Initializing',
}

export type InstallSource =
  | { Mirror: { mirror_name: string; url: string } }
  | { Builtin: { version: string } }
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

  // 运行时字段（任务 10/11 新增）
  pid: number | null
  last_started_at: string | null
  last_stopped_at: string | null
  last_error: string | null
  custom_start_command: CustomStartCommand | null
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

// ===== 任务 12：软件管理模块扩展类型 =====
// 注意：Rust 端用 #[serde(tag = "kind", content = "spec")] 序列化枚举，
// 序列化格式为 { kind: "Variant", spec: {...} | null }，
// 不是 { Variant: ... } 形式

/// 自定义软件启动命令
export interface CustomStartCommand {
  executable: string
  args: string[]
  working_dir: string | null
  env_vars: Record<string, string>
  health_check: CustomHealthSpec
  config_file_relative: string | null
}

/// 自定义软件健康检查规格
/// serde tag=kind, content=spec
export type CustomHealthSpec =
  | { kind: 'None'; spec: null }
  | { kind: 'Tcp'; spec: { port: number } }
  | { kind: 'Http'; spec: { url: string; expected_status: number } }


/// 配置表单 schema
export interface ConfigSchema {
  fields: ConfigField[]
  /// 标记为 ephemeral 的字段 key 列表（如 MySQL 初始化密码）。
  /// 这些字段是敏感的一次性值，绝不写入配置文件 / installed.json；前端渲染为红色敏感字段。
  ephemeral_keys?: string[]
}

export interface ConfigField {
  key: string
  label_i18n: string
  field_type: ConfigFieldType
  default_value: any
  section: string | null
  description_i18n: string | null
}

/// 配置字段类型
/// serde tag="type"（无 content，因 unit variant 无负载）
export type ConfigFieldType =
  | { type: 'Text' }
  | { type: 'Number' }
  | { type: 'Port' }
  | { type: 'Password' }
  | { type: 'Select'; options: string[] }
  | { type: 'Size'; units: string[] }

/// 卸载安全性报告
export interface UninstallSafetyReport {
  safe: boolean
  blockers: UninstallBlocker[]
}

export interface UninstallBlocker {
  kind: string
  message_i18n: string
  dependents: JreDependent[]
}

/// JRE 使用情况报告
export interface JreUsageReport {
  in_use: boolean
  is_default: boolean
  dependents: JreDependent[]
}

export interface JreDependent {
  kind: string
  id: string
  name: string
  status: string
}

/// Tauri 事件 software-status-changed 载荷
export interface SoftwareStatusEvent {
  installed_id: string
  status: SoftwareStatus
  pid: number | null
  error: string | null
  timestamp: string
}

/// 内置自定义模板
export interface CustomTemplate {
  id: string
  name_i18n: string
  executable: string
  args: string[]
  config_file_relative: string | null
}

/// 配置表单数据（key → 字段值）
export type FormData = Record<string, any>
