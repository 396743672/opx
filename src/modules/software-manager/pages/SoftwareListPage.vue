<template>
  <div class="software-container">
    <h2 class="page-title">{{ $t('softwareManagement') }}</h2>

    <div class="toolbar">
      <input
        type="text"
        placeholder="{{ $t('searchSoftware') }}"
        v-model="searchQuery"
        class="search-input"
      />
      <button class="refresh-btn" @click="refreshSoftware">
        🔄 {{ $t('refresh') }}
      </button>
    </div>

    <div class="software-table">
      <table>
        <thead>
          <tr>
            <th>{{ $t('name') }}</th>
            <th>{{ $t('version') }}</th>
            <th>{{ $t('status') }}</th>
            <th>{{ $t('installPath') }}</th>
            <th>{{ $t('actions') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="software in filteredSoftware" :key="software.id">
            <td>{{ software.name }}</td>
            <td>{{ software.version }}</td>
            <td>
              <span :class="['status-badge', software.status]">
                {{ software.status }}
              </span>
            </td>
            <td>{{ software.path }}</td>
            <td>
              <button class="action-btn" @click="startSoftware(software)">
                ▶️ {{ $t('start') }}
              </button>
              <button class="action-btn stop" @click="stopSoftware(software)">
                ⏹️ {{ $t('stop') }}
              </button>
              <button class="action-btn settings" @click="configureSoftware(software)">
                ⚙️ {{ $t('settings') }}
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const searchQuery = ref('')

const softwareList = ref([
  { id: 1, name: 'Nginx', version: '1.25.3', status: 'running', path: '/usr/local/nginx' },
  { id: 2, name: 'MySQL', version: '8.0.33', status: 'stopped', path: '/usr/local/mysql' },
  { id: 3, name: 'Redis', version: '7.0.11', status: 'running', path: '/usr/local/redis' },
  { id: 4, name: 'MongoDB', version: '6.0.6', status: 'stopped', path: '/usr/local/mongodb' },
])

const filteredSoftware = computed(() => {
  if (!searchQuery.value) return softwareList.value
  const query = searchQuery.value.toLowerCase()
  return softwareList.value.filter(sw =>
    sw.name.toLowerCase().includes(query) ||
    sw.version.toLowerCase().includes(query) ||
    sw.status.toLowerCase().includes(query)
  )
})

const refreshSoftware = () => {
  // TODO: 刷新软件列表
}

const startSoftware = (software: any) => {
  // TODO: 启动软件
}

const stopSoftware = (software: any) => {
  // TODO: 停止软件
}

const configureSoftware = (software: any) => {
  // TODO: 配置软件
}
</script>

<style scoped>
.software-container {
  padding: 2rem;
  overflow-y: auto;
}

.page-title {
  margin: 0 0 2rem 0;
  font-size: 1.8rem;
}

.toolbar {
  display: flex;
  gap: 1rem;
  margin-bottom: 1.5rem;
  align-items: center;
}

.search-input {
  flex: 1;
  max-width: 400px;
  padding: 0.75rem;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: var(--card);
  color: var(--foreground);
}

.refresh-btn {
  padding: 0.75rem 1.5rem;
  background: var(--border);
  border: none;
  border-radius: 4px;
  cursor: pointer;
}

.software-table {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow: hidden;
}

table {
  width: 100%;
  border-collapse: collapse;
}

thead {
  background: var(--background);
}

th {
  padding: 1rem;
  text-align: left;
  font-weight: 600;
}

td {
  padding: 1rem;
  border-top: 1px solid var(--border);
}

.status-badge {
  padding: 0.25rem 0.75rem;
  border-radius: 20px;
  font-size: 0.8rem;
  font-weight: 500;
}

.status-badge.running {
  background: #dcfce7;
  color: #166534;
}

.status-badge.stopped {
  background: #fef2f2;
  color: #991b1b;
}

.action-btn {
  padding: 0.5rem 0.75rem;
  margin-right: 0.5rem;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.8rem;
}

.action-btn.stop {
  background: #fee2e2;
  color: #991b1b;
}

.action-btn.settings {
  background: #fef3c7;
  color: #92400e;
}
</style>