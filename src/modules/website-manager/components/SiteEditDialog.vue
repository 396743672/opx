<template>
  <Teleport to="body">
    <div class="overlay">
      <div class="dialog">
        <div class="head">
          <div class="title"><Icon icon="mdi:web" /> {{ form.name || $t('newSite') }}</div>
          <button class="x" @click="$emit('close')"><Icon icon="mdi:close" /></button>
        </div>

        <div class="body">
          <label class="lbl">{{ $t('siteName') }}</label>
          <input v-model="form.name" class="input w-full mb-3" />

          <div class="flex gap-3 mb-3">
            <div class="flex-1">
              <label class="lbl">{{ $t('serverNameLabel') }}</label>
              <input v-model="form.server_name" class="input w-full font-mono" placeholder="app.demo.com" />
              <div class="hint">{{ $t('serverNameHint') }}</div>
            </div>
            <div style="width:130px">
              <label class="lbl">{{ $t('listenPort') }}</label>
              <input v-model.number="form.listen" type="number" class="input w-full font-mono" />
            </div>
          </div>

          <label class="flex items-center gap-2 mb-3 opacity-50 cursor-not-allowed">
            <input type="checkbox" disabled />
            <span class="text-sm">{{ $t('httpsReserved') }}</span>
          </label>

          <label class="lbl">{{ $t('routeRules') }}</label>
          <LocationEditor v-model="form.locations" :site-id="form.id" />
        </div>

        <div class="foot">
          <button class="btn" @click="$emit('close')">{{ $t('cancel') }}</button>
          <button class="btn" :disabled="saving" @click="save(false)">{{ $t('save') }}</button>
          <button class="btn primary" :disabled="saving" @click="save(true)">{{ $t('saveAndApply') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import LocationEditor from './LocationEditor.vue'
import type { Site } from '@/models/website'

const props = defineProps<{ site: Site }>()
const emit = defineEmits<{ close: []; saved: [] }>()

const form = ref<Site>(props.site)
const saving = ref(false)

async function save(apply: boolean) {
  saving.value = true
  try {
    await invoke('save_website', { site: form.value, apply })
    emit('saved')
  } catch (e) {
    window.alert(String(e))
  } finally {
    saving.value = false
  }
}
</script>

<style scoped>
.overlay { position: fixed; inset: 0; z-index: 50; display: flex; align-items: center; justify-content: center; background: oklch(0 0 0 / 0.5); backdrop-filter: blur(4px); }
.dialog { width: 640px; max-height: 90vh; overflow-y: auto; border-radius: 10px; border: 1px solid var(--color-border); background: var(--color-card); box-shadow: 0 8px 24px oklch(0 0 0 / 0.45); }
.head { display: flex; justify-content: space-between; align-items: center; padding: 14px 18px; border-bottom: 1px solid var(--color-border); }
.title { font-weight: 600; display: flex; gap: 8px; align-items: center; }
.title svg { color: var(--color-primary); }
.x { border: none; background: transparent; color: var(--color-muted-foreground); cursor: pointer; }
.body { padding: 16px 18px; }
.foot { display: flex; justify-content: flex-end; gap: 8px; padding: 12px 18px; border-top: 1px solid var(--color-border); }
.lbl { display: block; font-size: 12px; color: var(--color-muted-foreground); margin-bottom: 4px; }
.hint { font-size: 11px; color: var(--color-muted-foreground); margin-top: 3px; }
.input { height: 32px; padding: 0 10px; background: var(--color-muted); border: 1px solid transparent; border-radius: 6px; color: var(--color-foreground); font-size: 13px; outline: none; box-sizing: border-box; }
.input:focus { border-color: var(--color-primary); background: var(--color-card); }
.btn { display: inline-flex; align-items: center; gap: 6px; height: 32px; padding: 0 12px; border-radius: 6px; border: 1px solid var(--color-border); background: var(--color-card); color: var(--color-foreground); font-size: 13px; cursor: pointer; }
.btn.primary { background: var(--color-primary); color: var(--color-primary-foreground); border-color: var(--color-primary); }
.btn:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
