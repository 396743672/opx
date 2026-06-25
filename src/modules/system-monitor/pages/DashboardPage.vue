<template>
  <div class="dashboard-container">
    <h2 class="page-title">{{ $t('systemMonitor') }}</h2>

    <div class="stats-grid">
      <div class="stat-card">
        <div class="stat-icon">💻</div>
        <div class="stat-content">
          <div class="stat-number">0</div>
          <div class="stat-label">{{ $t('activeProcesses') }}</div>
        </div>
      </div>

      <div class="stat-card">
        <div class="stat-icon">📈</div>
        <div class="stat-content">
          <div class="stat-number">0%</div>
          <div class="stat-label">{{ $t('cpuUsage') }}</div>
        </div>
      </div>

      <div class="stat-card">
        <div class="stat-icon">💾</div>
        <div class="stat-content">
          <div class="stat-number">0MB</div>
          <div class="stat-label">{{ $t('memoryUsage') }}</div>
        </div>
      </div>

      <div class="stat-card">
        <div class="stat-icon'>📂</div>
        <div class="stat-content">
          <div class="stat-number">0</div>
          <div class="stat-label">{{ $t('runningContainers') }}</div>
        </div>
      </div>
    </div>

    <div class="charts-section">
      <div class="chart-card">
        <h3>{{ $t('cpuUsage') }}</h3>
        <div class="chart-placeholder">
          CPU使用率图表
        </div>
      </div>

      <div class="chart-card">
        <h3>{{ $t('memoryUsage') }}</h3>
        <div class="chart-placeholder">
          内存使用率图表
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

onMounted(() => {
  // TODO: 初始化系统监控数据
})
</script>

<style scoped>
.dashboard-container {
  padding: 2rem;
  overflow-y: auto;
}

.page-title {
  margin: 0 0 2rem 0;
  font-size: 1.8rem;
}

.stats-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
  gap: 1.5rem;
  margin-bottom: 2rem;
}

.stat-card {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 1.5rem;
  display: flex;
  align-items: center;
  gap: 1rem;
}

.stat-icon {
  font-size: 2.5rem;
}

.stat-content {
  flex: 1;
}

.stat-number {
  font-size: 1.8rem;
  font-weight: 600;
  margin-bottom: 0.25rem;
}

.stat-label {
  font-size: 0.9rem;
  opacity: 0.7;
}

.charts-section {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(500px, 1fr));
  gap: 1.5rem;
}

.chart-card {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 1.5rem;
}

.chart-card h3 {
  margin: 0 0 1rem 0;
  font-size: 1.2rem;
}

.chart-placeholder {
  height: 200px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--background);
  border-radius: 4px;
  color: var(--foreground);
  opacity: 0.7;
}
</style>