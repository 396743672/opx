<template>
  <div class="form-tab">
    <div v-if="loading" class="loading">{{ $t('loading') }}</div>
    <div v-else-if="!schema" class="empty">{{ $t('noConfigSchema') }}</div>
    <div v-else class="form-grid">
      <div
        v-for="field in schema.fields"
        :key="field.key"
        class="field"
        :class="{ full: isPort(field) }"
      >
        <label class="form-field-label">{{ $t(field.label_i18n) }}</label>
        <input
          v-if="isText(field)"
          v-model="formData[field.key]"
          class="input"
          :placeholder="String(field.default_value ?? '')"
        />
        <input
          v-else-if="isNumber(field) || isPort(field)"
          type="number"
          v-model.number="formData[field.key]"
          class="input tnum"
        />
        <input
          v-else-if="isPassword(field)"
          v-model="formData[field.key]"
          type="password"
          class="input"
        />
        <select v-else-if="isSelect(field)" v-model="formData[field.key]" class="input">
          <option v-for="opt in selectOptions(field)" :key="opt" :value="opt">{{ opt }}</option>
        </select>
        <div v-if="field.description_i18n" class="form-field-desc">{{ $t(field.description_i18n) }}</div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ConfigField, ConfigSchema, FormData, InstalledSoftware } from '@/models/software'

const props = defineProps<{ software: InstalledSoftware; schema: ConfigSchema | null }>()
const emit = defineEmits<{ 'update:dirty': [boolean] }>()

const loading = ref(true)
const formData = ref<FormData>({})

onMounted(async () => {
  if (!props.schema) {
    loading.value = false
    return
  }
  try {
    formData.value = await invoke<FormData>('read_config_form', {
      installedId: props.software.id,
    })
  } catch (e) {
    console.error('read config form failed:', e)
  }
  loading.value = false
})

watch(formData, () => emit('update:dirty', true), { deep: true })

// ConfigFieldType 用 #[serde(tag = "type")] 序列化为 { type: "Text" } / { type: "Select", options: [...] }
function isText(f: ConfigField) {
  return f.field_type.type === 'Text'
}
function isNumber(f: ConfigField) {
  return f.field_type.type === 'Number'
}
function isPort(f: ConfigField) {
  return f.field_type.type === 'Port'
}
function isPassword(f: ConfigField) {
  return f.field_type.type === 'Password'
}
function isSelect(f: ConfigField) {
  return f.field_type.type === 'Select'
}
function selectOptions(f: ConfigField): string[] {
  return f.field_type.type === 'Select' ? f.field_type.options : []
}

defineExpose({ formData })
</script>

<style scoped>
.form-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 14px;
}
.field.full {
  grid-column: 1 / -1;
}
.form-field-label {
  display: block;
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-bottom: 6px;
}
.input {
  width: 100%;
  height: 34px;
  padding: 0 10px;
  background: var(--color-muted);
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--color-foreground);
  font-size: 13px;
  outline: none;
}
.input:focus {
  border-color: var(--color-primary);
  background: var(--color-card);
}
.tnum {
  font-variant-numeric: tabular-nums;
}
.form-field-desc {
  font-size: 11px;
  color: var(--color-muted-foreground);
  margin-top: 4px;
}
.loading,
.empty {
  padding: 32px;
  text-align: center;
  color: var(--color-muted-foreground);
}
</style>
