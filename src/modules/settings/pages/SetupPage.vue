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

      <!-- 浏览器管理入口：桌面首启的核心决策（headless 恒开，不显示开关） -->
      <div v-if="isDesktop" class="mt-6">
        <div class="flex items-center gap-3">
          <span class="text-sm font-medium">{{ $t('setupWebEntry') }}</span>
          <SwitchBtn v-model="webEnabledValue" />
        </div>
        <p class="text-xs text-muted-foreground mt-2">{{ $t('webEnabledHint') }}</p>
      </div>

      <!-- 端口（桌面关闭 web 时无意义，随之隐藏） -->
      <div v-show="showNet" class="mt-6">
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
      <div v-show="showNet" class="mt-6">
        <div class="flex items-center gap-3">
          <span class="text-sm font-medium">{{ $t('setupLan') }}</span>
          <SwitchBtn v-model="lanValue" />
        </div>
        <p class="text-xs text-muted-foreground mt-2">{{ $t('webLanAccessHint') }}</p>
        <p v-if="lanValue" class="text-xs text-destructive mt-1.5">{{ $t('setupLanRisk') }}</p>
        <p v-else class="text-xs text-muted-foreground mt-1.5 mono">{{ $t('setupLanOffHint') }}</p>
        <!-- 防火墙放行状态：Linux 上「开了局域网访问还是连不上」几乎都卡在这一层 -->
        <p
          v-if="lanValue && fwHint"
          class="text-xs mt-1.5 inline-flex items-center gap-1.5"
          :class="fwOk ? 'text-success' : 'text-amber-600'"
        >
          <Icon :icon="fwOk ? 'mdi:check-circle' : 'mdi:alert-outline'" />
          {{ fwHint }}
        </p>
      </div>

      <!-- 动作 -->
      <div class="flex items-center justify-between mt-8 pt-5 border-t border-border">
        <button class="btn ghost" type="button" @click="skip">{{ $t('setupSkip') }}</button>
        <button class="btn primary" type="button" :disabled="finishing" @click="finish">
          <Icon :icon="finishing ? 'mdi:loading' : 'mdi:check'" :class="{ spinning: finishing }" />
          {{ $t('setupFinish') }}
        </button>
      </div>

      <!-- 地址面板：桌面=刚开启浏览器入口（后端生成含 token 完整 URL，supervisor
           ~2s 内热启动）；headless=origin 变化（sessionStorage 不随迁，token 走
           #/?token= fragment 送到新 origin，TokenGate 会收录） -->
      <div v-if="newUrls.length" class="mt-6 border border-border rounded-xl bg-muted/40 p-4">
        <template v-if="webJustEnabled">
          <div class="flex items-center gap-2 text-sm font-medium mb-1">
            <Icon icon="mdi:web-check" class="text-primary" />
            {{ $t('setupWebReady') }}
          </div>
          <p class="text-xs text-muted-foreground mb-2">{{ $t('setupWebReadyHint') }}</p>
        </template>
        <template v-else>
          <div class="flex items-center gap-2 text-sm font-medium mb-1">
            <Icon icon="mdi:web-refresh" class="text-primary" />
            {{ $t('setupNewAddr') }}
          </div>
          <p class="text-xs text-muted-foreground mb-2">{{ $t('setupNewAddrHint') }}</p>
        </template>
        <div class="space-y-1.5">
          <div v-for="u in newUrls" :key="u" class="flex items-center gap-2">
            <code class="flex-1 text-xs bg-background border border-border rounded-lg px-2.5 py-1.5 truncate">{{ u }}</code>
          </div>
        </div>
        <div class="flex items-center gap-2 mt-2.5">
          <button class="btn" type="button" @click="copyUrls">
            <Icon :icon="copied ? 'mdi:check' : 'mdi:content-copy'" />
            {{ copied ? $t('setupCopied') : $t('setupCopy') }}
          </button>
          <a v-if="newUrls[0]" class="btn primary" :href="newUrls[0]" target="_blank" rel="noopener">
            {{ $t('setupOpen') }}
          </a>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue'
import { Icon } from '@iconify/vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import SwitchBtn from '@/components/SwitchBtn.vue'
import { useSettingsStore } from '@/stores/settings'
import { invoke } from '@/utils/ipc'
import { isDesktop, getStoredToken } from '@/utils/transport'
import { toast } from '@/composables/useToast'
import type { Language } from '@/models/settings'
import type { FirewallOverview } from '@/models/firewall'

const settingsStore = useSettingsStore()
const router = useRouter()
const { t } = useI18n()

const langs: { value: Language; label: string }[] = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'en-US', label: 'English' },
]

