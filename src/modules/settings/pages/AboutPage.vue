<template>
  <div class="animate-fade-in">
    <PageHeader icon="mdi:information-outline" :title="$t('about')">
      <template #actions>
        <!-- 桌面专属（更新器走 Tauri updater）：Web 模式隐藏入口 -->
        <button v-if="isDesktop" class="btn" :disabled="checking || installing" @click="runCheck">
          <Icon
            :icon="checking ? 'mdi:loading' : 'mdi:update'"
            :class="{ spinning: checking }"
          />
          {{ checking ? $t('checkingUpdate') : $t('checkUpdate') }}
        </button>
      </template>
    </PageHeader>

    <div class="max-w-2xl space-y-4">
      <!-- 应用信息 -->
      <div class="rounded-xl border border-border bg-card p-6 flex items-start gap-4">
        <div
          class="w-12 h-12 flex-shrink-0 rounded-xl flex items-center justify-center bg-primary/10 text-primary"
        >
          <Icon icon="mdi:package-variant-closed" class="text-2xl" />
        </div>
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-2 flex-wrap">
            <h2 class="text-lg font-semibold tracking-tight">OPX</h2>
            <!-- app_version 为桌面专属命令：Web 模式不展示版本徽标 -->
            <span
              v-if="isDesktop"
              class="text-xs px-2 py-0.5 rounded-full bg-muted text-muted-foreground font-medium"
            >v{{ version }}</span>
            <!-- 便携版徽标：应用内更新不可用（updater 只能交付安装器，会装出第二份副本） -->
            <span
              v-if="isDesktop && portable"
              class="text-xs px-2 py-0.5 rounded-full bg-amber-500/15 text-amber-600 dark:text-amber-400 font-medium"
            >{{ $t('portableBadge') }}</span>
          </div>
          <p class="text-sm text-muted-foreground mt-1.5 leading-relaxed">
            {{ $t('aboutDescription') }}
          </p>
        </div>
      </div>

      <!-- 便携版常驻提示：无新版时也告知「应用内更新不可用」，避免用户白点 -->
      <div
        v-if="isDesktop && portable && !updateInfo"
        class="rounded-xl border border-border bg-card p-5"
      >
        <div class="flex items-start gap-3">
          <Icon icon="mdi:package-variant-closed" class="text-lg mt-0.5 flex-shrink-0 text-muted-foreground" />
          <p class="text-xs text-muted-foreground leading-relaxed">{{ $t('portableUpdateHint') }}</p>
        </div>
      </div>

      <!-- 可用更新 -->
      <div v-if="updateInfo" class="rounded-xl border border-border bg-card p-5 space-y-3">
        <div class="text-sm font-medium" :class="portable ? 'text-foreground' : 'text-success'">
          {{
            portable
              ? $t('updateAvailablePortable', { version: updateInfo.version })
              : $t('updateAvailable', { version: updateInfo.version })
          }}
        </div>
        <p
          v-if="updateInfo.notes"
          class="text-xs text-muted-foreground whitespace-pre-line"
        >{{ updateInfo.notes }}</p>

        <!-- 便携版：不触发应用内安装，引导手动下载便携包覆盖 opx.exe -->
        <template v-if="portable">
          <p class="text-xs text-muted-foreground leading-relaxed">{{ $t('portableUpdateHint') }}</p>
          <button class="btn primary" @click="openPortableDownload">
            <Icon icon="mdi:download" />
            {{ $t('downloadPortable') }}
          </button>
        </template>

        <!-- 安装版：走 Tauri updater -->
        <div v-else class="flex items-center gap-3">
          <button class="btn primary" :disabled="installing" @click="runInstall">
            <Icon
              :icon="installing ? 'mdi:loading' : 'mdi:download'"
              :class="{ spinning: installing }"
            />
            {{ installing ? $t('installingUpdate') : $t('installUpdate') }}
          </button>
          <div v-if="installing" class="flex-1 flex items-center gap-2 min-w-0">
            <div class="flex-1 h-1.5 rounded-full bg-muted overflow-hidden">
              <div class="h-full bg-primary transition-all" :style="{ width: progress + '%' }"></div>
            </div>
            <span class="text-xs text-muted-foreground tabular-nums">{{ progress }}%</span>
          </div>
        </div>
      </div>

      <!-- 链接与技术信息 -->
      <div class="rounded-xl border border-border bg-card divide-y divide-border">
        <button
          class="w-full flex items-center gap-3 px-5 py-3.5 text-left hover:bg-muted/50 transition-colors"
          @click="openRepo"
        >
          <Icon icon="mdi:github" class="text-xl flex-shrink-0 text-muted-foreground" />
          <span class="flex-1 text-sm">{{ $t('aboutRepo') }}</span>
          <span class="text-xs text-muted-foreground">{{ repoLabel }}</span>
          <Icon icon="mdi:open-in-new" class="text-base text-muted-foreground" />
        </button>
        <div class="flex items-center gap-3 px-5 py-3.5">
          <Icon icon="mdi:tools" class="text-xl flex-shrink-0 text-muted-foreground" />
          <span class="flex-1 text-sm">{{ $t('aboutTech') }}</span>
          <span class="text-xs text-muted-foreground">Tauri 2 · Vue 3 · Rust</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@/utils/ipc'
import { getTransport } from '@/utils/transport'
import { openUrl } from '@tauri-apps/plugin-opener'
import PageHeader from '@/components/PageHeader.vue'
import { useUpdater } from '@/composables/useUpdater'

const REPO_URL = 'https://github.com/396743672/opx'
const repoLabel = REPO_URL.replace('https://', '')
// 便携版手动更新入口： Releases 页列出自便携 zip 在内的全部产物
const PORTABLE_DOWNLOAD_URL = 'https://github.com/396743672/opx/releases/latest'

// 桌面专属入口隐藏依据（批次 4.4）：更新器 / app_version / 系统打开器
const isDesktop = getTransport().isDesktop

const version = ref('—')

const {
  checking,
  installing,
  updateInfo,
  portable,
  progress,
  check: runCheck,
  install: runInstall,
  loadPortable,
} = useUpdater()

function openRepo() {
  // Web 模式没有 opener 插件，退化为浏览器新开标签页
  if (!isDesktop) {
    window.open(REPO_URL, '_blank', 'noopener')
    return
  }
  openUrl(REPO_URL).catch(() => {})
}

function openPortableDownload() {
  if (!isDesktop) {
    window.open(PORTABLE_DOWNLOAD_URL, '_blank', 'noopener')
    return
  }
  openUrl(PORTABLE_DOWNLOAD_URL).catch(() => {})
}

onMounted(async () => {
  // 便携标识与网络无关，先取（决定徽标与更新区形态）
  loadPortable()
  try {
    version.value = await invoke<string>('app_version')
  } catch {
    version.value = '—'
  }
})
</script>
