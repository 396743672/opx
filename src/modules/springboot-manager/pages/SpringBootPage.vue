<template>
  <div class="springboot-container">
    <h2 class="page-title">{{ $t('springBoot') }}</h2>

    <div class="toolbar">
      <button class="create-btn" @click="createProject">
        ➕ {{ $t('createProject') }}
      </button>
      <button class="refresh-btn" @click="refreshProjects">
        🔄 {{ $t('refresh') }}
      </button>
    </div>

    <div class="projects-grid">
      <div
        v-for="project in projects"
        :key="project.id"
        class="project-card"
      >
        <div class="project-header">
          <div class="project-icon">🚀</div>
          <div class="project-info">
            <h3 class="project-name">{{ project.name }}</h3>
            <div class="project-version">Spring Boot {{ project.version }}</div>
          </div>
        </div>
        <div class="project-path">{{ project.path }}</div>
        <div class="project-status">
          <span :class="['status-badge', project.status]">
            {{ project.status }}
          </span>
        </div>
        <div class="project-actions">
          <button class="start-btn" @click="startProject(project)" :disabled="project.status === 'running'">
            ▶️ {{ $t('start') }}
          </button>
          <button class="stop-btn" @click="stopProject(project)" :disabled="project.status !== 'running'">
            ⏹️ {{ $t('stop') }}
          </button>
          <button class="logs-btn" @click="viewLogs(project)">
            📋 {{ $t('logs') }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const projects = ref([
  {
    id: 1,
    name: 'user-service',
    version: '3.2.0',
    path: '/opt/projects/user-service',
    status: 'running',
  },
  {
    id: 2,
    name: 'order-service',
    version: '3.1.5',
    path: '/opt/projects/order-service',
    status: 'stopped',
  },
  {
    id: 3,
    name: 'payment-service',
    version: '3.2.0',
    path: '/opt/projects/payment-service',
    status: 'running',
  },
  {
    id: 4,
    name: 'inventory-service',
    version: '3.1.5',
    path: '/opt/projects/inventory-service',
    status: 'stopped',
  },
])

const createProject = () => {
  // TODO: 创建Spring Boot项目
}

const refreshProjects = () => {
  // TODO: 刷新项目列表
}

const startProject = (project: any) => {
  // TODO: 启动项目
}

const stopProject = (project: any) => {
  // TODO: 停止项目
}

const viewLogs = (project: any) => {
  // TODO: 查看日志
}
</script>

<style scoped>
.springboot-container {
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
}

.create-btn,
.refresh-btn {
  padding: 0.75rem 1.5rem;
  border: none;
  border-radius: 4px;
  cursor: pointer;
}

.create-btn {
  background: #dcfce7;
  color: #166534;
}

.refresh-btn {
  background: var(--border);
  color: var(--foreground);
}

.projects-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(350px, 1fr));
  gap: 1.5rem;
}

.project-card {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 1.5rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.project-header {
  display: flex;
  align-items: center;
  gap: 1rem;
}

.project-icon {
  font-size: 2.5rem;
}

.project-info {
  flex: 1;
}

.project-name {
  margin: 0 0 0.25rem 0;
  font-size: 1.2rem;
}

.project-version {
  font-size: 0.8rem;
  opacity: 0.7;
}

.project-path {
  font-size: 0.9rem;
  opacity: 0.8;
  word-break: break-all;
}

.project-status {
  margin: 0.5rem 0;
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

.project-actions {
  display: flex;
  gap: 0.5rem;
  margin-top: auto;
}

.start-btn,
.stop-btn,
.logs-btn {
  flex: 1;
  padding: 0.5rem;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.8rem;
}

.start-btn {
  background: #dcfce7;
  color: #166534;
}

.start-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.stop-btn {
  background: #fee2e2;
  color: #991b1b;
}

.stop-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.logs-btn {
  background: var(--background);
  color: var(--foreground);
}
</style>