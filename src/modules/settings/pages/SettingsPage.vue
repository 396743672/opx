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
        <div class="flex items-start justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('testToken') }}</span>
          <div class="flex flex-col gap-1 items-end min-w-0">
            <div class="flex items-center gap-2">
              <input
                v-model="dnsTestZone"
                class="h-8 px-2 w-56 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
                :placeholder="$t('testTokenZonePlaceholder')"
              />
              <button class="btn text-xs h-7 px-2" :disabled="dnsTesting" @click="testToken">
                {{ dnsTesting ? $t('testTokenTesting') : $t('testToken') }}
              </button>
            </div>
            <span
              v-if="dnsTestResult"
              class="text-xs max-w-72 text-right"
              :class="dnsTestOk ? 'text-success' : 'text-destructive'"
              style="overflow-wrap: anywhere; word-break: break-word"
            >{{ dnsTestResult }}</span>
          </div>
        </div>
      </div>

      <div class="border-t border-border" />

      <!-- 告警阈值 -->
      <div class="px-5 pt-4 pb-1">
        <h3 class="text-sm font-semibold tracking-tight">{{ $t('alertThresholds') }}</h3>
      </div>
      <div class="px-5 pb-4 divide-y divide-border">
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('alertSystemCpu') }}</span>
          <input
            v-model.number="alertSystemCpuValue"
            type="number"
            min="1"
            max="100"
            class="h-8 px-2 w-20 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
          />
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('alertSystemMem') }}</span>
          <input
            v-model.number="alertSystemMemValue"
            type="number"
            min="1"
            max="100"
            class="h-8 px-2 w-20 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
          />
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('alertProcessCpu') }}</span>
          <input
            v-model.number="alertProcessCpuValue"
            type="number"
            min="1"
            max="100"
            class="h-8 px-2 w-20 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
          />
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('alertProcessMem') }}</span>
          <input
            v-model.number="alertProcessMemValue"
            type="number"
            min="1"
            max="100"
            class="h-8 px-2 w-20 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
          />
        </div>
      </div>

      <div class="border-t border-border" />

      <!-- 告警通知 -->
      <div class="px-5 pt-4 pb-1">
        <h3 class="text-sm font-semibold tracking-tight">{{ $t('alertNotify') }}</h3>
      </div>
      <div class="px-5 pb-4 divide-y divide-border">
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('webhookUrl') }}</span>
          <input
            v-model="webhookUrlValue"
            class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
            :placeholder="$t('webhookUrlPlaceholder')"
          />
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('webhookFormat') }}</span>
          <select
            v-model="webhookFormatValue"
            class="h-8 px-2 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary cursor-pointer"
          >
            <option value="json">{{ $t('webhookFormatJson') }}</option>
            <option value="dingtalk">{{ $t('webhookFormatDingtalk') }}</option>
            <option value="wecom">{{ $t('webhookFormatWecom') }}</option>
            <option value="feishu">{{ $t('webhookFormatFeishu') }}</option>
          </select>
        </div>
        <div v-if="webhookFormatValue === 'dingtalk'" class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('dingtalkSecret') }}</span>
          <input
            v-model="webhookSecretValue"
            type="password"
            class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
          />
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('smtpEnabled') }}</span>
          <SwitchBtn v-model="smtpEnabledValue" />
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('smtpHost') }}</span>
          <input
            v-model="smtpHostValue"
            class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
            placeholder="smtp.qq.com"
          />
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('smtpPort') }}</span>
          <input
            v-model.number="smtpPortValue"
            type="number"
            min="1"
            max="65535"
            class="h-8 px-2 w-20 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
          />
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('smtpUser') }}</span>
          <input
            v-model="smtpUserValue"
            class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
          />
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('smtpPass') }}</span>
          <input
            v-model="smtpPassValue"
            type="password"
            class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
          />
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('smtpTo') }}</span>
          <input
            v-model="smtpToValue"
            class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
            placeholder="a@x.com, b@y.com"
          />
        </div>
        <div v-if="smtpEnabledValue" class="flex items-start justify-between gap-4 py-3">
          <span class="text-xs text-warning max-w-72">{{ $t('smtpPassWarning') }}</span>
        </div>
        <div class="flex items-center justify-between gap-4 py-3">
          <span class="text-sm">{{ $t('sendTestNotify') }}</span>
          <div class="flex flex-col gap-1 items-end min-w-0">
            <button class="btn text-xs h-7 px-2" :disabled="notifyTesting" @click="testNotify">
              {{ notifyTesting ? $t('testNotifySending') : $t('sendTestNotify') }}
            </button>
            <span
              v-if="notifyTestResult"
              class="text-xs max-w-72 text-right"
              :class="notifyTestOk ? 'text-success' : 'text-destructive'"
              style="overflow-wrap: anywhere; word-break: break-word"
            >{{ notifyTestResult }}</span>
          </div>
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

