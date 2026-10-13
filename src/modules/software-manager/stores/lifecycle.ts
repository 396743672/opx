import { defineStore } from 'pinia'
import { ref } from 'vue'
import { listen, type UnlistenFn } from '@/utils/transport'
import { getTransport } from '@/utils/transport'
import { invoke } from '@/utils/ipc'
import { SoftwareStatus, type SoftwareStatusEvent } from '@/models/software'

/**
 * 软件生命周期 Pinia store
 *
 * 监听后端 `software-status-changed` 事件，维护三个字典：
 * - statuses: installed_id → SoftwareStatus
 * - errors:   installed_id → string | null
 * - pids:     installed_id → number | null
 *
 * 组件用法：
 * - onMounted:  await lifecycleStore.initListener()
 * - onBeforeUnmount: lifecycleStore.destroyListener()
 * - getStatus(id) / getError(id) / getPid(id) 取实时状态
 */
export const useLifecycleStore = defineStore('software-lifecycle', () => {
  const statuses = ref<Record<string, SoftwareStatus>>({})
  const errors = ref<Record<string, string | null>>({})
  const pids = ref<Record<string, number | null>>({})
  // 全局错误提示（统一弹窗显示，替代原生 window.alert）
  const errorMessage = ref<string | null>(null)
  // 运行时暂存的「初始化密码」（如 MySQL 初始化 root 密码）。
  // 仅存在于内存，绝不持久化到磁盘；首次初始化消费一次后（server 起来）清除。
  const initPasswords = ref<Record<string, string>>({})
  let unlisten: UnlistenFn | null = null
  // WS 重连补偿（D4）：重连成功后全量刷新状态缓存（桌面模式恒不触发）
  let offResync: UnlistenFn | null = null

  function setStatus(
    id: string,
    status: SoftwareStatus,
    pid?: number | null,
    error?: string | null,
  ) {
    statuses.value[id] = status
    // pid: undefined 不改，null 清除，number 设置
    if (pid !== undefined) {
      pids.value[id] = pid
    }
    // error: undefined 不改，null 清除，string 设置
    if (error !== undefined) {
      errors.value[id] = error
    }
    // 初始化密码仅消费一次：server 起来后清除运行时暂存，避免明文密码常驻内存
    if (status === SoftwareStatus.Running) {
      delete initPasswords.value[id]
    }
  }

  function setInitPassword(id: string, password: string) {
    if (password) initPasswords.value[id] = password
    else delete initPasswords.value[id]
  }

  function getInitPassword(id: string): string | undefined {
    return initPasswords.value[id]
  }

  function clearInitPassword(id: string) {
    delete initPasswords.value[id]
  }

  function getStatus(id: string): SoftwareStatus {
    return statuses.value[id] ?? SoftwareStatus.Unknown
  }

  function getError(id: string): string | null {
    return errors.value[id] ?? null
  }

  function clearErrorMessage() {
    errorMessage.value = null
  }

  function getPid(id: string): number | null {
    return pids.value[id] ?? null
  }

  async function initListener() {
    if (unlisten) return
    unlisten = await listen<SoftwareStatusEvent>(
      'software-status-changed',
      (e) => {
        const { installed_id, status, pid, error } = e.payload
        // 直接传 pid/error（可能是 null），让 setStatus 区分 null（清除）与 undefined（不改）
        setStatus(installed_id, status as SoftwareStatus, pid, error)
        // 启动/重启失败（如端口占用）弹框提示原因，而不只是把按钮状态置为 Error。
        if (status === SoftwareStatus.Error && error) {
          errorMessage.value = error
        }
      },
    )
    // D4 重连补偿：WS 断线期间的状态事件无法补投，重连成功后逐实例重查
    //（get_software_status 已在 HTTP 注册表挂上）。单实例查询失败静默跳过，
    // 不拖垮其余实例的补偿。
    offResync = getTransport().onResync(async () => {
      const ids = Object.keys(statuses.value)
      for (const id of ids) {
        try {
          const st = await invoke<SoftwareStatus>('get_software_status', { installedId: id })
          setStatus(id, st)
        } catch {
          // 实例可能已被卸载，忽略
        }
      }
    })
  }

  function destroyListener() {
    if (unlisten) {
      unlisten()
      unlisten = null
    }
    offResync?.()
    offResync = null
  }

  return {
    statuses,
    errors,
    pids,
    initPasswords,
    errorMessage,
    setStatus,
    getStatus,
    getError,
    getPid,
    setInitPassword,
    getInitPassword,
    clearInitPassword,
    clearErrorMessage,
    initListener,
    destroyListener,
  }
})
