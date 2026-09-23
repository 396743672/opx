<template>
  <Teleport to="body">
    <div class="overlay">
      <div class="dialog">
        <div class="head">
          <div class="title">
            <Icon icon="mdi:dns-outline" /> {{ isNew ? $t('newDnsAccount') : $t('editDnsAccount') }}
          </div>
          <button class="x" @click="$emit('close')"><Icon icon="mdi:close" /></button>
        </div>

        <div class="body">
          <label class="lbl">{{ $t('accountName') }} <span class="text-destructive">*</span></label>
          <input
            v-model="form.name"
            class="input w-full mb-1"
            :class="{ 'border-destructive': nameError }"
            :placeholder="$t('accountNamePlaceholder')"
            @input="nameError = ''"
          />
          <div v-if="nameError" class="text-xs text-destructive mb-3">{{ nameError }}</div>
          <div v-else class="mb-3"></div>

          <div class="mb-3">
            <label class="lbl">{{ $t('dnsProviderLabel') }}</label>
            <select v-model="form.provider" class="input w-full">
              <option v-for="p in PROVIDERS" :key="p" :value="p">{{ p }}</option>
            </select>
          </div>

          <div class="flex gap-3 mb-3">
            <div v-if="cred.idLabel" class="flex-1 min-w-0">
              <label class="lbl">{{ cred.idLabel }}</label>
              <input v-model="form.access_key_id" type="password" class="input w-full font-mono" />
            </div>
            <div class="flex-1 min-w-0">
              <label class="lbl">{{ cred.idLabel ? cred.secretLabel : $t('credToken') }}</label>
              <input
                v-if="cred.idLabel"
                v-model="form.access_key_secret"
                type="password"
                class="input w-full font-mono"
              />
              <input v-else v-model="form.token" type="password" class="input w-full font-mono" />
            </div>
          </div>

          <div class="hint mb-3">
            <Icon icon="mdi:information-outline" /> {{ $t('credPlaintextWarning') }}
          </div>
          <div v-if="testMsg" class="hint mb-3" :class="testOk ? 'ok' : 'bad'">{{ testMsg }}</div>
        </div>

        <div class="foot">
          <button class="btn" :disabled="saving" @click="$emit('close')">{{ $t('cancel') }}</button>
          <button
            class="btn"
            :disabled="testing || isNew"
            :title="isNew ? $t('acmeNeedSave') : ''"
            @click="doTest"
          >
            <Icon icon="mdi:lan-connect" /> {{ testing ? $t('testing') : $t('testConnect') }}
          </button>
          <button class="btn primary" :disabled="saving" @click="save">
            <Icon icon="mdi:content-save-outline" /> {{ saving ? $t('saving') : $t('save') }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>

  <Teleport to="body">
    <div v-if="saveError" class="overlay" style="z-index:70" @click.self="saveError = ''">
      <div class="confirm-box">
        <div class="confirm-title"><Icon icon="mdi:alert-circle-outline" /></div>
        <p class="confirm-msg">{{ saveError }}</p>
        <div class="confirm-actions">
          <button class="btn primary" @click="saveError = ''">{{ $t('confirm') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import { PROVIDERS, CRED_FIELDS, type DnsAccount, type ProviderId } from '@/models/dns-account'

const { t } = useI18n()
const props = withDefaults(defineProps<{ account: DnsAccount; isNew?: boolean }>(), { isNew: false })
const emit = defineEmits<{ close: []; saved: [] }>()

// 深拷贝：编辑时不能改 prop，zones 也要独立一份
const form = ref<DnsAccount>({ ...props.account, zones: [...props.account.zones] })
const cred = computed(() => CRED_FIELDS[form.value.provider as ProviderId] ?? CRED_FIELDS.cloudflare)

const saving = ref(false)
const saveError = ref('')
const nameError = ref('')
const testing = ref(false)
const testOk = ref<boolean | null>(null)
const testMsg = ref('')

async function doTest() {
  testing.value = true
  testOk.value = null
  testMsg.value = ''
  try {
    // 后端返回落盘后的账号：直接覆盖表单，避免保存时用旧的空 zones 抹掉缓存
    form.value = await invoke<DnsAccount>('test_dns_account', { id: form.value.id })
    testOk.value = true
    testMsg.value = t('testOkZones', { n: form.value.zones.length })
  } catch (e) {
    testOk.value = false
    testMsg.value = `${t('testFailed')}: ${String(e)}`
  } finally {
    testing.value = false
  }
}

async function save() {
  if (!form.value.name.trim()) {
    nameError.value = t('siteNameRequired')
    return
  }
  saving.value = true
  saveError.value = ''
  try {
    await invoke('save_dns_account', { account: form.value })
    emit('saved')
  } catch (e) {
    saveError.value = String(e)
  } finally {
    saving.value = false
  }
}
</script>

<style scoped>
.overlay {
  position: fixed; inset: 0; z-index: 50;
  display: flex; align-items: center; justify-content: center;
  background: oklch(0 0 0 / 0.5); backdrop-filter: blur(4px);
}
.dialog {
  width: 560px; max-height: 90vh; border-radius: 10px;
  border: 1px solid var(--color-border); background: var(--color-card);
  box-shadow: 0 8px 24px oklch(0 0 0 / 0.45);
  display: flex; flex-direction: column; overflow: hidden;
}
.head {
  display: flex; justify-content: space-between; align-items: center;
  padding: 14px 18px; border-bottom: 1px solid var(--color-border);
  flex: none;
}
.title { font-weight: 600; display: flex; gap: 8px; align-items: center; }
.title svg { color: var(--color-primary); }
.x { border: none; background: transparent; color: var(--color-muted-foreground); cursor: pointer; }
.body { padding: 16px 18px; flex: 1 1 auto; min-height: 0; overflow-y: auto; }
.foot {
  display: flex; justify-content: flex-end; gap: 8px;
  padding: 12px 18px; border-top: 1px solid var(--color-border);
  flex: none;
}
.lbl { display: block; font-size: 12px; color: var(--color-muted-foreground); margin-bottom: 4px; }
.hint { font-size: 11px; color: var(--color-muted-foreground); display: flex; align-items: center; gap: 5px; overflow-wrap: anywhere; }
.hint.ok { color: oklch(0.6 0.14 150); }
.hint.bad { color: var(--color-destructive); }
.input {
  height: 32px; padding: 0 10px; background: var(--color-muted); border: 1px solid transparent;
  border-radius: 6px; color: var(--color-foreground); font-size: 13px; outline: none; box-sizing: border-box;
}
.input:focus { border-color: var(--color-primary); background: var(--color-card); }
.btn {
  display: inline-flex; align-items: center; gap: 6px; height: 32px; padding: 0 12px;
  border-radius: 6px; border: 1px solid var(--color-border); background: var(--color-card);
  color: var(--color-foreground); font-size: 13px; cursor: pointer;
}
.btn.primary { background: var(--color-primary); color: var(--color-primary-foreground); border-color: var(--color-primary); }
.btn:disabled { opacity: 0.4; cursor: not-allowed; }
.btn svg { width: 16px; height: 16px; }
.confirm-box {
  width: 380px; padding: 24px; border-radius: 10px; border: 1px solid var(--color-border);
  background: var(--color-card); box-shadow: 0 8px 24px oklch(0 0 0 / 0.45);
}
.confirm-title { font-size: 15px; font-weight: 600; display: flex; align-items: center; gap: 8px; margin-bottom: 12px; }
.confirm-title svg { color: var(--color-destructive); }
.confirm-msg { font-size: 13px; color: var(--color-muted-foreground); margin-bottom: 20px; overflow-wrap: anywhere; word-break: break-word; max-height: 40vh; overflow-y: auto; }
.confirm-actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>