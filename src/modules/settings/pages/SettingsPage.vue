<template>
  <div class="settings-container">
    <h2 class="page-title">{{ $t('settings') }}</h2>

    <div class="settings-form">
      <div class="setting-section">
        <h3 class="section-title">{{ $t('interfaceSettings') }}</h3>

        <div class="form-group">
          <label class="form-label">{{ $t('theme') }}</label>
          <select v-model="localSettings.theme" class="form-control">
            <option value="light">{{ $t('light') }}</option>
            <option value="dark">{{ $t('dark') }}</option>
            <option value="auto">{{ $t('auto') }}</option>
          </select>
        </div>

        <div class="form-group">
          <label class="form-label">{{ $t('language') }}</label>
          <select v-model="localSettings.language" class="form-control">
            <option value="zh-CN">中文</option>
            <option value="en-US">English</option>
          </select>
        </div>

        <div class="form-group">
          <label class="form-label">
            <input
              type="checkbox"
              v-model="localSettings.sidebarCollapsed"
            />
            {{ $t('defaultSidebarCollapsed') }}
          </label>
        </div>
      </div>

      <div class="setting-section">
        <h3 class="section-title">{{ $t('systemSettings') }}</h3>

        <div class="form-group">
          <label class="form-label">
            <input
              type="checkbox"
              v-model="autoStart"
            />
            {{ $t('autoStartOnLogin') }}
          </label>
        </div>

        <div class="form-group">
          <label class="form-label">
            <input
              type="checkbox"
              v-model="checkUpdates"
            />
            {{ $t('checkUpdatesAutomatically') }}
          </label>
        </div>
      </div>

      <div class="form-actions">
        <button class="cancel-btn" @click="cancel">
          {{ $t('cancel') }}
        </button>
        <button class="save-btn" @click="save">
          {{ $t('save') }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/settings'

const { t } = useI18n()
const settingsStore = useSettingsStore()

const localSettings = ref({
  theme: 'auto',
  language: 'zh-CN',
  sidebarCollapsed: false,
})

const autoStart = ref(false)
const checkUpdates = ref(true)

onMounted(() => {
  if (settingsStore.settings) {
    localSettings.value = {
      ...settingsStore.settings,
    }
  }
})

const cancel = () => {
  // 重置为原始设置
  if (settingsStore.settings) {
    localSettings.value = {
      ...settingsStore.settings,
    }
  }
}

const save = async () => {
  settingsStore.updateSettings(localSettings.value)
  await settingsStore.saveSettings()
  // TODO: 显示保存成功提示
}
</script>

<style scoped>
.settings-container {
  padding: 2rem;
  overflow-y: auto;
  max-width: 800px;
}

.page-title {
  margin: 0 0 2rem 0;
  font-size: 1.8rem;
}

.settings-form {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 2rem;
}

.setting-section {
  margin-bottom: 2rem;
}

.section-title {
  margin: 0 0 1.5rem 0;
  font-size: 1.2rem;
  padding-bottom: 0.5rem;
  border-bottom: 1px solid var(--border);
}

.form-group {
  margin-bottom: 1.5rem;
  display: flex;
  align-items: center;
  gap: 1rem;
}

.form-label {
  min-width: 150px;
  font-weight: 500;
}

.form-control {
  flex: 1;
  max-width: 300px;
  padding: 0.75rem;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: var(--background);
  color: var(--foreground);
}

.form-actions {
  display: flex;
  gap: 1rem;
  justify-content: flex-end;
  margin-top: 2rem;
}

.cancel-btn,
.save-btn {
  padding: 0.75rem 1.5rem;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 1rem;
}

.cancel-btn {
  background: var(--background);
  color: var(--foreground);
}

.save-btn {
  background: #dcfce7;
  color: #166534;
}
</style>