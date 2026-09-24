/// 操作记录（对应后端 services/software_manager/audit.rs）

export interface AuditEntry {
  ts: string
  action: string
  target: string
  detail: string
  /** '' = 未采集 | 'running' | 'ok' | 'fail' */
  result: string
  /** result === 'fail' 时非空 */
  error: string
}

export interface ActionCount {
  action: string
  count: number
}

export interface AuditStats {
  today: number
  total: number
  /** 近 N 天失败条数 */
  failed: number
  by_action: ActionCount[]
  last_ts: string | null
}
