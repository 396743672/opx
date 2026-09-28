import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@/utils/ipc'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useSpringBootStore } from '@/modules/springboot-manager/stores/springboot'
import { useLifecycleStore } from '@/modules/software-manager/stores/lifecycle'
import type { InstalledSoftware } from '@/models/software'
import { SoftwareStatus } from '@/models/software'
import type { SpringBootApp } from '@/models/springboot'
import { AppStatus } from '@/models/springboot'

/// 运行中的软件与应用：系统监控页与监控中心页共用。
/// 内部自管取数与事件监听的生命周期（onMounted 注册、onUnmounted 清理）。
/// ponytail: installedSoftware 只在挂载时拉一次（与首页原行为一致），
/// 启停变化由 lifecycleStore.getStatus 覆盖状态触发 runningSoftware 重算。
export function useRunningSoftware() {
  const sbStore = useSpringBootStore()
  const lifecycleStore = useLifecycleStore()

  const installedSoftware = ref<InstalledSoftware[]>([])
  const runningApps = ref<SpringBootApp[]>([])
  let unlistenSb: UnlistenFn | null = null
  let unmounted = false

  const runningSoftware = computed(() =>
    installedSoftware.value.filter((s) => {
      const st = lifecycleStore.getStatus(s.id)
      const status = st !== SoftwareStatus.Unknown ? st : s.status
      return status === SoftwareStatus.Running
    })
  )

  async function refreshApps() {
    await sbStore.fetchApps()
    runningApps.value = sbStore.apps.filter((a) => a.status === AppStatus.Running)
  }

  onMounted(async () => {
    installedSoftware.value = await invoke<InstalledSoftware[]>('list_installed_software')
    await refreshApps()
    const unlisten = await listen('springboot-status-changed', refreshApps)
    // 卸载发生在 await 期间时立即注销，否则监听器泄漏（两页共用后暴露面翻倍）
    if (unmounted) unlisten()
    else unlistenSb = unlisten
  })

  onUnmounted(() => {
    unmounted = true
    unlistenSb?.()
  })

  return { installedSoftware, runningSoftware, runningApps }
}
