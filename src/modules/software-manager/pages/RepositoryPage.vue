<template>
  <div class="repository-container">
    <h2 class="page-title">{{ $t('softwareRepository') }}</h2>

    <div class="toolbar">
      <input
        type="text"
        placeholder="{{ $t('searchPackages') }}"
        v-model="searchQuery"
        class="search-input"
      />
      <select v-model="selectedCategory" class="category-select">
        <option value="all">{{ $t('allCategories') }}</option>
        <option value="web">Web 服务器</option>
        <option value="database">数据库</option>
        <option value="cache">缓存</option>
        <option value="dev">开发工具</option>
      </select>
    </div>

    <div class="packages-grid">
      <div
        v-for="pkg in filteredPackages"
        :key="pkg.id"
        class="package-card"
      >
        <div class="package-header">
          <div class="package-icon">{{ pkg.icon }}</div>
          <div class="package-info">
            <h3 class="package-name">{{ pkg.name }}</h3>
            <div class="package-version">{{ pkg.version }}</div>
          </div>
        </div>
        <div class="package-description">{{ pkg.description }}</div>
        <div class="package-category">{{ pkg.category }}</div>
        <div class="package-actions">
          <button class="install-btn" @click="installPackage(pkg)">
            📥 {{ $t('install') }}
          </button>
          <button class="details-btn" @click="viewDetails(pkg)">
            ℹ️ {{ $t('details') }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const searchQuery = ref('')
const selectedCategory = ref('all')

const packages = ref([
  {
    id: 1,
    name: 'Nginx',
    version: '1.25.3',
    icon: '🌐',
    description: '高性能的 HTTP 和反向代理服务器',
    category: 'web',
  },
  {
    id: 2,
    name: 'MySQL',
    version: '8.0.33',
    icon: '🗄️',
    description: '最流行的开源关系型数据库管理系统',
    category: 'database',
  },
  {
    id: 3,
    name: 'Redis',
    version: '7.0.11',
    icon: '⚡',
    description: '开源的内存数据结构存储，用作数据库、缓存和消息代理',
    category: 'cache',
  },
  {
    id: 4,
    name: 'MongoDB',
    version: '6.0.6',
    icon: '🍃',
    description: '面向文档的开源 NoSQL 数据库',
    category: 'database',
  },
  {
    id: 5,
    name: 'Node.js',
    version: '20.3.1',
    icon: '🟢',
    description: 'JavaScript 运行时环境',
    category: 'dev',
  },
  {
    id: 6,
    name: 'Docker',
    version: '24.0.6',
    icon: '🐳',
    description: '开源的应用容器引擎',
    category: 'dev',
  },
])

const filteredPackages = computed(() => {
  let result = packages.value

  if (searchQuery.value) {
    const query = searchQuery.value.toLowerCase()
    result = result.filter(pkg =>
      pkg.name.toLowerCase().includes(query) ||
      pkg.description.toLowerCase().includes(query)
    )
  }

  if (selectedCategory.value !== 'all') {
    result = result.filter(pkg => pkg.category === selectedCategory.value)
  }

  return result
})

const installPackage = (pkg: any) => {
  // TODO: 安装软件包
}

const viewDetails = (pkg: any) => {
  // TODO: 查看详细信息
}
</script>

<style scoped>
.repository-container {
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

.category-select {
  padding: 0.75rem;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: var(--card);
  color: var(--foreground);
}

.packages-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: 1.5rem;
}

.package-card {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 1.5rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.package-header {
  display: flex;
  align-items: center;
  gap: 1rem;
}

.package-icon {
  font-size: 2.5rem;
}

.package-info {
  flex: 1;
}

.package-name {
  margin: 0 0 0.25rem 0;
  font-size: 1.2rem;
}

.package-version {
  font-size: 0.8rem;
  opacity: 0.7;
}

.package-description {
  font-size: 0.9rem;
  line-height: 1.5;
}

.package-category {
  font-size: 0.8rem;
  color: var(--foreground);
  opacity: 0.7;
}

.package-actions {
  display: flex;
  gap: 0.5rem;
  margin-top: auto;
}

.install-btn,
.details-btn {
  flex: 1;
  padding: 0.75rem;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.9rem;
}

.install-btn {
  background: #dcfce7;
  color: #166534;
}

.details-btn {
  background: var(--background);
  color: var(--foreground);
}
</style>