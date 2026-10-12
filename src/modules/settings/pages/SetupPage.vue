<template>
  <div class="min-h-full flex items-start xl:items-center justify-center p-6">
    <div class="w-full max-w-xl border border-border bg-card rounded-2xl shadow-card p-8">
      <!-- 头部 -->
      <div class="flex items-center gap-3 mb-1">
        <div class="w-10 h-10 rounded-xl bg-primary/10 text-primary flex items-center justify-center">
          <Icon icon="mdi:rocket-launch-outline" class="text-xl" />
        </div>
        <div>
          <h1 class="text-lg font-semibold tracking-tight">{{ $t('setup') }}</h1>
          <p class="text-xs text-muted-foreground">{{ $t('setupSubtitle') }}</p>
        </div>
      </div>

      <!-- 语言 -->
      <div class="mt-7">
        <div class="text-sm font-medium mb-2">{{ $t('setupLanguage') }}</div>
        <div class="flex gap-2">
          <button
            v-for="l in langs"
            :key="l.value"
            class="btn"
            :class="{ primary: language === l.value }"
            type="button"
            @click="settingsStore.setLanguage(l.value)"
          >
            {{ l.label }}
          </button>
        </div>
      </div>

      <!-- 端口 -->
      <div class="mt-6">
        <div class="text-sm font-medium mb-2">{{ $t('setupPort') }}</div>
        <div class="flex items-center gap-2">
          <input
            v-model.number="portValue"
            type="number"
            min="1"
            max="65535"
            class="h-8 px-2 w-32 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
            :placeholder="String(settings?.web_port ?? 17580)"
          />
          <button class="btn" type="button" :disabled="checking" @click="checkPort">
            <Icon :icon="checking ? 'mdi:loading' : 'mdi:magnify'" :class="{ spinning: checking }" />
            {{ checking ? $t('setupChecking') : $t('setupCheck') }}
          </button>
          <span v-if="portCheck === 'free'" class="text-xs text-success inline-flex items-center gap-1">
            <Icon icon="mdi:check-circle" /> {{ $t('setupPortFree') }}
          </span>
          <span v-else-if="portCheck === 'busy'" class="text-xs text-destructive inline-flex items-center gap-1">
            <Icon icon="mdi:alert-circle" /> {{ $t('setupPortBusy') }}
          </span>
        </div>
        <p class="text-xs text-muted-foreground mt-2">{{ $t('setupPortHint') }}</p>
      </div>

      <!-- 局域网 -->
      <div class="mt-6">
        <div class="flex items-center gap-3">
          <span class="text-sm font-medium">{{ $t('setupLan') }}</span>
          <SwitchBtn v-model="lanValue" />
        </div>
        <p class="text-xs text-muted-foreground mt-2">{{ $t('webLanAccessHint') }}</p>
        <p v-if="lanValue" class="text-xs text-destructive mt-1.5">{{ $t('setupLanRisk') }}</p>
        <p v-else class="text-xs text-muted-foreground mt-1.5 mono">{{ $t('setupLanOffHint') }}</p>
      </div>

      <!-- 动作 -->
      <div class="flex items-center justify-between mt-8 pt-5 border-t border-border">
        <button class="btn ghost" type="button" @click="skip">{{ $t('setupSkip') }}</button>
        <button class="btn primary" type="button" :disabled="finishing" @click="finish">
          <Icon :icon="finishing ? 'mdi:loading' : 'mdi:check'" :class="{ spinning: finishing }" />
          {{ $t('setupFinish') }}
        </button>
      </div>

      <!-- 入口地址变更面板：origin 变了，旧页面 sessionStorage 里的 token 不随迁，
           必须以 #/?token= fragment 把 token 送到新 origin（TokenGate 会收录） -->
      <div v-if="newUrl" class="mt-6 border border-border rounded-xl bg-muted/40 p-4">
        <div class="flex items-center gap-2 text-sm font-medium mb-1">
          <Icon icon="mdi:web-refresh" class="text-primary" />
          {{ $t('setupNewAddr') }}
        </div>
        <p class="text-xs text-muted-foreground mb-2">{{ $t('setupNewAddrHint') }}</p>
        <div class="flex items-center gap-2">
          <code class="flex-1 text-xs bg-background border border-border rounded-lg px-2.5 py-1.5 truncate">{{ newUrl }}</code>
          <button class="btn" type="button" @click="copyNewUrl">
            <Icon :icon="copied ? 'mdi:check' : 'mdi:content-copy'" />
            {{ copied ? $t('setupCopied') : $t('setupCopy') }}
          </button>
          <a class="btn primary" :href="newUrl">{{ $t('setupOpen') }}</a>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { Icon } from '@iconify/vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import SwitchBtn from '@/components/SwitchBtn.vue'
