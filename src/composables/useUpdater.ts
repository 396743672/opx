// 应用更新 composable：封装「检查更新 / 下载安装」并暴露进度状态。
// 端点与代理解析全在 Rust 端（utils/update.rs）完成，这里只触发命令并接收进度事件。
import { ref } from 'vue'
import { invoke } from '@/utils/ipc'
import { listen, type UnlistenFn } from '@/utils/transport'
import { useI18n } from 'vue-i18n'
import { toast } from './useToast'

interface UpdateCheckResult {
  available: boolean
  version: string | null
  notes: string | null
  current_version: string
  /** 便携版：应用内更新不可用，需手动下载便携包覆盖 opx.exe */
  portable: boolean
}

interface UpdateProgress {
  phase: string // started | progress | finished | done
  chunk: number
  total: number | null
}

export function useUpdater() {
  const { t } = useI18n()
  const checking = ref(false)
  const installing = ref(false)
  const updateInfo = ref<{ version: string; notes: string | null } | null>(null)
  const portable = ref(false)
  const progress = ref(0) // 0-100
  const error = ref('')

  // 便携标识与网络无关（后端读 exe 同级 portable.mode 标记 / 安装痕迹），
  // 页面挂载即取，离线也能正确显示「便携版」徽标与手动下载引导。
  async function loadPortable() {
    try {
      portable.value = await invoke<boolean>('is_portable_build')
    } catch {
      // Web 模式无此命令（桌面专属），保持 false
    }
  }

  async function check() {
    checking.value = true
    error.value = ''
    updateInfo.value = null
    try {
      const r = await invoke<UpdateCheckResult>('check_app_update')
      portable.value = r.portable ?? false
      if (r.available && r.version) {
        updateInfo.value = { version: r.version, notes: r.notes }
      } else {
        updateInfo.value = null
        toast(t('updateUpToDate'), 'ok')
      }
    } catch (e) {
      error.value = String(e)
      toast(t('updateCheckFailed', { msg: String(e) }), 'err')
    } finally {
      checking.value = false
    }
  }

  async function install() {
    if (!updateInfo.value || installing.value) return
    // 便携版不触发安装：updater 只能交付 NSIS/MSI，会在 %LOCALAPPDATA%\opx
    // 装出第二份副本且数据不随迁。UI 已改显手动下载引导，这里双保险。
    if (portable.value) return
    installing.value = true
    progress.value = 0
    error.value = ''
    let unlisten: UnlistenFn | null = null
    let downloaded = 0
    let total: number | null = null
    try {
      unlisten = await listen<UpdateProgress>('update-progress', (e) => {
        const p = e.payload
        if (p.phase === 'started') {
          total = p.total
        } else if (p.phase === 'progress') {
          downloaded += p.chunk
          progress.value =
            total && total > 0 ? Math.min(99, Math.floor((downloaded / total) * 100)) : 0
        } else if (p.phase === 'finished') {
          progress.value = 99
        } else if (p.phase === 'done') {
          progress.value = 100
        }
      })
      await invoke('install_app_update')
      toast(t('updateInstalled'), 'ok')
    } catch (e) {
      error.value = String(e)
      toast(t('updateInstallFailed', { msg: String(e) }), 'err')
    } finally {
      installing.value = false
      unlisten?.()
    }
  }

  return { checking, installing, updateInfo, portable, progress, error, check, install, loadPortable }
}