const { t } = useI18n()
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
const alertSystemCpuValue = ref(90)
const alertSystemMemValue = ref(90)
const alertProcessCpuValue = ref(90)
const alertProcessMemValue = ref(90)
const webhookUrlValue = ref('')
const webhookFormatValue = ref('json')
const webhookSecretValue = ref('')
const smtpEnabledValue = ref(false)
const smtpHostValue = ref('')
const smtpPortValue = ref(465)
const smtpUserValue = ref('')
const smtpPassValue = ref('')
const smtpToValue = ref('')
const notifyTesting = ref(false)
const notifyTestResult = ref('')
const notifyTestOk = ref(false)
// DNS Token 测试（不持久化）：填入该服务商账户下的域名，实际建/删一条临时 TXT 验证写权限
const dnsTestZone = ref('')
const dnsTesting = ref(false)
const dnsTestResult = ref('')
const dnsTestOk = ref(false)

async function testToken() {
  const zone = dnsTestZone.value.trim()
  if (!zone) {
    dnsTestOk.value = false
    dnsTestResult.value = t('testTokenNeedZone')
    return
  }
  dnsTesting.value = true
  dnsTestResult.value = t('testTokenTesting')
  try {
    await invoke('test_dns_token', {
      provider: dnsProviderValue.value,
      token: cloudflareApiTokenValue.value,
      zone,
    })
    dnsTestOk.value = true
    dnsTestResult.value = t('testTokenOk')
  } catch (e) {
    dnsTestOk.value = false
    dnsTestResult.value = String(e)
  } finally {
    dnsTesting.value = false
  }
}

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
      alertSystemCpuValue.value = s.alert_system_cpu ?? 90
      alertSystemMemValue.value = s.alert_system_mem ?? 90
      alertProcessCpuValue.value = s.alert_process_cpu ?? 90
      alertProcessMemValue.value = s.alert_process_mem ?? 90
      webhookUrlValue.value = s.alert_webhook_url || ''
      webhookFormatValue.value = s.alert_webhook_format || 'json'
      webhookSecretValue.value = s.alert_webhook_secret || ''
      smtpEnabledValue.value = s.smtp_enabled
      smtpHostValue.value = s.smtp_host || ''
      smtpPortValue.value = s.smtp_port ?? 465
      smtpUserValue.value = s.smtp_user || ''
      smtpPassValue.value = s.smtp_pass || ''
      smtpToValue.value = s.smtp_to || ''
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
  [closeActionValue, askOnCloseValue, githubProxyValue, proxyValue, dnsProviderValue, cloudflareApiTokenValue, acmeStagingValue, alertSystemCpuValue, alertSystemMemValue, alertProcessCpuValue, alertProcessMemValue, webhookUrlValue, webhookFormatValue, webhookSecretValue, smtpEnabledValue, smtpHostValue, smtpPortValue, smtpUserValue, smtpPassValue, smtpToValue],
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
  // 数值输入被清空时 v-model.number 会给出 ''，直接写进 store 会让 save_settings 反序列化失败
  // 并污染后续所有保存 —— 这里归一到 [1,100]，非法值回落默认 90
  const pct = (v: number) => (Number(v) >= 1 && Number(v) <= 100 ? Number(v) : 90)
  alertSystemCpuValue.value = pct(alertSystemCpuValue.value)
  alertSystemMemValue.value = pct(alertSystemMemValue.value)
  alertProcessCpuValue.value = pct(alertProcessCpuValue.value)
  alertProcessMemValue.value = pct(alertProcessMemValue.value)
  settingsStore.settings.alert_system_cpu = alertSystemCpuValue.value
  settingsStore.settings.alert_system_mem = alertSystemMemValue.value
  settingsStore.settings.alert_process_cpu = alertProcessCpuValue.value
  settingsStore.settings.alert_process_mem = alertProcessMemValue.value
  // 端口输入清空时 v-model.number 给 ''，归一回落 465（同告警阈值的 pct 兜底逻辑）
  smtpPortValue.value = Number(smtpPortValue.value) >= 1 && Number(smtpPortValue.value) <= 65535 ? Number(smtpPortValue.value) : 465
  settingsStore.settings.alert_webhook_url = webhookUrlValue.value
  settingsStore.settings.alert_webhook_format = webhookFormatValue.value
  settingsStore.settings.alert_webhook_secret = webhookSecretValue.value
  settingsStore.settings.smtp_enabled = smtpEnabledValue.value
  settingsStore.settings.smtp_host = smtpHostValue.value
  settingsStore.settings.smtp_port = smtpPortValue.value
  settingsStore.settings.smtp_user = smtpUserValue.value
  settingsStore.settings.smtp_pass = smtpPassValue.value
  settingsStore.settings.smtp_to = smtpToValue.value
  await settingsStore.saveSettings()
}

async function testNotify() {
  if (!webhookUrlValue.value.trim() && !smtpEnabledValue.value) {
    notifyTestOk.value = false
    notifyTestResult.value = t('testNotifyNoChannel')
    return
  }
  notifyTesting.value = true
  notifyTestResult.value = t('testNotifySending')
  try {
    await save() // 先持久化当前输入，后端读的是 settings.json
    await invoke('test_alert_webhook')
    notifyTestOk.value = true
    notifyTestResult.value = t('testTokenOk')
  } catch (e) {
    notifyTestOk.value = false
    notifyTestResult.value = String(e)
  } finally {
    notifyTesting.value = false
  }
}
</script>
