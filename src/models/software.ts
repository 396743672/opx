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
  Registry = 'Registry',
  Storage = 'Storage',
  MessageQueue = 'MessageQueue',
  Search = 'Search',
  TimeSeries = 'TimeSeries',
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
  // 来自 catalog 的软件图标（软件专属，与软件仓库一致）；自定义软件为空
  icon: string
  // 来自 catalog 的软件分类；自定义软件为 null
  category: SoftwareCategory | null
}

export interface InstalledSoftwareList {
  software: InstalledSoftware[]
}

/// 可升级信息（check_upgrades 返回）
export interface UpgradeInfo {
  key: string
  name: string
  current_version: string
  target_version?: string | null
  /// 同 key 存在 <ver>.bak 备份时的回滚目标版本
  rollback_to?: string | null
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
  /// 字段显示/必填规则：如「auth_enabled=true 时才显示 admin_token 且必填」。
  field_rules?: FieldRule[]
}

/// 字段规则：visible_when 满足时显示；required 时必填（非空）。
export interface FieldRule {
  field_key: string
  visible_when?: FieldCondition | null
  required: boolean
}

/// 条件：当 config 的 key 字段值等于 equals 时满足。
export interface FieldCondition {
  key: string
  equals: any
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
  | { type: 'Select'; options: string[]; labels?: string[]; disabled_options?: string[]; disabled_hint_i18n?: string }
  | { type: 'Size'; units: string[] }
  | { type: 'Boolean' }

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

// ===== C 扩展（日志查看器 + 备份/恢复）前端类型 =====

/** 日志来源种类 */
export type LogSourceKind = 'StdoutRedirect' | 'ProviderFile'

/** 单条日志来源（后端 SoftwareProvider.log_sources 序列化给前端） */
export interface LogSource {
  /** 日志文件绝对路径 */
  path: string
  kind: LogSourceKind
  /** 是否结构化、可显示级别筛选 */
  has_levels: boolean
  /** provider 提供的级别提取正则；null 时用前端内置默认正则 */
  level_pattern: string | null
  /** 展示名（如「访问日志」「错误日志」）；null 时前端用通用标签 */
  label?: string | null
  /** 历史归档（时间倒序，最新在前）；空 = 无归档 */
  archives?: ArchiveLog[]
}

/** 历史归档日志描述（同目录/日期目录下滚动压缩的旧日志） */
export interface ArchiveLog {
  /** 归档文件绝对路径 */
  path: string
  /** 展示标签（如 "2026-08-24" / "info.2026-08-26.0.log.gz"） */
  label: string
}

/** 全局日志搜索命中（search_all_logs 返回） */
export interface LogHit {
  installed_id: string
  software_name: string
  source_label: string
  file: string
  line: string
}

/** 读取日志返回的分块（前端轮询/分页消费） */
export interface LogChunk {
  /** 命中的日志行（已应用关键字/正则/级别过滤） */
  lines: string[]
  /** 本块首行的字节偏移 */
  start_offset: number
  /** 本块末行之后的字节偏移（下次轮询/分页携带） */
  end_offset: number
  /** 文件总字节数 */
  total_bytes: number
  /** 向前是否还有更早的历史（用于「加载更多历史」） */
  has_more: boolean
  /** 因超过单次上限被截断（命中行多于 limit） */
  truncated: boolean
  /** 当前实际读取的历史归档索引（0=主文件） */
  archive_index?: number
}

/** 备份模式 */
export type BackupMode = 'StopAndBackup' | 'Hot'

/** 快照元信息（持久化于 <app_data>/backups/<id>/manifest.json） */
export interface SnapshotMeta {
  /** = 快照文件名去后缀（如 20260817_143000） */
  id: string
  /** RFC3339 创建时间 */
  created_at: string
  /** 来源软件 key（如 "mysql"） */
  source_key: string
  /** 来源软件版本（如 "8.4.11"） */
  source_version: string
  /** 首数字段大版本；MinIO 等无法解析为 null */
  major_version: number | null
  /** zip 文件字节大小 */
  size_bytes: number
  /** 快照格式（"zip"） */
  format: string
  /** 用户自定义名称 */
  name: string | null
  /** 用户自定义备注 */
  note: string | null
}
