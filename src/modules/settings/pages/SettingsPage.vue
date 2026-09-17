<template>
  <div class="animate-fade-in max-w-2xl">
    <PageHeader
      icon="mdi:cog"
      :title="$t('settings')"
      :subtitle="$t('appearance')"
    />

    <!-- Tab 分区：切换只改 v-show，字段与保存逻辑不变 -->
    <div class="mb-4">
      <CategoryTabs v-model="activeTab" :tabs="settingsTabs" />
    </div>

    <div class="rounded-xl border border-border bg-card shadow-card overflow-hidden">
      <!-- ===== 通用 ===== -->
      <div v-show="activeTab === 'general'">
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
      </div>

      <!-- ===== 监控与告警 ===== -->
      <div v-show="activeTab === 'monitor'">
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
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('metricsRetainDays') }}</span>
            <div class="flex items-center gap-2">
              <input
                v-model.number="metricsRetainDaysValue"
                type="number"
                min="1"
                max="90"
                class="h-8 px-2 w-20 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
              />
              <span class="text-xs text-muted-foreground">{{ $t('daysUnit') }}</span>
            </div>
          </div>
        </div>

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

      <!-- ===== 域名与 DNS ===== -->
      <div v-show="activeTab === 'dns'">
        <!-- DDNS 动态域名 -->
        <div class="px-5 pt-4 pb-1">
          <h3 class="text-sm font-semibold tracking-tight">{{ $t('ddnsSection') }}</h3>
        </div>
        <div class="px-5 pb-4 divide-y divide-border">
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('ddnsEnabled') }}</span>
            <SwitchBtn v-model="ddnsEnabledValue" />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('ddnsProvider') }}</span>
            <select v-model="ddnsProviderValue" class="h-8 px-2 w-56 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary cursor-pointer">
              <option value="cloudflare">Cloudflare</option>
              <option value="aliyun">阿里云</option>
              <option value="dnspod">DNSPod</option>
              <option value="huawei">华为云</option>
            </select>
          </div>
          <div v-if="ddnsProviderValue === 'cloudflare'" class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('ddnsCloudflareToken') }}</span>
            <input v-model="ddnsCloudflareTokenValue" type="password"
              class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
          </div>
          <template v-else-if="ddnsProviderValue === 'aliyun'">
            <div class="flex items-center justify-between gap-4 py-3">
              <span class="text-sm">{{ $t('ddnsAliyunKey') }}</span>
              <input v-model="ddnsAliyunKeyValue" class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
            </div>
            <div class="flex items-center justify-between gap-4 py-3">
              <span class="text-sm">{{ $t('ddnsAliyunSecret') }}</span>
              <input v-model="ddnsAliyunSecretValue" type="password"
                class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
            </div>
          </template>
          <template v-else-if="ddnsProviderValue === 'dnspod'">
            <div class="flex items-center justify-between gap-4 py-3">
              <span class="text-sm">{{ $t('ddnsDnspodId') }}</span>
              <input v-model="ddnsDnspodIdValue" class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
            </div>
            <div class="flex items-center justify-between gap-4 py-3">
              <span class="text-sm">{{ $t('ddnsDnspodKey') }}</span>
              <input v-model="ddnsDnspodKeyValue" type="password"
                class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
            </div>
          </template>
          <template v-else>
            <div class="flex items-center justify-between gap-4 py-3">
              <span class="text-sm">{{ $t('ddnsHuaweiKey') }}</span>
              <input v-model="ddnsHuaweiKeyValue" class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
            </div>
            <div class="flex items-center justify-between gap-4 py-3">
              <span class="text-sm">{{ $t('ddnsHuaweiSecret') }}</span>
              <input v-model="ddnsHuaweiSecretValue" type="password"
                class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
            </div>
          </template>
          <div class="flex items-start justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('ddnsDomains') }}</span>
            <textarea v-model="ddnsDomainsText" rows="4"
              :placeholder="$t('ddnsDomainsPlaceholder')"
              class="px-2 py-1.5 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">
              {{ $t('ddnsIpv6') }}
              <span class="block text-xs text-muted-foreground">{{ $t('ddnsIpv6Hint') }}</span>
            </span>
            <SwitchBtn v-model="ddnsIpv6Value" />
          </div>
          <div class="flex items-center justify-between gap-4 py-3">
            <span class="text-sm">{{ $t('syncDdnsNow') }}</span>
            <div class="flex flex-col gap-1 items-end min-w-0">
              <button class="btn text-xs h-7 px-2" :disabled="ddnsSyncing" @click="syncDdns">
                {{ ddnsSyncing ? $t('ddnsSyncing') : $t('syncDdnsNow') }}
              </button>
              <span v-if="ddnsSyncResult" class="text-xs max-w-72 text-right"
                :class="ddnsSyncOk ? 'text-success' : 'text-destructive'"
                style="overflow-wrap: anywhere; word-break: break-word">{{ ddnsSyncResult }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '@/stores/settings'
import { CloseWindowAction, type ThemeMode, type Language } from '@/models/settings'
import PageHeader from '@/components/PageHeader.vue'
import SwitchBtn from '@/components/SwitchBtn.vue'
import CategoryTabs, { type CategoryTab } from '@/modules/software-manager/components/CategoryTabs.vue'

const { t } = useI18n()
const settingsStore = useSettingsStore()
const route = useRoute()
const router = useRouter()

// Tab 分区：选中状态存 URL query —— 刷新保持、可从别处直达；非法值回落首 Tab
const TAB_KEYS = ['general', 'monitor', 'dns'] as const
const activeTab = ref<string>(
  TAB_KEYS.includes(route.query.tab as (typeof TAB_KEYS)[number])
    ? (route.query.tab as string)
    : 'general'
)
watch(activeTab, (tab) => {
  // replace：切 Tab 不该污染浏览器历史
  router.replace({ query: { ...route.query, tab } })
})

const settingsTabs = computed<CategoryTab[]>(() => [
  { key: 'general', label: t('settingsTabGeneral'), icon: 'mdi:tune' },
  { key: 'monitor', label: t('settingsTabMonitor'), icon: 'mdi:bell-outline' },
  { key: 'dns', label: t('settingsTabDdns'), icon: 'mdi:dns-outline' },
])

const themeValue = ref<ThemeMode>('auto')
const languageValue = ref<Language>('zh-CN')
const closeActionValue = ref<CloseWindowAction>(CloseWindowAction.CloseToTray)
const askOnCloseValue = ref(true)
const githubProxyValue = ref('')
const proxyValue = ref('')
const autostartValue = ref(false)
const acmeStagingValue = ref(false)
const alertSystemCpuValue = ref(90)
const alertSystemMemValue = ref(90)
const alertProcessCpuValue = ref(90)
const alertProcessMemValue = ref(90)
const metricsRetainDaysValue = ref(7)
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
const ddnsEnabledValue = ref(false)
const ddnsProviderValue = ref('cloudflare')
const ddnsCloudflareTokenValue = ref('')
const ddnsAliyunKeyValue = ref('')
const ddnsAliyunSecretValue = ref('')
const ddnsDnspodIdValue = ref('')
const ddnsDnspodKeyValue = ref('')
const ddnsHuaweiKeyValue = ref('')
const ddnsHuaweiSecretValue = ref('')
const ddnsDomainsText = ref('')
const ddnsIpv6Value = ref(false)
const ddnsSyncing = ref(false)
const ddnsSyncResult = ref('')
const ddnsSyncOk = ref(false)

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
      acmeStagingValue.value = s.acme_use_staging
      alertSystemCpuValue.value = s.alert_system_cpu ?? 90
      alertSystemMemValue.value = s.alert_system_mem ?? 90
      alertProcessCpuValue.value = s.alert_process_cpu ?? 90
      alertProcessMemValue.value = s.alert_process_mem ?? 90
      metricsRetainDaysValue.value = s.metrics_retain_days ?? 7
      webhookUrlValue.value = s.alert_webhook_url || ''
      webhookFormatValue.value = s.alert_webhook_format || 'json'
      webhookSecretValue.value = s.alert_webhook_secret || ''
      smtpEnabledValue.value = s.smtp_enabled
      smtpHostValue.value = s.smtp_host || ''
      smtpPortValue.value = s.smtp_port ?? 465
      smtpUserValue.value = s.smtp_user || ''
      smtpPassValue.value = s.smtp_pass || ''
      smtpToValue.value = s.smtp_to || ''
      ddnsEnabledValue.value = s.ddns_enabled
      ddnsProviderValue.value = s.ddns_provider || 'cloudflare'
      ddnsCloudflareTokenValue.value = s.ddns_cloudflare_token || ''
      ddnsAliyunKeyValue.value = s.ddns_aliyun_access_key_id || ''
      ddnsAliyunSecretValue.value = s.ddns_aliyun_access_key_secret || ''
      ddnsDnspodIdValue.value = s.ddns_dnspod_secret_id || ''
      ddnsDnspodKeyValue.value = s.ddns_dnspod_secret_key || ''
      ddnsHuaweiKeyValue.value = s.ddns_huawei_access_key || ''
      ddnsHuaweiSecretValue.value = s.ddns_huawei_secret_key || ''
      ddnsDomainsText.value = (s.ddns_domains || []).join('\n')
      ddnsIpv6Value.value = s.ddns_enable_ipv6
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
  [closeActionValue, askOnCloseValue, githubProxyValue, proxyValue, acmeStagingValue, alertSystemCpuValue, alertSystemMemValue, alertProcessCpuValue, alertProcessMemValue, metricsRetainDaysValue, webhookUrlValue, webhookFormatValue, webhookSecretValue, smtpEnabledValue, smtpHostValue, smtpPortValue, smtpUserValue, smtpPassValue, smtpToValue, ddnsEnabledValue, ddnsProviderValue, ddnsCloudflareTokenValue, ddnsAliyunKeyValue, ddnsAliyunSecretValue, ddnsDnspodIdValue, ddnsDnspodKeyValue, ddnsHuaweiKeyValue, ddnsHuaweiSecretValue, ddnsDomainsText, ddnsIpv6Value],
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
  // 指标保留天数归一 [1,90]，非法值回落 7（清空输入同理，防止 '' 写坏 settings.json）
  metricsRetainDaysValue.value =
    Number(metricsRetainDaysValue.value) >= 1 && Number(metricsRetainDaysValue.value) <= 90
      ? Number(metricsRetainDaysValue.value)
      : 7
  settingsStore.settings.metrics_retain_days = metricsRetainDaysValue.value
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
  settingsStore.settings.ddns_enabled = ddnsEnabledValue.value
  settingsStore.settings.ddns_provider = ddnsProviderValue.value
  settingsStore.settings.ddns_cloudflare_token = ddnsCloudflareTokenValue.value
  settingsStore.settings.ddns_aliyun_access_key_id = ddnsAliyunKeyValue.value
  settingsStore.settings.ddns_aliyun_access_key_secret = ddnsAliyunSecretValue.value
  settingsStore.settings.ddns_dnspod_secret_id = ddnsDnspodIdValue.value
  settingsStore.settings.ddns_dnspod_secret_key = ddnsDnspodKeyValue.value
  settingsStore.settings.ddns_huawei_access_key = ddnsHuaweiKeyValue.value
  settingsStore.settings.ddns_huawei_secret_key = ddnsHuaweiSecretValue.value
  settingsStore.settings.ddns_domains = ddnsDomainsText.value.split('\n')
    .map((x) => x.trim()).filter(Boolean)
  settingsStore.settings.ddns_enable_ipv6 = ddnsIpv6Value.value
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
    notifyTestResult.value = t('testNotifyOk')
  } catch (e) {
    notifyTestOk.value = false
    notifyTestResult.value = String(e)
  } finally {
    notifyTesting.value = false
  }
}

async function syncDdns() {
  ddnsSyncing.value = true
  ddnsSyncResult.value = t('ddnsSyncing')
  try {
    await save() // 先持久化当前输入，后端读的是 settings.json
    ddnsSyncResult.value = await invoke<string>('sync_ddns_now')
    ddnsSyncOk.value = true
  } catch (e) {
    ddnsSyncOk.value = false
    ddnsSyncResult.value = String(e)
  } finally {
    ddnsSyncing.value = false
  }
}
</script>
