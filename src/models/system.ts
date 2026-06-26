export interface SystemInfo {
  cpu_usage: number
  memory_used: number
  memory_total: number
  memory_usage: number
  disks: DiskInfo[]
  network: NetworkInfo
  os_name: string
  os_version: string
  hostname: string
  boot_time: number
}

export interface DiskInfo {
  mount_point: string
  total: number
  used: number
  usage: number
}

export interface NetworkInfo {
  bytes_sent: number
  bytes_recv: number
  packets_sent: number
  packets_recv: number
}

export interface ProcessInfo {
  pid: number
  name: string
  cpu_usage: number
  memory_usage: number
  status: string
}

export interface HistoryPoint {
  timestamp: number
  cpu_usage: number
  memory_usage: number
}
