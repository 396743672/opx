import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type {
  BackupMode,
  LogChunk,
  LogSource,
  SnapshotMeta,
} from '@/models/software'

/**
 * 实例运维 Pinia store（日志查看器 + 备份/恢复）
 *
 * 封装 8 个 Tauri 命令调用，并提供：
 * - 快照列表缓存（snapshotsCache，按 installed_id 索引）
 * - 重置中状态（resettingIds）
 *
 * 日志实时轮询（1.5s）由组件持有定时器，统一调用本 store 的 readLog 封装，
 * 以便轮询状态与对话框生命周期绑定、避免跨组件泄漏。
 *
 * 命令参数采用 Tauri 默认的 camelCase 约定（Rust 端 snake_case 自动转换）：
 *   installed_id  → installedId
 *   source_index  → sourceIndex
 *   dest_path     → destPath
 *   snapshot_id   → snapshotId
 */
export const useOpsStore = defineStore('software-ops', () => {
  // 快照列表缓存（installed_id -> SnapshotMeta[]）
  const snapshotsCache = ref<Record<string, SnapshotMeta[]>>({})
  // 重置中集合（installed_id -> bool）
  const resettingIds = ref<Record<string, boolean>>({})

  // ===== 日志查看器 =====

  /** 列出某实例的日志来源（StdoutRedirect / ProviderFile） */
  async function getLogSources(installedId: string): Promise<LogSource[]> {
    return invoke<LogSource[]>('get_log_sources', { installedId })
  }

  /** 读取日志（tail / 增量 / 历史分页 + 关键字/正则/级别过滤） */
  async function readLog(params: {
    installedId: string
    sourceIndex: number
    offset?: number | null
    before?: boolean
    limit?: number
    keyword?: string
    regex?: boolean
    level?: string | null
  }): Promise<LogChunk> {
    return invoke<LogChunk>('read_log', {
      installedId: params.installedId,
      sourceIndex: params.sourceIndex,
      offset: params.offset ?? null,
      before: params.before ?? false,
      limit: params.limit ?? 2000,
      keyword: params.keyword ?? null,
      regex: params.regex ?? false,
      level: params.level ?? null,
    })
  }

  /** 下载（拷贝）指定日志源到用户选择的路径 */
  async function downloadLog(
    installedId: string,
    sourceIndex: number,
    destPath: string,
  ): Promise<void> {
    return invoke('download_log', { installedId, sourceIndex, destPath })
  }

  // ===== 备份 / 恢复 / 重置 =====

  /** 创建快照（压缩 data_dirs → <app_data>/backups/<id>/<ts>.zip，并写 manifest），成功后刷新缓存 */
  async function createSnapshot(
    installedId: string,
    mode: BackupMode,
    name?: string | null,
    note?: string | null,
  ): Promise<SnapshotMeta> {
    const meta = await invoke<SnapshotMeta>('create_snapshot', {
      installedId,
      mode,
      name: name ?? null,
      note: note ?? null,
    })
    await loadSnapshots(installedId)
    return meta
  }

  /** 设置定时备份间隔（分钟，0 关闭） */
  async function setBackupSchedule(installedId: string, minutes: number): Promise<void> {
    await invoke('set_backup_schedule', { installedId, minutes })
  }

  /** 查询定时备份间隔（分钟，0 表示未启用） */
  async function getBackupSchedule(installedId: string): Promise<number> {
    return invoke<number>('get_backup_schedule', { installedId })
  }

  /** 直接读取后端快照列表（不写缓存） */
  async function listSnapshots(installedId: string): Promise<SnapshotMeta[]> {
    return invoke<SnapshotMeta[]>('list_snapshots', { installedId })
  }

  /** 读取快照列表并写入缓存 */
  async function loadSnapshots(installedId: string): Promise<SnapshotMeta[]> {
    const list = await listSnapshots(installedId)
    snapshotsCache.value[installedId] = list
    return list
  }

  /** 恢复快照（运行态需先停服；跨大版本需 force 确认） */
  async function restoreSnapshot(
    installedId: string,
    snapshotId: string,
    force = false,
  ): Promise<void> {
    await invoke('restore_snapshot', { installedId, snapshotId, force })
  }

  /** 删除快照（删 zip + 更新 manifest），成功后刷新缓存 */
  async function deleteSnapshot(installedId: string, snapshotId: string): Promise<void> {
    await invoke('delete_snapshot', { installedId, snapshotId })
    await loadSnapshots(installedId)
  }

  /** 一键重置（对每个 data_dir 重建空态，含护栏） */
  async function resetInstance(installedId: string): Promise<void> {
    resettingIds.value[installedId] = true
    try {
      await invoke('reset_instance', { installedId })
    } finally {
      resettingIds.value[installedId] = false
    }
  }

  return {
    snapshotsCache,
    resettingIds,
    getLogSources,
    readLog,
    downloadLog,
    createSnapshot,
    listSnapshots,
    loadSnapshots,
    restoreSnapshot,
    deleteSnapshot,
    resetInstance,
    setBackupSchedule,
    getBackupSchedule,
  }
})
