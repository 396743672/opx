<template>
  <Teleport to="body">
    <div class="overlay">
      <div class="dialog narrow">
        <div class="dialog-head">
          <div class="dialog-title">
            <Icon icon="mdi:alert-circle" class="warn" /> {{ $t('error') }}
          </div>
          <button class="dialog-close" @click="close">
            <Icon icon="mdi:close" />
          </button>
        </div>
        <div class="error-msg">{{ message }}</div>
        <div class="dialog-footer">
          <button class="btn primary" @click="close">{{ $t('confirm') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'
import { useLifecycleStore } from '../modules/software-manager/stores/lifecycle'

const { t, te } = useI18n()
const lifecycleStore = useLifecycleStore()

// 后端错误可能带 "i18n:key" 标记，且外层可能包前缀（如 "启动失败：i18n:xxx"）。
// 用正则提取 i18n:key 并调 t() 转译；key 不存在或无标记则显示原文（避免暴露裸 key）。
const message = computed(() => {
  const raw = lifecycleStore.errorMessage
  if (!raw) return ''
  const m = raw.match(/i18n:([A-Za-z0-9_.-]+)/)
  return m && te(m[1]) ? t(m[1]) : raw
})

function close() {
  lifecycleStore.clearErrorMessage()
}
</script>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 60;
  display: flex;
  align-items: center;
  justify-content: center;
  background: oklch(0 0 0 / 0.5);
  backdrop-filter: blur(4px);
}
.dialog {
  width: 420px;
  border-radius: 10px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  box-shadow: 0 8px 24px oklch(0 0 0 / 0.45);
  padding: 20px;
}
.dialog-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 14px;
}
.dialog-title {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 15px;
  font-weight: 600;
}
.dialog-title .warn {
  color: var(--color-destructive);
}
.dialog-title svg {
  width: 22px;
  height: 22px;
}
.dialog-close {
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--color-muted-foreground);
  border-radius: 4px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}
.dialog-close:hover {
  background: var(--color-muted);
  color: var(--color-foreground);
}
.error-msg {
  font-size: 13px;
  color: var(--color-foreground);
  padding: 12px;
  border-radius: 6px;
  background: color-mix(in oklch, var(--color-destructive) 8%, transparent);
  border: 1px solid color-mix(in oklch, var(--color-destructive) 25%, transparent);
  margin-bottom: 16px;
  white-space: pre-wrap;
  word-break: break-all;
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 12px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  color: var(--color-foreground);
}
.btn:hover {
  background: var(--color-muted);
}
.btn.primary {
  background: var(--color-primary);
  color: var(--color-primary-foreground);
  border-color: var(--color-primary);
}
</style>
