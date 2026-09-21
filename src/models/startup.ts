/** 应用启动编排报告（扩展 4：统一启动编排 + 报告） */

/** 启动项类型（与后端 models/startup.rs 的 KIND_* 常量一致） */
export type StartupItemKind = 'software' | 'node' | 'stack'

/** 单项结果状态 */
export type StartupItemStatus = 'ok' | 'failed' | 'skipped'

/** 单项报告 */
export interface StartupItemReport {
  kind: string
  id: string
  name: string
  status: StartupItemStatus
  elapsed_ms: number
  message: string
}

/** 一次启动编排的整体报告 */
export interface StartupReport {
  /** RFC3339 本地时间 */
  started_at: string
  total_elapsed_ms: number
  /** 逐项结果（含被跳过项） */
  items: StartupItemReport[]
  /** 是否发生了失败回滚 */
  rolled_back: boolean
}
