import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { SystemInfo, HistoryPoint } from '@/models/system'
import type { ProcessSample } from '@/models/process'

const POLL_INTERVAL = 1000
/** 趋势图最多保留的点数，超出后丢弃最旧的 */
const MAX_HISTORY = 120

export const useSystemStore = defineStore('system', () => {
  const systemInfo = ref<SystemInfo | null>(null)
  const history = ref<HistoryPoint[]>([])

  const loading = ref(false)
  const lastUpdated = ref<number>(0)

  /** pid -> 该进程的历史采样点（环形 MAX_HISTORY） */
  const processSamples = ref<Record<number, HistoryPoint[]>>({})
  /** 最近一次整机内存总量，用于内存趋势归一化 */
  const memTotal = ref(0)

  // 进程采样：一次 invoke 批量采样所有 pid，写入各 pid 历史
  async function sampleProcesses(pids: number[]) {
    if (pids.length === 0) return []
    try {
      const samples = await invoke<ProcessSample[]>('sample_process_resources', { pids })
      for (const s of samples) {
        const point: HistoryPoint = {
          timestamp: Date.now(),
          cpu_usage: s.cpu_usage,
          memory_usage: memTotal.value > 0 ? (s.mem_bytes / memTotal.value) * 100 : 0,
        }
        const list = processSamples.value[s.pid] ?? []
        list.push(point)
        if (list.length > MAX_HISTORY) list.splice(0, list.length - MAX_HISTORY)
        processSamples.value[s.pid] = list
      }
      return samples
    } catch (e) {
      console.error('process sample failed:', e)
      return []
    }
  }

  /** 上一次网络字节，用于计算实时速率 */
  let prevNet: { sent: number; recv: number; ts: number } | null = null
  const netSentRate = ref(0) // bytes/s
  const netRecvRate = ref(0) // bytes/s

  let timer: number | null = null

  const cpuUsage = computed(() => systemInfo.value?.cpu_usage ?? 0)
  const memoryUsage = computed(() => systemInfo.value?.memory_usage ?? 0)

  /** 磁盘聚合使用率（取所有挂载点平均） */
  const diskUsage = computed(() => {
    const disks = systemInfo.value?.disks
    if (!disks || disks.length === 0) return 0
    return disks.reduce((s, d) => s + d.usage, 0) / disks.length
  })

  async function fetchAll() {
    loading.value = true
    try {
      const info = await invoke<SystemInfo>('system_info')
      systemInfo.value = info
      if (info.memory_total) memTotal.value = info.memory_total

      // 计算网络速率
      const now = Date.now()
      if (prevNet && info.network) {
        const dt = (now - prevNet.ts) / 1000
        if (dt > 0) {
          netSentRate.value = Math.max(
            0,
            (info.network.bytes_sent - prevNet.sent) / dt
          )
          netRecvRate.value = Math.max(
            0,
            (info.network.bytes_recv - prevNet.recv) / dt
          )
        }
      }
      if (info.network) {
        prevNet = {
          sent: info.network.bytes_sent,
          recv: info.network.bytes_recv,
          ts: now,
        }
      }

      // 追加历史趋势点
      history.value.push({
        timestamp: now,
        cpu_usage: info.cpu_usage,
        memory_usage: info.memory_usage,
      })
      if (history.value.length > MAX_HISTORY) {
        history.value = history.value.slice(-MAX_HISTORY)
      }

      lastUpdated.value = now
    } finally {
      loading.value = false
    }
  }

  async function loadHistory() {
    try {
      const pts = await invoke<HistoryPoint[]>('system_history')
      if (pts && pts.length) {
        history.value = pts.slice(-MAX_HISTORY)
      }
    } catch {
      // 历史文件可能尚不存在，忽略
    }
  }

  function startPolling() {
    if (timer !== null) return
    loadHistory()
    fetchAll()
    timer = window.setInterval(fetchAll, POLL_INTERVAL)
  }

  function stopPolling() {
    if (timer !== null) {
      clearInterval(timer)
      timer = null
    }
  }

  return {
    systemInfo,
    history,
    loading,
    lastUpdated,
    netSentRate,
    netRecvRate,
    cpuUsage,
    memoryUsage,
    diskUsage,
    processSamples,
    memTotal,
    fetchAll,
    loadHistory,
    startPolling,
    stopPolling,
    sampleProcesses,
  }
})
