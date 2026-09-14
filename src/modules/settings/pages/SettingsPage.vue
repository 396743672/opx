<template>
  <div class="animate-fade-in max-w-2xl">
    <PageHeader
      icon="mdi:cog"
      :title="$t('settings')"
      :subtitle="$t('appearance')"
    />

    <div class="rounded-xl border border-border bg-card shadow-card overflow-hidden">
      <!-- 外观 -->
      <div class="px-5 pt-4 pb-1">
        <h3 class="text-sm font-semibold tracking-tight">{{ $t('appearance') }}</h3>
      </div>
      <div class="px-5 pb-1 divide-y divide-border">
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('themeLabel') }}</span>
          <select
            v-model="themeValue"
            class="h-8 px-2 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary cursor-pointer"
          >
            <option value="auto">{{ $t('auto') }}</option>
            <option value="light">{{ $t('light') }}</option>
            <option value="warm">{{ $t('warm') }}</option>
            <option value="dark">{{ $t('dark') }}</option>
          </select>
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('languageLabel') }}</span>
          <select
            v-model="languageValue"
            class="h-8 px-2 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary cursor-pointer"
          >
            <option value="zh-CN">中文</option>
            <option value="en-US">English</option>
          </select>
        </div>
      </div>

      <div class="border-t border-border" />

      <!-- 关闭行为 -->
      <div class="px-5 pt-4 pb-1">
        <h3 class="text-sm font-semibold tracking-tight">{{ $t('closeWindowAction') }}</h3>
      </div>
      <div class="px-5 pb-1 divide-y divide-border">
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('closeBehavior') }}</span>
          <select
            v-model="closeActionValue"
            class="h-8 px-2 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary cursor-pointer"
          >
            <option value="CloseToTray">{{ $t('closeToTray') }}</option>
            <option value="Exit">{{ $t('exitProgram') }}</option>
          </select>
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('askOnClose') }}</span>
          <SwitchBtn v-model="askOnCloseValue" />
        </div>
      </div>

      <div class="border-t border-border" />

      <!-- 开机自启 -->
      <div class="px-5 pt-4 pb-1">
        <h3 class="text-sm font-semibold tracking-tight">{{ $t('autoStartOnBoot') }}</h3>
      </div>
      <div class="px-5 pb-1 divide-y divide-border">
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('autoStartOnBootDesc') }}</span>
          <SwitchBtn v-model="autostartValue" @update:modelValue="onToggleAutostart" />
        </div>
      </div>

      <div class="border-t border-border" />

      <!-- 下载代理 -->
      <div class="px-5 pt-4 pb-1">
        <h3 class="text-sm font-semibold tracking-tight">{{ $t('proxySettings') }}</h3>
      </div>
      <div class="px-5 pb-4 divide-y divide-border">
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('githubProxy') }}</span>
          <div class="flex gap-2 items-center">
            <input
              v-model="githubProxyValue"
              class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
              :placeholder="$t('proxyDefaultHint')"
            />
            <button
              class="btn text-xs h-7 px-2"
              @click="githubProxyValue = 'https://ghfast.top'"
              :title="$t('resetDefault')"
            >↺</button>
          </div>
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('globalProxy') }}</span>
          <input
            v-model="proxyValue"
            class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
            :placeholder="$t('proxyEmptyDirect')"
          />
        </div>
      </div>

      <div class="border-t border-border" />

      <!-- DNS 服务商（证书自动化） -->
      <div class="px-5 pt-4 pb-1">
        <h3 class="text-sm font-semibold tracking-tight">{{ $t('dnsProvider') }}</h3>
      </div>
      <div class="px-5 pb-4 divide-y divide-border">
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('dnsProviderDesc') }}</span>
          <select
            v-model="dnsProviderValue"
            class="h-8 px-2 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary cursor-pointer"
          >
            <option value="cloudflare">Cloudflare</option>
          </select>
        </div>
        <div class="flex items-start justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('cloudflareToken') }}</span>
          <div class="flex flex-col gap-1">
            <input
              v-model="cloudflareApiTokenValue"
              type="password"
              autocomplete="off"
              class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
            />
            <span class="text-xs text-warning">{{ $t('dnsTokenPlaintextWarning') }}</span>
          </div>
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('acmeStaging') }}</span>
          <SwitchBtn v-model="acmeStagingValue" />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '@/stores/settings'
import { CloseWindowAction, type ThemeMode, type Language } from '@/models/settings'
import PageHeader from '@/components/PageHeader.vue'
import SwitchBtn from '@/components/SwitchBtn.vue'

useI18n()
const settingsStore = useSettingsStore()

const themeValue = ref<ThemeMode>('auto')
const languageValue = ref<Language>('zh-CN')
const closeActionValue = ref<CloseWindowAction>(CloseWindowAction.CloseToTray)
const askOnCloseValue = ref(true)
const githubProxyValue = ref('')
const proxyValue = ref('')
const autostartValue = ref(false)
const dnsProviderValue = ref('cloudflare')
const cloudflareApiTokenValue = ref('')
const acmeStagingValue = ref(false)

onMounted(async () => {
  try {
    autostartValue.value = await invoke<boolean>('get_autostart')
  } catch {
    autostartValue.value = false
  }
})

async function onToggleAutostart(v: boolean) {
  try {
    await invoke('set_autostart', { enabled: v })
  } catch (e) {
    console.error('set autostart failed:', e)
    autostartValue.value = !v
  }
}

watch(
  () => settingsStore.settings,
  (s) => {
    if (s) {
      themeValue.value = (s.theme as ThemeMode) || 'auto'
      languageValue.value = (s.language as Language) || 'zh-CN'
      closeActionValue.value = s.close_window_action
      askOnCloseValue.value = s.ask_on_close
      githubProxyValue.value = s.github_proxy_url || ''
      proxyValue.value = s.proxy_url || ''
      dnsProviderValue.value = s.dns_provider || 'cloudflare'
      cloudflareApiTokenValue.value = s.cloudflare_api_token || ''
      acmeStagingValue.value = s.acme_use_staging
    }
  },
  { immediate: true }
)

watch(languageValue, (lang) => {
  settingsStore.setLanguage(lang)
})

watch(themeValue, (mode) => {
  settingsStore.setTheme(mode)
})

let saveTimer: ReturnType<typeof setTimeout> | undefined

watch(
  [closeActionValue, askOnCloseValue, githubProxyValue, proxyValue, dnsProviderValue, cloudflareApiTokenValue, acmeStagingValue],
  () => {
    clearTimeout(saveTimer)
    saveTimer = setTimeout(save, 400)
  }
)

async function save() {
  if (!settingsStore.settings) return
  settingsStore.settings.close_window_action = closeActionValue.value
  settingsStore.settings.ask_on_close = askOnCloseValue.value
  settingsStore.settings.github_proxy_url = githubProxyValue.value
  settingsStore.settings.proxy_url = proxyValue.value
  settingsStore.settings.dns_provider = dnsProviderValue.value
  settingsStore.settings.cloudflare_api_token = cloudflareApiTokenValue.value
  settingsStore.settings.acme_use_staging = acmeStagingValue.value
  await settingsStore.saveSettings()
}
</script>