const language = computed(() => settingsStore.language)
const settings = computed(() => settingsStore.settings)

// 初值取自后端（headless 种子已把 lan 置 true；桌面 web 默认关）
const portValue = ref(17580)
const lanValue = ref(false)
const webEnabledValue = ref(false)
const origPort = ref(17580)
const origLan = ref(false)
const origWebEnabled = ref(false)

// 桌面关闭浏览器入口时，端口/局域网两节无意义，随之隐藏
const showNet = computed(() => (isDesktop ? webEnabledValue.value : true))

const checking = ref(false)
const portCheck = ref<'unknown' | 'free' | 'busy'>('unknown')
const finishing = ref(false)
const newUrls = ref<string[]>([])
const webJustEnabled = ref(false)
const copied = ref(false)

// 防火墙放行状态：null = 尚未查到（或查询失败，此时不显示任何提示，避免误报）
const fwAvailable = ref<boolean | null>(null)
const fwOk = ref(false)

const fwHint = computed(() => {
  if (fwAvailable.value === null) return ''
  const port = normPort()
  if (!fwAvailable.value) return t('setupFwNoBackend', { port })
  return fwOk.value ? t('setupFwAllowed', { port }) : t('setupFwNotAllowed', { port })
})

/** 查当前端口在系统防火墙里的真实状态（幂等只读，失败静默——引导页不该被它挡住） */
async function refreshFirewall() {
  try {
    const ov = await invoke<FirewallOverview>('get_firewall_overview')
    fwAvailable.value = ov.backend.available
    const port = normPort()
    fwOk.value = ov.ports.some((p) => p.source === 'opx' && p.port === port && p.allowed)
  } catch {
    fwAvailable.value = null
  }
}

onMounted(() => {
  const s = settingsStore.settings
  if (s) {
    portValue.value = s.web_port ?? 17580
    lanValue.value = s.web_lan_access ?? false
    webEnabledValue.value = s.web_enabled ?? false
    origPort.value = portValue.value
    origLan.value = lanValue.value
    origWebEnabled.value = webEnabledValue.value
  }
  void refreshFirewall()
})

// 改端口要重新查：默认端口已放行、换成 8080 后状态就变了
watch(portValue, () => {
  void refreshFirewall()
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
  s.web_enabled = webEnabledValue.value
  s.first_run_done = true
  await settingsStore.saveSettings()
}

async function finish() {
  finishing.value = true
  try {
    const port = normPort()
    // 保存前预检（安装引导设计 §5：真绑即真知）。桌面关闭 web 时无网络面，跳过
    if (showNet.value && port !== (settings.value?.web_port ?? 17580)) {
      const free = await invoke<boolean>('check_port_free', { port, lan: lanValue.value })
      if (!free) {
        portCheck.value = 'busy'
        toast(t('setupPortBusy'), 'err')
        return
      }
    }
    await persistDone()

    // 用户在这里显式开了局域网访问 → 顺手把端口放行掉。
    // 不放行的话「开了局域网还是连不上」是 Linux 上最常见的困惑；
    // 失败不阻断引导（用户可到「端口与防火墙」页重试）。
    if (showNet.value && lanValue.value) {
      try {
        await invoke('set_firewall_port', { port: normPort(), proto: 'tcp', allow: true })
      } catch {
        /* 非致命：跳过 */
      }
    }

    // 桌面：刚开启浏览器入口 → 后端生成含 token 的完整 URL 面板
    if (isDesktop) {
      if (webEnabledValue.value && !origWebEnabled.value) {
        newUrls.value = await invoke<string[]>('get_web_access_urls')
        webJustEnabled.value = true
      } else {
        router.replace('/')
      }
      return
    }

    // headless：origin 变化（端口/局域网）→ 新地址接力（sessionStorage 不随迁）
    const changed = port !== origPort.value || lanValue.value !== origLan.value
    if (!changed) {
      router.replace('/')
      return
    }
    // 局域网关闭后远程入口失效，退回 127.0.0.1 并提示隧道
    const host = lanValue.value ? location.hostname : '127.0.0.1'
    newUrls.value = [`${location.protocol}//${host}:${port}/#/?token=${getStoredToken() ?? ''}`]
    webJustEnabled.value = false
  } finally {
    finishing.value = false
  }
}

async function skip() {
  await persistDone()
  router.replace('/')
}

async function copyUrls() {
  try {
    await navigator.clipboard.writeText(newUrls.value.join('\n'))
    copied.value = true
    window.setTimeout(() => (copied.value = false), 1500)
  } catch {
    toast(t('setupCopyFailed'), 'err')
  }
}
</script>
