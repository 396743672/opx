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
          <!-- 桌面端：原生文件对话框；Web 端：浏览器没有本机路径，只能 <input type=file> 读成 base64 传给后端 -->
          <button v-if="l.source === 'Upload' && isDesktop" class="btn" @click="upload(l)"><Icon icon="mdi:upload" /> {{ $t('uploadZip') }}</button>
          <label v-else-if="l.source === 'Upload'" class="btn upload-btn" :class="{ disabled: !!uploading }">
            <Icon icon="mdi:upload" /> {{ uploading ? $t('uploading') : $t('uploadZip') }}
            <input type="file" accept=".zip,application/zip" class="file-input" :disabled="!!uploading" @change="onWebUpload(l, $event)" />
          </label>
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
import { invoke } from '@/utils/ipc'
import { fileToBase64 } from '@/utils/file'
import { open } from '@tauri-apps/plugin-dialog'
import { isDesktop } from '@/utils/transport'
import type { SiteLocation } from '@/models/website'

const props = defineProps<{ modelValue: SiteLocation[]; siteId: string; locked?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [SiteLocation[]] }>()

const model = props.modelValue
const locked = props.locked ?? false
const uploadError = ref('')
/** Web 端上传中：用于禁用按钮与文案反馈（zip 可能几十 MB，读入+传输有可感知耗时） */
const uploading = ref(false)

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

/**
 * Web 端上传：浏览器给不出本机路径，读成 base64 交后端落盘。
 *
 * 后端仍返回同一个 `_pending_/…zip` 标记，解压与换 root 由「保存站点」时的
 * `save_website` 统一完成——两条上传路径的落点完全一致，不会出现行为差异。
 */
async function onWebUpload(l: SiteLocation, e: Event) {
  const input = e.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return
  uploadError.value = ''
  uploading.value = true
  try {
    const dataBase64 = await fileToBase64(file)
    l.root = await invoke<string>('upload_site_bundle_data', {
      id: props.siteId,
      locPath: l.path,
      fileName: file.name,
      dataBase64,
    })
    emit('update:modelValue', model)
  } catch (err) {
    uploadError.value = typeof err === 'string' ? err : String(err)
  } finally {
    uploading.value = false
    input.value = '' // 允许重复选同一文件
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
/* Web 端上传：<label> 包住隐藏 input，点击 label 即触发文件选择 */
.upload-btn.disabled { opacity: 0.6; cursor: not-allowed; }
.file-input { display: none; }
</style>
