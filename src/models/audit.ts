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

export interface AuditQuery {
  entries: AuditEntry[]
  /** 过滤后的总条数（不受分页影响） */
  total: number
  /** 是否还有下一页 */
  truncated: boolean
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
