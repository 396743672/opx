/** 字节格式化：自动选择 B/KB/MB/GB/TB */
export function formatBytes(bytes: number, decimals = 1): string {
  if (!bytes || bytes < 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB']
  const i = Math.min(
    units.length - 1,
    Math.floor(Math.log(bytes) / Math.log(1024))
  )
  const v = bytes / Math.pow(1024, i)
  return `${v.toFixed(i === 0 ? 0 : decimals)} ${units[i]}`
}

/** 速率格式化（字节/秒） */
export function formatRate(bytesPerSec: number): string {
  return `${formatBytes(bytesPerSec)}/s`
}

/** 运行时长格式化：1d 2h 3m */
export function formatUptime(seconds: number): string {
  if (seconds < 0) seconds = 0
  const d = Math.floor(seconds / 86400)
  const h = Math.floor((seconds % 86400) / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  const parts: string[] = []
  if (d > 0) parts.push(`${d}d`)
  if (h > 0 || d > 0) parts.push(`${h}h`)
  parts.push(`${m}m`)
  return parts.join(' ')
}

/** 启动时间（unix 秒）转本地时间字符串 */
export function formatBootTime(bootTime: number): string {
  return new Date(bootTime * 1000).toLocaleString()
}
