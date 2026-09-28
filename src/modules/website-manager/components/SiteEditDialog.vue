<template>
  <Teleport to="body">
    <div class="overlay">
      <div class="dialog">
        <div class="head">
          <div class="title"><Icon icon="mdi:web" /> {{ form.name || $t('newSite') }}</div>
          <button class="dialog-close" @click="$emit('close')"><Icon icon="mdi:close" /></button>
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
              <div class="flex gap-2">
                <input v-model="form.server_name" class="input flex-1 font-mono" placeholder="app.demo.com" />
                <select
                  v-if="zoneOptions.length"
                  class="input font-mono"
                  style="width:180px"
                  :title="$t('zoneHelperHint')"
                  @change="onZonePick"
                >
                  <option value="">{{ $t('zoneHelper') }}</option>
                  <option v-for="z in zoneOptions" :key="z" :value="z">{{ z }}</option>
                </select>
              </div>
              <div class="hint">{{ $t('serverNameHint') }}</div>
            </div>
            <div style="width:130px">
              <label class="lbl">{{ $t('listenPort') }}</label>
              <input v-model.number="form.listen" type="number" class="input w-full font-mono" />
            </div>
          </div>

          <div class="mb-3 space-y-2">
            <label class="flex items-center gap-2">
              <input type="checkbox" v-model="form.ssl.enabled" :disabled="!!site.custom_conf" />
              <span class="text-sm">{{ $t('sslEnable') }}</span>
            </label>
            <template v-if="form.ssl.enabled">
              <div>
                <label class="lbl">{{ $t('certSource') }}</label>
                <select v-model="certSource" class="input w-full" :disabled="!!site.custom_conf">
                  <option value="self-signed">{{ $t('certSelfSigned') }}</option>
                  <option value="acme">{{ $t('certAcme') }}</option>
                </select>
              </div>
              <template v-if="certSource === 'self-signed'">
                <div class="flex gap-2">
                  <input v-model="genDomain" class="input flex-1 font-mono" :placeholder="$t('sslDomain')" />
                  <button class="btn" :disabled="genning" @click="genCert">
                    <Icon icon="mdi:shield-check-outline" /> {{ genning ? $t('genCerting') : $t('genCert') }}
                  </button>
                </div>
                <div>
                  <label class="lbl">{{ $t('sslCertPath') }}</label>
                  <input v-model="form.ssl.cert_path" class="input w-full font-mono" placeholder="sites-data/certs/demo.crt" />
                </div>
                <div>
                  <label class="lbl">{{ $t('sslKeyPath') }}</label>
                  <input v-model="form.ssl.key_path" class="input w-full font-mono" placeholder="sites-data/certs/demo.key" />
                </div>
              </template>
              <template v-else>
                <div class="flex items-center justify-between gap-3">
                  <div>
                    <label class="lbl" style="margin-bottom:0">{{ $t('dnsAccount') }}</label>
                    <div class="hint" style="margin-top:0">{{ $t('acmeNeedAccount') }}</div>
                  </div>
                  <select v-model="form.ssl.dns_account_id" class="input" style="width:220px">
                    <option :value="null">{{ $t('selectDnsAccount') }}</option>
                    <option v-for="a in dnsAccounts" :key="a.id" :value="a.id">
                      {{ a.name }}（{{ a.provider }}）
                    </option>
                  </select>
                </div>
                <div class="flex items-center gap-2 flex-wrap">
                  <button v-if="!isNew" class="btn primary" :disabled="acmeBusy" @click="issueCert">
                    <Icon icon="mdi:certificate-outline" />
                    {{ form.ssl.cert_expires_at ? $t('reissueCert') : $t('issueCert') }}
                  </button>
                  <span v-if="acmeStatus" class="hint" style="margin-top:0">{{ translateError(acmeStatus, t, te) }}</span>
                </div>
                <div v-if="form.ssl.cert_expires_at" class="hint">
                  {{ $t('certExpiresAt') }}: {{ form.ssl.cert_expires_at }}
                </div>
                <div v-if="sslNeedIssue" class="hint">{{ $t('acmeNeedIssue') }}</div>
              </template>
              <div class="hint">{{ $t('sslHint') }}</div>
            </template>
          </div>

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

        <div class="foot">
          <button class="btn" :disabled="saving" @click="$emit('close')">{{ $t('cancel') }}</button>
          <button
            class="btn primary"
            :disabled="saving || (tab === 'form' && !!site.custom_conf)"
            :title="tab === 'form' && site.custom_conf ? $t('customConfLocked') : ''"
            @click="save()"
          >
            {{ saving ? $t('saving') : $t('save') }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
  <Teleport to="body">
    <div v-if="busyKind" class="overlay" style="z-index:65">
      <div class="confirm-box">
        <div class="confirm-title" style="color: var(--color-primary)">
          <Icon icon="mdi:progress-clock" /> {{ busyTitle }}
        </div>
        <p class="confirm-msg" style="margin-bottom: 12px">{{ busyText }}</p>
        <div v-if="busyPct != null" class="busy-bar">
          <i :style="{ width: busyPct + '%' }"></i>
        </div>
      </div>
    </div>
  </Teleport>

  <Teleport to="body">
    <div v-if="confirmUnlock" class="overlay" style="z-index:60">
      <div class="confirm-box">
        <div class="confirm-title"><Icon icon="mdi:alert-circle-outline" /> {{ $t('confirm') }}</div>
        <p class="confirm-msg">{{ $t('restoreFormConfirm') }}</p>
        <div class="confirm-actions">
          <button class="btn" @click="confirmUnlock = false">{{ $t('cancel') }}</button>
          <button class="btn danger" @click="doUnlock">{{ $t('confirm') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
  <Teleport to="body">
    <div v-if="saveError" class="overlay" style="z-index:70" @click.self="saveError = ''">
      <div class="confirm-box">
        <div class="confirm-title"><Icon icon="mdi:alert-circle-outline" /></div>
        <p class="confirm-msg">{{ translateError(saveError, t, te) }}</p>
        <div class="confirm-actions">
          <button class="btn primary" @click="saveError = ''">{{ $t('confirm') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@/utils/ipc'
import { listen } from '@tauri-apps/api/event'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/settings'
import { translateError } from '@/utils/i18nError'
import LocationEditor from './LocationEditor.vue'
import type { Site, SiteLocation } from '@/models/website'
import type { DnsAccount } from '@/models/dns-account'

const { t, te } = useI18n()
const settingsStore = useSettingsStore()
const props = withDefaults(defineProps<{ site: Site; isNew?: boolean }>(), { isNew: false })
const emit = defineEmits<{ close: []; saved: [] }>()

const form = ref<Site>(props.site)
if (!form.value.ssl) {
  form.value.ssl = { enabled: false, cert_path: null, key_path: null, acme: false, cert_expires_at: null }
}
const certSource = ref<'self-signed' | 'acme'>(props.site.ssl?.acme ? 'acme' : 'self-signed')
const acmeBusy = ref(false)
const acmeStatus = ref('')
let unlistenAcme: (() => void) | null = null

// ponytail: 对话框自己拉账号列表（列表页未加载账号，少一层 prop）
const dnsAccounts = ref<DnsAccount[]>([])

// 用缓存 zone 辅助填写 server_name。只在 SSL 走 ACME 且已选账号时出现，
// 避免给不需要证书的站点凭空多一个下拉。
const zoneOptions = computed(() => {
  if (!form.value.ssl.enabled || certSource.value !== 'acme') return []
  const a = dnsAccounts.value.find((x) => x.id === form.value.ssl.dns_account_id)
  return a?.zones ?? []
})

/** 选中 zone 时把 server_name 的主域名部分换掉（保留子域前缀，如 app.demo.com → app.example.com） */
function onZonePick(e: Event) {
  const zone = (e.target as HTMLSelectElement).value
  ;(e.target as HTMLSelectElement).value = '' // 复位成占位项，把它当一次性助手而非表单字段
  if (!zone) return
  const cur = (form.value.server_name ?? '').trim().replace(/\.$/, '')
  const lower = cur.toLowerCase()
  const sub = lower.endsWith(`.${zone}`) ? cur.slice(0, -(zone.length + 1)) : ''
  form.value.server_name = sub ? `${sub}.${zone}` : zone
  if (!genDomain.value) genDomain.value = form.value.server_name
}

// ===== 忙碌弹窗（保存 / 申请证书 / 生成自签证书）=====
const busyKind = ref<'' | 'save' | 'issue' | 'gen'>('')
const acmePct = ref(0)
/** 后端 acme-progress 的阶段 → 进度百分比（仅用于展示） */
const PHASE_PCT: Record<string, number> = {
  'creating-order': 10,
  'waiting-dns': 35,
  validating: 65,
  downloading: 90,
  done: 100,
}
const busyTitle = computed(() =>
  busyKind.value === 'issue' ? t('issueCert') : busyKind.value === 'gen' ? t('genCert') : t('save'),
)
const busyText = computed(() => {
  if (busyKind.value === 'issue') return acmeStatus.value || t('acmeIssuing')
  if (busyKind.value === 'gen') return t('genCerting')
  return t('saving')
})
const busyPct = computed(() => (busyKind.value === 'issue' ? acmePct.value : null))

// 选了 ACME 但尚未签发（无证书路径）：保存时会自动申请证书
const sslNeedIssue = computed(
  () => certSource.value === 'acme' && !form.value.ssl.cert_path
)

/** 实际发起签发（不含前置校验），供「保存」与「申请证书」共用。返回是否成功。 */
async function doIssue(): Promise<boolean> {
  acmeBusy.value = true
  busyKind.value = 'issue'
  acmePct.value = 0
  acmeStatus.value = t('acmeIssuing')
  try {
    await invoke('issue_site_certificate', { siteId: props.site.id })
    // staging 下流程同样「成功」，但浏览器不信任这张证书——必须说清楚，否则用户看到
    // 「签发成功」再打开页面报证书错误会以为是 nginx 配错了
    acmeStatus.value = settingsStore.settings?.acme_use_staging
      ? `${t('acmeDone')} · ${t('acmeStagingWarn')}`
      : t('acmeDone')
    // 成功后重新读取站点，拿到 cert_expires_at / 证书路径
    try {
      const list = await invoke<Site[]>('list_websites')
      const fresh = list.find((s) => s.id === props.site.id)
      if (fresh?.ssl) {
        form.value.ssl = fresh.ssl
        if (fresh.server_name) genDomain.value = fresh.server_name
      }
    } catch {
      // 刷新失败不影响签发结果提示
    }
    return true
  } catch (e) {
    acmeStatus.value = String(e)
    saveError.value = String(e)
    return false
  } finally {
    acmeBusy.value = false
    busyKind.value = ''
  }
}

/** 独立的「申请证书/重新申请」按钮：需站点已保存（后端按 id 读取） */
async function issueCert() {
  if (props.isNew) {
    acmeStatus.value = t('acmeNeedSave')
    return
  }
  if (!form.value.ssl.dns_account_id) {
    acmeStatus.value = t('acmeNeedAccount')
    return
  }
  await doIssue()
}

onMounted(async () => {
  try {
    dnsAccounts.value = await invoke<DnsAccount[]>('list_dns_accounts')
  } catch (e) {
    console.error('load dns accounts failed:', e)
  }
  unlistenAcme = await listen<{ domain: string; phase: string; message: string }>('acme-progress', (e) => {
    const cur = (props.site.server_name ?? '').trim().toLowerCase()
    if (e.payload.domain.trim().toLowerCase() !== cur) return
    acmeStatus.value = e.payload.message || e.payload.phase
    acmePct.value = PHASE_PCT[e.payload.phase] ?? acmePct.value
  })
})
onUnmounted(() => {
  unlistenAcme?.()
})

const genDomain = ref(props.site.server_name || '')
const genning = ref(false)

async function genCert() {
  const d = genDomain.value.trim() || form.value.server_name?.trim() || ''
  if (!d) {
    saveError.value = t('sslDomainRequired')
    return
  }
  genning.value = true
  busyKind.value = 'gen'
  saveError.value = ''
  try {
    const [cert, key] = await invoke<string[]>('generate_self_signed_cert', { domain: d })
    form.value.ssl.cert_path = cert
    form.value.ssl.key_path = key
  } catch (e) {
    saveError.value = String(e)
  } finally {
    genning.value = false
    busyKind.value = ''
  }
}
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
  if (tab.value === 'form') {
    form.value.ssl.acme = certSource.value === 'acme'
    // 选了 ACME 但尚未签发：本次保存不启用 HTTPS（否则生成的配置缺 ssl_certificate，
    // nginx -t 会失败）；签发成功后由后端写回 enabled=true + 证书路径。
    if (form.value.ssl.acme && !form.value.ssl.cert_path) {
      form.value.ssl.enabled = false
    }
  }
  saving.value = true
  busyKind.value = 'save'
  try {
    if (tab.value === 'source') {
      await invoke('set_site_conf', {
        id: props.site.id,
        content: sourceText.value?.value ?? '',
      })
    } else {
      // 保存即生效：nginx 运行中时后端自动校验并 reload
      await invoke('save_website', { site: form.value })
      // 选了 ACME 但尚无证书：保存时一并申请，一次操作完成
      if (sslNeedIssue.value) {
        if (!form.value.ssl.dns_account_id) {
          saveError.value = t('acmeNeedAccount') // 站点已保存，留在对话框提示选择 DNS 账号
          return
        }
        if (!(await doIssue())) return // 签发失败：留在对话框展示原因
      }
    }
    emit('saved')
  } catch (e) {
    saveError.value = String(e)
  } finally {
    saving.value = false
    busyKind.value = ''
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
    busyKind.value = ''
  }
}

// 新建站点：从表单数据生成 nginx 配置预览，跳过后端读取（尚未落库）
function generateNginxPreview(s: Site): string {
  const serverName = s.server_name?.trim() || '_'
  const ssl = s.ssl?.enabled
  let out = `# 配置预览（来源于表单数据，保存后写入文件）\nserver {\n`
  out += ssl ? `    listen 443 ssl;\n` : `    listen ${s.listen};\n`
  if (ssl && s.ssl?.cert_path) out += `    ssl_certificate ${s.ssl.cert_path};\n`
  if (ssl && s.ssl?.key_path) out += `    ssl_certificate_key ${s.ssl.key_path};\n`
  if (ssl) {
    out += `    ssl_protocols TLSv1.2 TLSv1.3;\n`
    out += `    ssl_session_cache shared:SSL:10m;\n`
  }
  out += `    server_name ${serverName};\n`
  out += `\n`
  const shortId = s.id.slice(0, 8)
  for (const loc of s.locations) {
    out += `    ${genLocForPreview(loc, shortId)}\n`
  }
  out += `}\n`
  if (ssl && s.listen !== 443) {
      out += `server {\n`
      out += `    listen ${s.listen};\n`
      out += `    server_name ${serverName};\n`
      out += `    return 301 https://$host$request_uri;\n`
      out += `}\n`
    }
  return out
}

function sanitizePath(path: string): string {
  const s = path.replace(/[^a-zA-Z0-9]/g, '_')
  return s || 'root'
}

function genLocForPreview(loc: SiteLocation, shortId: string): string {
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
  const upstreams = (loc.upstreams || []).map((u) => u.addr.trim()).filter(Boolean)
  const target =
    upstreams.length > 1
      ? `http://site_${shortId}_${sanitizePath(loc.path)}`
      : loc.target?.trim()
  if (!target) return `# location ${loc.path} { proxy_pass … }  // 填写后端地址后生效`
  const subpath = loc.proxy_subpath?.trim()
  let block = `location ${loc.path} {\n`
  block += `        proxy_pass ${subpath ? `${target.replace(/\/$/, '')}${subpath}` : target};\n`
  block += `        proxy_http_version 1.1;\n`
  block += `        proxy_set_header Host $host;\n`
  block += `        proxy_set_header X-Real-IP $remote_addr;\n`
  block += `        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n`
  block += `        proxy_set_header X-Forwarded-Proto $scheme;\n`
  block += `        proxy_set_header Upgrade $http_upgrade;\n`
  block += `        proxy_set_header Connection $connection_upgrade;\n`
  for (const h of loc.proxy_headers || []) {
    if (h.name.trim()) block += `        proxy_set_header ${h.name} ${h.value};\n`
  }
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
.confirm-msg { font-size: 13px; color: var(--color-muted-foreground); margin-bottom: 20px; overflow-wrap: anywhere; word-break: break-word; max-height: 40vh; overflow-y: auto; }
.confirm-actions { display: flex; justify-content: flex-end; gap: 8px; }
.busy-bar { height: 6px; border-radius: 999px; background: var(--color-muted); overflow: hidden; }
.busy-bar i { display: block; height: 100%; background: var(--color-primary); transition: width 0.3s ease-out; }
.dialog { width: 640px; max-height: 90vh; display: flex; flex-direction: column; overflow: hidden; border-radius: 10px; border: 1px solid var(--color-border); background: var(--color-card); box-shadow: 0 8px 24px oklch(0 0 0 / 0.45); }
.head { display: flex; justify-content: space-between; align-items: center; padding: 14px 18px; border-bottom: 1px solid var(--color-border); flex: none; }
.title { font-weight: 600; display: flex; gap: 8px; align-items: center; }
.title svg { color: var(--color-primary); }

.body { padding: 16px 18px; overflow-y: auto; flex: 1 1 auto; min-height: 0; }
.foot { display: flex; justify-content: flex-end; gap: 8px; padding: 12px 18px; border-top: 1px solid var(--color-border); flex: none; }
.lbl { display: block; font-size: 12px; color: var(--color-muted-foreground); margin-bottom: 4px; }
.hint { font-size: 11px; color: var(--color-muted-foreground); margin-top: 3px; overflow-wrap: anywhere; word-break: break-word; }
.input { height: 32px; padding: 0 10px; background: var(--color-muted); border: 1px solid transparent; border-radius: 6px; color: var(--color-foreground); font-size: 13px; outline: none; box-sizing: border-box; }
.input:focus { border-color: var(--color-primary); background: var(--color-card); }
.btn { display: inline-flex; align-items: center; gap: 6px; height: 32px; padding: 0 12px; border-radius: 6px; border: 1px solid var(--color-border); background: var(--color-card); color: var(--color-foreground); font-size: 13px; cursor: pointer; }
.btn.primary { background: var(--color-primary); color: var(--color-primary-foreground); border-color: var(--color-primary); }
.btn.sm { height: 26px; padding: 0 10px; font-size: 12px; }
.btn:disabled { opacity: 0.4; cursor: not-allowed; }
.tab-bar { display: flex; gap: 4px; padding: 8px 18px 0; border-bottom: 1px solid var(--color-border); flex: none; }
.tab-bar button { height: 32px; padding: 0 14px; border: none; background: transparent; color: var(--color-muted-foreground); font-size: 13px; cursor: pointer; border-bottom: 2px solid transparent; margin-bottom: -1px; }
.tab-bar button.active { color: var(--color-foreground); border-bottom-color: var(--color-primary); }
.tab-bar button:disabled { opacity: 0.4; cursor: not-allowed; }
.source-view { display: flex; flex-direction: column; gap: 10px; }
.source-input { width: 100%; min-height: 340px; padding: 12px; background: var(--color-muted); border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-foreground); font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 12.5px; line-height: 1.55; tab-size: 4; outline: none; resize: vertical; box-sizing: border-box; }
.source-input:focus { border-color: var(--color-primary); }
.locked-tip { display: flex; align-items: center; gap: 6px; padding: 8px 10px; margin-bottom: 12px; border-radius: 6px; background: oklch(0.7 0.12 300 / 0.12); color: oklch(0.55 0.15 300); font-size: 12px; }
.locked-tip.between { justify-content: space-between; margin-bottom: 0; }
</style>