import { useSettingsStore } from '@/stores/settings'
import { invoke } from '@/utils/ipc'
import { isDesktop, getStoredToken } from '@/utils/transport'
import { toast } from '@/composables/useToast'
import type { Language } from '@/models/settings'

const settingsStore = useSettingsStore()
const router = useRouter()
const { t } = useI18n()

const langs: { value: Language; label: string }[] = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'en-US', label: 'English' },
]

const language = computed(() => settingsStore.language)
const settings = computed(() => settingsStore.settings)

// 初值取自后端（headless 种子已把 lan 置 true；桌面默认 false）
const portValue = ref(17580)
const lanValue = ref(false)
const origPort = ref(17580)
const origLan = ref(false)

const checking = ref(false)
const portCheck = ref<'unknown' | 'free' | 'busy'>('unknown')
const finishing = ref(false)
const newUrl = ref('')
const copied = ref(false)

onMounted(() => {
  const s = settingsStore.settings
  if (s) {
    portValue.value = s.web_port ?? 17580
    lanValue.value = s.web_lan_access ?? false
    origPort.value = portValue.value
    origLan.value = lanValue.value
  }
})

function normPort(): number {
  const p = Number(portValue.value)
  return p >= 1 && p <= 65535 ? p : 17580
}

async function checkPort() {
  checking.value = true
  portCheck.value = 'unknown'
  try {
    const port = normPort()
    // 当前配置端口即自身 Web 入口（必然被自己绑定），视为可用
    if (port === (settings.value?.web_port ?? 17580)) {
      portCheck.value = 'free'
      return
    }
    const free = await invoke<boolean>('check_port_free', { port, lan: lanValue.value })
    portCheck.value = free ? 'free' : 'busy'
  } catch {
    portCheck.value = 'unknown'
  } finally {
    checking.value = false
  }
}

async function persistDone(): Promise<void> {
  const s = settingsStore.settings
  if (!s) return
  s.web_port = normPort()
  s.web_lan_access = lanValue.value
  s.first_run_done = true
  await settingsStore.saveSettings()
}

async function finish() {
  finishing.value = true
  try {
    const port = normPort()
    // 当前配置端口即自身入口（必然被自己绑定），视为可用
    if (port !== (settings.value?.web_port ?? 17580)) {
      // 保存前预检（安装引导设计 §5：真绑即真知）
      const free = await invoke<boolean>('check_port_free', { port, lan: lanValue.value })
      if (!free) {
        portCheck.value = 'busy'
        toast(t('setupPortBusy'), 'err')
        return
      }
    }
    await persistDone()
    const changed = port !== origPort.value || lanValue.value !== origLan.value
    if (isDesktop || !changed) {
      router.replace('/')
      return
    }
    // origin 变化：拼新地址（LAN 关闭后远程入口失效，退回 127.0.0.1 并提示隧道）
    const host = lanValue.value ? location.hostname : '127.0.0.1'
    newUrl.value = `${location.protocol}//${host}:${port}/#/?token=${getStoredToken() ?? ''}`
  } finally {
    finishing.value = false
  }
}

async function skip() {
  await persistDone()
  router.replace('/')
}

async function copyNewUrl() {
  try {
    await navigator.clipboard.writeText(newUrl.value)
    copied.value = true
    window.setTimeout(() => (copied.value = false), 1500)
  } catch {
    toast(t('setupCopyFailed'), 'err')
  }
}
</script>
