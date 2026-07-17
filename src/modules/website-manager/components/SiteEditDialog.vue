<template>
  <Teleport to="body">
    <div class="overlay">
      <div class="dialog">
        <div class="head">
          <div class="title"><Icon icon="mdi:web" /> {{ form.name || $t('newSite') }}</div>
          <button class="x" @click="$emit('close')"><Icon icon="mdi:close" /></button>
        </div>

        <div class="tab-bar">
          <button :class="{ active: tab === 'form' }" @click="tab = 'form'">{{ $t('tabForm') }}</button>
          <button :class="{ active: tab === 'source' }" :disabled="isNew" @click="!isNew && (tab = 'source')">
            {{ $t('tabSource') }}
          </button>
        </div>

        <div class="body" v-if="tab === 'form'">
          <div v-if="site.custom_conf" class="locked-tip">
            <Icon icon="mdi:information-outline" /> {{ $t('customConfLocked') }}
          </div>
          <label class="lbl">{{ $t('siteName') }} <span class="text-destructive">*</span></label>
          <input v-model="form.name" class="input w-full mb-1" :class="{ 'border-destructive': nameError }" @input="nameError = ''" />
          <div v-if="nameError" class="text-xs text-destructive mb-3">{{ nameError }}</div>
          <div v-else class="text-xs hint mb-3">{{ $t('siteNameHint') }}</div>

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
          <LocationEditor v-model="form.locations" :site-id="form.id" :locked="!isNew" />
        </div>

        <div class="body source-view" v-else>
          <div v-if="site.custom_conf" class="locked-tip between">
            <span><Icon icon="mdi:code-tags" /> {{ $t('customConfLocked') }}</span>
            <button class="btn sm" @click="unlock">{{ $t('restoreForm') }}</button>
          </div>
          <textarea ref="sourceText" spellcheck="false" class="source-input" @input="sourceDirty = true"></textarea>
        </div>

        <div v-if="saveError" class="error-banner">{{ saveError }}</div>
        <div class="foot">
          <button class="btn" @click="$emit('close')">{{ $t('cancel') }}</button>
          <button
            class="btn primary"
            :disabled="saving || (tab === 'form' && !!site.custom_conf)"
            :title="tab === 'form' && site.custom_conf ? $t('customConfLocked') : ''"
            @click="save()"
          >
            {{ $t('save') }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
  <Teleport to="body">
    <div v-if="confirmUnlock" class="overlay" style="z-index:60">
      <div class="confirm-box">
        <div class="confirm-title"><Icon icon="mdi:alert-circle-outline" /> 确认操作</div>
        <p class="confirm-msg">{{ $t('restoreFormConfirm') }}</p>
        <div class="confirm-actions">
          <button class="btn" @click="confirmUnlock = false">{{ $t('cancel') }}</button>
          <button class="btn danger" @click="doUnlock">{{ $t('confirm') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import LocationEditor from './LocationEditor.vue'
import type { Site, SiteLocation } from '@/models/website'

const { t } = useI18n()
const props = withDefaults(defineProps<{ site: Site; isNew?: boolean }>(), { isNew: false })
const emit = defineEmits<{ close: []; saved: [] }>()

const form = ref<Site>(props.site)
const saving = ref(false)
const saveError = ref('')
const tab = ref<'form' | 'source'>('form')
const sourceText = ref<HTMLTextAreaElement | null>(null)
const sourceDirty = ref(false)
const nameError = ref('')
const confirmUnlock = ref(false)

function validateName(): boolean {
  const v = form.value.name.trim()
  if (!v) {
    nameError.value = t('siteNameRequired')
    return false
  }
  if (/[\u4e00-\u9fff\u3400-\u4dbf]/.test(v)) {
    nameError.value = t('siteNameNoCJK')
    return false
  }
  nameError.value = ''
  return true
}

async function save() {
  if (tab.value === 'form' && !validateName()) return
  saving.value = true
  try {
    if (tab.value === 'source') {
      await invoke('set_site_conf', {
        id: props.site.id,
        content: sourceText.value?.value ?? '',
      })
    } else {
      // 保存即生效：nginx 运行中时后端自动校验并 reload
      await invoke('save_website', { site: form.value })
    }
    emit('saved')
  } catch (e) {
    saveError.value = String(e)
  } finally {
    saving.value = false
  }
}

// 解除手写模式：恢复表单生成
function unlock() {
  confirmUnlock.value = true
}
async function doUnlock() {
  confirmUnlock.value = false
  saving.value = true
  saveError.value = ''
  try {
    await invoke('unlock_site_conf', { id: props.site.id })
    emit('saved')
  } catch (e) {
    saveError.value = String(e)
  } finally {
    saving.value = false
  }
}

// 新建站点：从表单数据生成 nginx 配置预览，跳过后端读取（尚未落库）
function generateNginxPreview(s: Site): string {
  const serverName = s.server_name?.trim() || '_'
  let out = `# 配置预览（来源于表单数据，保存后写入文件）\nserver {\n`
  out += `    listen ${s.listen};\n`
  out += `    server_name ${serverName};\n`
  out += `\n`
  for (const loc of s.locations) {
    out += `    ${genLocForPreview(loc)}\n`
  }
  out += `}\n`
  return out
}

function genLocForPreview(loc: SiteLocation): string {
  if (loc.kind === 'Static') {
    const root = loc.root?.replace(/\\/g, '/') || ''
    let block = `location ${loc.path} {\n`
    if (root) block += `        root "${root}";\n`
    block += `        index index.html;\n`
    if (loc.spa_fallback) block += `        try_files \$uri \$uri/ /index.html;\n`
    block += `    }`
    return block
  }
  // Proxy
  const target = loc.target?.trim()
  if (!target) return `# location ${loc.path} { proxy_pass … }  // 填写后端地址后生效`
  let block = `location ${loc.path} {\n`
  block += `        proxy_pass ${target};\n`
  block += `        proxy_http_version 1.1;\n`
  block += `        proxy_set_header Host \$host;\n`
  block += `        proxy_set_header X-Real-IP \$remote_addr;\n`
  block += `        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;\n`
  block += `        proxy_set_header X-Forwarded-Proto \$scheme;\n`
  block += `        proxy_set_header Upgrade \$http_upgrade;\n`
  block += `        proxy_set_header Connection \$connection_upgrade;\n`
  block += `    }`
  return block
}

// 切换到源码视图时拉取当前配置；切回表单时重置标记，下次切 source 重新加载
watch(tab, async (t) => {
  if (t === 'source') {
    if (sourceDirty.value) return // 已有编辑不覆盖
    if (props.isNew) {
      // 新建态：从表单数据生成预览
      if (sourceText.value) {
        sourceText.value.value = generateNginxPreview(form.value)
      }
    } else {
      // 已有站点：从后端读取配置
      try {
        const src = await invoke<string>('get_site_conf', { id: props.site.id })
        if (sourceText.value) {
          sourceText.value.value = src
        }
      } catch (e) {
        console.error(e)
      }
    }
  } else {
    // 切回表单时重置，下次切 source 重新加载
    sourceDirty.value = false
  }
})
</script>

<style scoped>
.overlay { position: fixed; inset: 0; z-index: 50; display: flex; align-items: center; justify-content: center; background: oklch(0 0 0 / 0.5); backdrop-filter: blur(4px); }
.error-banner {
  margin: 0 20px; padding: 8px 12px; background: #fef2f2; color: #b91c1c;
  border-radius: 6px; font-size: 12px;
}
.confirm-box {
  width: 380px; padding: 24px;
  border-radius: 10px; border: 1px solid var(--color-border);
  background: var(--color-card); box-shadow: 0 8px 24px oklch(0 0 0 / 0.45);
}
.confirm-title { font-size: 15px; font-weight: 600; display: flex; align-items: center; gap: 8px; margin-bottom: 12px; }
.confirm-title svg { color: var(--color-destructive); }
.confirm-msg { font-size: 13px; color: var(--color-muted-foreground); margin-bottom: 20px; }
.confirm-actions { display: flex; justify-content: flex-end; gap: 8px; }
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
.btn.sm { height: 26px; padding: 0 10px; font-size: 12px; }
.btn:disabled { opacity: 0.4; cursor: not-allowed; }
.tab-bar { display: flex; gap: 4px; padding: 8px 18px 0; border-bottom: 1px solid var(--color-border); }
.tab-bar button { height: 32px; padding: 0 14px; border: none; background: transparent; color: var(--color-muted-foreground); font-size: 13px; cursor: pointer; border-bottom: 2px solid transparent; margin-bottom: -1px; }
.tab-bar button.active { color: var(--color-foreground); border-bottom-color: var(--color-primary); }
.tab-bar button:disabled { opacity: 0.4; cursor: not-allowed; }
.source-view { display: flex; flex-direction: column; gap: 10px; }
.source-input { width: 100%; min-height: 340px; padding: 12px; background: var(--color-muted); border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-foreground); font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 12.5px; line-height: 1.55; tab-size: 4; outline: none; resize: vertical; box-sizing: border-box; }
.source-input:focus { border-color: var(--color-primary); }
.locked-tip { display: flex; align-items: center; gap: 6px; padding: 8px 10px; margin-bottom: 12px; border-radius: 6px; background: oklch(0.7 0.12 300 / 0.12); color: oklch(0.55 0.15 300); font-size: 12px; }
.locked-tip.between { justify-content: space-between; margin-bottom: 0; }
</style>
