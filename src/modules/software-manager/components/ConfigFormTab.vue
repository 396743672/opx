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
        <label class="form-field-label" :class="{ danger: isEphemeral(field) }">{{ $t(field.label_i18n) }}</label>
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
          :disabled="isEphemeral(field) && isInitialized"
        />
        <select v-else-if="isSelect(field)" v-model="formData[field.key]" class="input">
          <option v-for="opt in selectOptions(field)" :key="opt" :value="opt">{{ opt }}</option>
        </select>
        <div v-else-if="isSize(field)" class="size-field">
          <input
            type="number"
            class="input tnum"
            :value="sizeNum(field.key)"
            @input="setSize(field, ($event.target as HTMLInputElement).value, sizeUnit(field))"
          />
          <select
            class="input unit"
            :value="sizeUnit(field)"
            @change="setSize(field, sizeNum(field.key), ($event.target as HTMLSelectElement).value)"
          >
            <option v-for="u in sizeUnits(field)" :key="u" :value="u">{{ u }}</option>
          </select>
        </div>
        <div v-else-if="isBoolean(field)" class="switch-field">
          <div
            class="toggle"
            :class="{ off: !formData[field.key] }"
            role="switch"
            :aria-checked="!!formData[field.key]"
            tabindex="0"
            @click="formData[field.key] = !formData[field.key]"
            @keydown.enter.prevent="formData[field.key] = !formData[field.key]"
            @keydown.space.prevent="formData[field.key] = !formData[field.key]"
          ></div>
          <span class="switch-label">{{ $t('enabled') }}</span>
        </div>
        <div v-if="field.description_i18n" class="form-field-desc">{{ $t(field.description_i18n) }}</div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ConfigField, ConfigSchema, FormData, InstalledSoftware } from '@/models/software'

const props = defineProps<{ software: InstalledSoftware; schema: ConfigSchema | null }>()
const emit = defineEmits<{ 'update:dirty': [boolean] }>()

const loading = ref(true)
const formData = ref<FormData>({})

async function loadForm() {
  if (!props.schema) {
    loading.value = false
    return
  }
  loading.value = true
  try {
    formData.value = await invoke<FormData>('read_config_form', {
      installedId: props.software.id,
    })
  } catch (e) {
    console.error('read config form failed:', e)
  }
  loading.value = false
}

onMounted(loadForm)

// schema 是父组件异步加载的（初始可能为 null），加载完成后重新读表单
watch(() => props.schema, (newSchema) => {
  if (newSchema) {
    loadForm()
  }
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
function isBoolean(f: ConfigField) {
  return f.field_type.type === 'Boolean'
}
function selectOptions(f: ConfigField): string[] {
  return f.field_type.type === 'Select' ? f.field_type.options : []
}

// Size 字段：值形如 "256mb"，拆成「数字 + 单位」编辑，单位只能从下拉里选（防手写单位出错）
function isSize(f: ConfigField) {
  return f.field_type.type === 'Size'
}
function sizeUnits(f: ConfigField): string[] {
  return f.field_type.type === 'Size' ? f.field_type.units : []
}
function parseSize(v: unknown): { num: string; unit: string } {
  const s = String(v ?? '')
  const m = s.match(/^\s*(\d+)\s*([a-zA-Z]+)\s*$/)
  return m ? { num: m[1], unit: m[2] } : { num: s.replace(/\D/g, ''), unit: '' }
}
function sizeNum(key: string): string {
  return parseSize(formData.value[key]).num
}
function sizeUnit(f: ConfigField): string {
  const u = parseSize(formData.value[f.key]).unit
  const units = sizeUnits(f)
  return u && units.includes(u) ? u : (units[0] ?? '')
}
function setSize(f: ConfigField, num: string, unit: string) {
  formData.value[f.key] = `${num}${unit}`
}

// ephemeral 字段（如 MySQL 初始化密码）：渲染为红色敏感字段，不落盘
function isEphemeral(f: ConfigField): boolean {
  return (props.schema?.ephemeral_keys ?? []).includes(f.key)
}

// 该实例是否已完成首次初始化（initialized == true）：ephemeral 字段应禁用
const isInitialized = computed(
  () => (props.software.config as Record<string, any> | undefined)?.initialized === true,
)

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
/* ephemeral 敏感字段（如初始化密码）：标签标红，提示一次性/不落盘 */
.form-field-label.danger {
  color: #e5484d;
  font-weight: 600;
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
.size-field {
  display: flex;
  gap: 8px;
}
.size-field .unit {
  width: 90px;
  flex: none;
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
.switch-field {
  display: flex;
  align-items: center;
  gap: 10px;
}
.switch-field .toggle {
  width: 36px;
  height: 20px;
  border-radius: 999px;
  background: var(--color-primary);
  position: relative;
  cursor: pointer;
  flex-shrink: 0;
}
.switch-field .toggle::after {
  content: '';
  position: absolute;
  top: 2px;
  left: 18px;
  width: 16px;
  height: 16px;
  border-radius: 999px;
  background: white;
  transition: left 0.2s;
}
.switch-field .toggle.off {
  background: var(--color-border);
}
.switch-field .toggle:focus-visible {
  outline: 2px solid var(--color-primary);
  outline-offset: 2px;
}
.switch-field .toggle.off::after {
  left: 2px;
}
.switch-label {
  font-size: 13px;
}
</style>
