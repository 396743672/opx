<!-- Web 模式访问令牌输入页（批次 4.4，设计 D3）：无 token 时全屏遮罩，
     输入桌面端「Web 管理」设置区展示的 64 位访问令牌后整页重载走干净启动。 -->
<template>
  <div class="fixed inset-0 z-[70] flex flex-col items-center justify-center bg-background px-6">
    <div class="w-full max-w-sm space-y-4 text-center">
      <div
        class="mx-auto w-14 h-14 rounded-2xl bg-primary/10 text-primary flex items-center justify-center"
      >
        <Icon icon="mdi:key-outline" class="text-3xl" />
      </div>
      <h1 class="text-lg font-semibold tracking-tight">{{ $t('webTokenTitle') }}</h1>
      <p class="text-sm text-muted-foreground leading-relaxed">{{ $t('webTokenDesc') }}</p>
      <input
        v-model="token"
        type="password"
        autocomplete="off"
        spellcheck="false"
        class="w-full rounded-lg border border-border bg-card px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/40"
        :placeholder="$t('webTokenPlaceholder')"
        @keydown.enter="submit"
      />
      <p v-if="error" class="text-xs text-destructive">{{ error }}</p>
      <button class="btn primary w-full justify-center" :disabled="!token.trim()" @click="submit">
        {{ $t('webTokenConfirm') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { Icon } from '@iconify/vue'
import { getTransport } from '@/utils/transport'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
const token = ref('')
const error = ref('')

function submit() {
  const v = token.value.trim()
  if (!v) return
  // 形态预检：token 为 64 位 hex（32 字节 CSPRNG），明显非法直接提示，避免无效往返；
  // 真实有效性（是否被拒）由首个 API 调用/WS 握手的 401 路径经 onUnauthorized 反馈
  if (!/^[0-9a-fA-F]{64}$/.test(v)) {
    error.value = t('webTokenInvalid')
    return
  }
  getTransport().setToken(v)
  // 整页重载：让启动流程（settings/轮询/监听）在持有 token 的状态下干净重跑
  window.location.reload()
}
</script>
