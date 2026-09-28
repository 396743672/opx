import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@/utils/ipc'
import type { SystemInfo } from '@/models/system'
import type { ProcessSample } from '@/models/process'

const POLL_INTERVAL = 1000

export const useSystemStore = defineStore('system', () => {
  const systemInfo = ref<SystemInfo | null>(null)

  const loading = ref(false)
  const lastUpdated = ref<number>(0)

  /** 最近一次整机内存总量，用于内存占比归一化 */
  const memTotal = ref(0)

  // 进程采样：一次 invoke 批量采样所有 pid，只返回实时样本（趋势历史由后端持久化）
  async function sampleProcesses(pids: number[]) {
    if (pids.length === 0) return []
    try {
      return await invoke<ProcessSample[]>('sample_process_resources', { pids })
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

      lastUpdated.value = now
    } finally {
      loading.value = false
    }
  }

  function startPolling() {
    if (timer !== null) return
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
    loading,
    lastUpdated,
    netSentRate,
    netRecvRate,
    cpuUsage,
    memoryUsage,
    diskUsage,
    memTotal,
    fetchAll,
    startPolling,
    stopPolling,
    sampleProcesses,
  }
})
