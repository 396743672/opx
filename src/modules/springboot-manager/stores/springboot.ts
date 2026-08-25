import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { ref } from 'vue'
import type { SpringBootApp, AppGroup, JvmInfo, JvmOptsTemplate, ReplaceResult, CreateAppParams, UpdateAppParams } from '@/models/springboot'
import type { InstalledSoftware } from '@/models/software'

export const useSpringBootStore = defineStore('springboot', () => {
  const apps = ref<SpringBootApp[]>([])
  const groups = ref<AppGroup[]>([])
  const loading = ref(false)
  const jdkList = ref<InstalledSoftware[]>([])
  const dependencyCandidates = ref<InstalledSoftware[]>([])

  async function fetchApps() {
    loading.value = true
    try {
      apps.value = await invoke<SpringBootApp[]>('list_springboot_apps')
    } finally {
      loading.value = false
    }
  }

  async function fetchGroups() {
    groups.value = await invoke<AppGroup[]>('list_springboot_groups')
  }

  async function createApp(params: CreateAppParams): Promise<SpringBootApp> {
    const app = await invoke<SpringBootApp>('create_springboot_app', { params })
    apps.value.push(app)
    return app
  }

  async function updateApp(id: string, params: UpdateAppParams): Promise<SpringBootApp> {
    const app = await invoke<SpringBootApp>('update_springboot_app', { id, params })
    const idx = apps.value.findIndex(a => a.id === id)
    if (idx >= 0) apps.value[idx] = app
    return app
  }

  async function deleteApp(id: string) {
    await invoke('delete_springboot_app', { id })
    apps.value = apps.value.filter(a => a.id !== id)
  }

  async function startApp(id: string) {
    await invoke('start_springboot_app', { id })
  }

  async function stopApp(id: string) {
    await invoke('stop_springboot_app', { id })
  }

  async function restartApp(id: string) {
    await invoke('restart_springboot_app', { id })
  }

  async function replaceJar(id: string, newJarPath: string): Promise<ReplaceResult> {
    return await invoke<ReplaceResult>('replace_springboot_jar', { id, newJarPath })
  }

  async function replaceJarAndRestart(id: string, newJarPath: string): Promise<ReplaceResult> {
    return await invoke<ReplaceResult>('replace_springboot_jar_and_restart', { id, newJarPath })
  }

  async function fetchJvmMetrics(id: string): Promise<JvmInfo | null> {
    return await invoke<JvmInfo | null>('get_springboot_jvm_metrics', { id })
  }

  async function fetchJdkList() {
    jdkList.value = await invoke<InstalledSoftware[]>('list_installed_software')
  }

  async function fetchDependencyCandidates() {
    dependencyCandidates.value = await invoke<InstalledSoftware[]>('list_springboot_dependency_candidates')
  }

  async function getRecommendedOpts(jdkInstalledId: string): Promise<JvmOptsTemplate> {
    return await invoke<JvmOptsTemplate>('get_recommended_jvm_opts', { jdkInstalledId })
  }

  async function readJarPort(jarPath: string): Promise<number | null> {
    return await invoke<number | null>('read_jar_port', { jarPath })
  }

  async function saveGroups(newGroups: AppGroup[]) {
    await invoke('save_springboot_groups', { groups: newGroups })
    groups.value = newGroups
  }

  async function getGlobalEnvVars(): Promise<[string, string][]> {
    return await invoke<[string, string][]>('get_springboot_global_env_vars')
  }

  async function setGlobalEnvVars(envVars: [string, string][]) {
    await invoke('set_springboot_global_env_vars', { envVars })
  }

  return {
    apps, groups, loading, jdkList, dependencyCandidates,
    fetchApps, fetchGroups, createApp, updateApp, deleteApp,
    startApp, stopApp, restartApp, replaceJar, replaceJarAndRestart,
    fetchJvmMetrics, fetchJdkList, fetchDependencyCandidates,
    getRecommendedOpts, readJarPort, saveGroups,
    getGlobalEnvVars, setGlobalEnvVars,
  }
})
