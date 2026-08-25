<template>
  <div class="space-y-2">
    <div v-for="(l, i) in model" :key="i" class="rounded-md border border-border p-3">
      <div class="flex items-center gap-2 mb-2">
        <input v-model="l.path" class="input flex-1 font-mono" :placeholder="$t('routePath')" />
        <div class="seg">
          <button :class="l.kind === 'Static' ? 'on' : 'off'" @click="l.kind = 'Static'">{{ $t('typeStatic') }}</button>
          <button :class="l.kind === 'Proxy' ? 'on' : 'off'" @click="l.kind = 'Proxy'">{{ $t('typeProxy') }}</button>
        </div>
        <button class="btn danger" @click="removeAt(i)"><Icon icon="mdi:delete" /></button>
      </div>

      <div v-if="l.kind === 'Static'" class="space-y-2">
        <div class="seg" :class="{ locked }">
          <button
            :class="l.source === 'Dir' ? 'on' : 'off'"
            :disabled="locked"
            :title="locked ? $t('deployTypeLocked') : ''"
            @click="!locked && (l.source = 'Dir')"
          >
            {{ $t('sourceDir') }}
          </button>
          <button
            :class="l.source === 'Upload' ? 'on' : 'off'"
            :disabled="locked"
            :title="locked ? $t('deployTypeLocked') : ''"
            @click="!locked && (l.source = 'Upload')"
          >
            {{ $t('sourceUpload') }}
          </button>
        </div>
        <div class="flex gap-2">
          <input v-model="l.root" class="input flex-1 font-mono" :placeholder="$t('staticRoot')" :readonly="!!l.root && l.source === 'Upload'" :title="l.root && l.source === 'Upload' ? $t('uploadPathReadonly') : ''" />
          <button v-if="l.source === 'Upload'" class="btn" @click="upload(l)"><Icon icon="mdi:upload" /> {{ $t('uploadZip') }}</button>
        </div>
      </div>

      <div v-else class="space-y-1">
        <input v-model="l.target" class="input w-full font-mono" :placeholder="$t('proxyTarget')" />
        <div class="text-[11px] text-muted-foreground">{{ $t('proxyHint') }}</div>

        <div class="border-t border-border pt-2 mt-2">
          <div class="flex items-center justify-between mb-1">
            <span class="text-[11px] text-muted-foreground">{{ $t('upstreamTargets') }}</span>
            <button class="btn sm" @click="addUpstream(l)"><Icon icon="mdi:plus" /> {{ $t('upstreamAdd') }}</button>
          </div>
          <div v-for="(u, ui) in l.upstreams || []" :key="ui" class="flex gap-1 items-center mb-1">
            <input v-model="u.addr" class="input flex-1 font-mono" :placeholder="$t('upstreamAddr')" />
            <button class="btn sm danger" @click="removeUpstream(l, ui)"><Icon icon="mdi:delete" /></button>
          </div>
          <div v-if="(l.upstreams?.length ?? 0) > 1" class="text-[11px] text-muted-foreground">{{ $t('upstreamHint') }}</div>
        </div>

        <input v-model="l.proxy_subpath" class="input w-full font-mono mt-2" :placeholder="$t('subPath')" />
        <div class="text-[11px] text-muted-foreground">{{ $t('subPathHint') }}</div>

        <div class="border-t border-border pt-2 mt-2">
          <div class="flex items-center justify-between mb-1">
            <span class="text-[11px] text-muted-foreground">{{ $t('customHeaders') }}</span>
            <button class="btn sm" @click="addHeader(l)"><Icon icon="mdi:plus" /> {{ $t('headerAdd') }}</button>
          </div>
          <div v-for="(h, hi) in l.proxy_headers || []" :key="hi" class="flex gap-1 items-center mb-1">
            <input v-model="h.name" class="input w-2/5 font-mono" :placeholder="$t('headerName')" />
            <input v-model="h.value" class="input flex-1 font-mono" :placeholder="$t('headerValue')" />
            <button class="btn sm danger" @click="removeHeader(l, hi)"><Icon icon="mdi:delete" /></button>
          </div>
        </div>
      </div>
    </div>

    <button class="btn w-full" @click="add"><Icon icon="mdi:plus" /> {{ $t('addRoute') }}</button>
  </div>
  <Teleport to="body">
    <div v-if="uploadError" class="overlay" style="z-index:75" @click.self="uploadError = ''">
      <div class="confirm-box">
        <div class="confirm-title"><Icon icon="mdi:alert-circle-outline" /></div>
        <p class="confirm-msg">{{ uploadError }}</p>
        <div class="confirm-actions">
          <button class="btn primary" @click="uploadError = ''">{{ $t('confirm') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import type { SiteLocation } from '@/models/website'

const props = defineProps<{ modelValue: SiteLocation[]; siteId: string; locked?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [SiteLocation[]] }>()

const model = props.modelValue
const locked = props.locked ?? false
const uploadError = ref('')

function add() {
  model.push({
    path: '/', kind: 'Static', source: 'Upload', root: '', spa_fallback: true, target: null,
    upstreams: [], proxy_headers: [], proxy_subpath: null,
  })
  emit('update:modelValue', model)
}
function removeAt(i: number) {
  model.splice(i, 1)
  emit('update:modelValue', model)
}
function addUpstream(l: SiteLocation) {
  ;(l.upstreams ??= []).push({ addr: '' })
  emit('update:modelValue', model)
}
function removeUpstream(l: SiteLocation, i: number) {
  l.upstreams?.splice(i, 1)
  emit('update:modelValue', model)
}
function addHeader(l: SiteLocation) {
  ;(l.proxy_headers ??= []).push({ name: '', value: '' })
  emit('update:modelValue', model)
}
function removeHeader(l: SiteLocation, i: number) {
  l.proxy_headers?.splice(i, 1)
  emit('update:modelValue', model)
}

async function upload(l: SiteLocation) {
  const file = await open({ filters: [{ name: 'zip', extensions: ['zip'] }] })
  if (typeof file !== 'string') return
  uploadError.value = ''
  try {
    l.root = await invoke<string>('upload_site_bundle', {
      id: props.siteId,
      locPath: l.path,
      zipPath: file,
    })
  } catch (e) {
    uploadError.value = String(e)
  }
}
</script>

<style scoped>
.seg { display: inline-flex; border: 1px solid var(--color-border); border-radius: 6px; overflow: hidden; }
.seg button { padding: 4px 10px; font-size: 12px; }
.seg .on { background: var(--color-primary); color: var(--color-primary-foreground); }
.seg .off { background: var(--color-card); color: var(--color-muted-foreground); }
.seg.locked { opacity: 0.6; }
.seg button:disabled { cursor: not-allowed; }
.input { height: 32px; padding: 0 10px; background: var(--color-muted); border: 1px solid transparent; border-radius: 6px; color: var(--color-foreground); font-size: 13px; outline: none; }
.input:focus { border-color: var(--color-primary); background: var(--color-card); }
.btn { display: inline-flex; align-items: center; gap: 4px; height: 32px; padding: 0 10px; border-radius: 6px; border: 1px solid var(--color-border); background: var(--color-card); color: var(--color-foreground); font-size: 13px; cursor: pointer; }
.btn.sm { height: 24px; padding: 0 8px; font-size: 12px; }
.btn.danger { color: #e5484d; }
</style>
