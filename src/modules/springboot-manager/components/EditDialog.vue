<dialog open>
  <h3 class="text-lg font-semibold mb-4">{{ props.editing ? $t('editApplication') : $t('addApplication') }}</h3>
  <div class="space-y-4">
    <div>
      <label class="block text-sm font-medium mb-1">{{ $t('name') }}</label>
      <input v-model="form.name" type="text" class="w-full rounded-md border border-border px-3 py-2" />
    </div>
    <div>
      <label class="block text-sm font-medium mb-1">{{ $t('jarPath') }}</label>
      <input v-model="form.jar_path" type="text" class="w-full rounded-md border border-border px-3 py-2" />
    </div>
    <div>
      <label class="block text-sm font-medium mb-1">{{ $t('version') }}</label>
      <input v-model="form.version" type="text" class="w-full rounded-md border border-border px-3 py-2" />
    </div>
    <div>
      <label class="block text-sm font-medium mb-1">{{ $t('port') }}</label>
      <input v-model.number="form.port" type="number" class="w-full rounded-md border border-border px-3 py-2" />
    </div>
    <div>
      <label class="block text-sm font-medium mb-1">JVM {{ $t('jvmOptions') }}</label>
      <input v-model="form.jvm_opts" type="text" class="w-full rounded-md border border-border px-3 py-2" />
    </div>
    <div>
      <label class="block text-sm font-medium mb-1">{{ $t('environment') }}</label>
      <input v-model="form.env" type="text" class="w-full rounded-md border border-border px-3 py-2" />
    </div>
    <div>
      <label class="block text-sm font-medium mb-1">{{ $t('group') }}</label>
      <input v-model="form.group" type="text" class="w-full rounded-md border border-border px-3 py-2" />
    </div>
    <div>
      <label class="flex items-center gap-2">
        <input v-model="form.auto_start_on_app_start" type="checkbox" />
        <span class="text-sm">{{ $t('autoStart') }}</span>
      </label>
    </div>
    <div>
      <label class="block text-sm font-medium mb-1">{{ $t('startupOrder') }}</label>
      <input v-model.number="form.startup_order" type="number" class="w-full rounded-md border border-border px-3 py-2" />
    </div>
  </div>
  <div class="flex justify-end gap-2 mt-6">
    <button @click="onCancel" class="px-4 py-2 rounded-md bg-secondary">
      {{ $t('cancel') }}
    </button>
    <button @click="handleSave" class="px-4 py-2 rounded-md bg-primary text-primary-foreground">
      {{ $t('save') }}
    </button>
  </div>
</dialog>

<script setup lang="ts">
import type { SpringApp } from '@/models/springboot'
import { useI18n } from 'vue-i18n'
import { ref, watch } from 'vue'
import { v4 } from 'uuid'

withDefaults(defineProps<{
  editing: SpringApp | null
  onCancel: () => void
  onSave: (app: SpringApp) => Promise<void>
}>(), {})

const { t } = useI18n()

const form = ref<SpringApp>({
  id: v4(),
  name: '',
  jar_path: '',
  version: '1.0.0',
  env: 'dev',
  port: 8080,
  jvm_opts: '-Xmx512m',
  args: '',
  status: 'Stopped',
  log_path: '',
  backup_enabled: true,
  auto_restart: false,
  group: null,
  auto_start_on_app_start: true,
  startup_order: 100,
})

watch(() => props.editing, (newVal) => {
  if (newVal) {
    form.value = { ...newVal }
  }
}, { immediate: true })

const handleSave = async () => {
  // Generate default log path if empty
  if (!form.value.log_path) {
    form.value.log_path = `logs/springboot/${form.value.id}.log`
  }
  await onSave(form.value)
  onCancel()
}
</script>