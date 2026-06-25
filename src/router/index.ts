import { createRouter, createWebHashHistory } from 'vue-router'
import { defineComponent } from 'vue'
import SystemMonitorDashboard from '@/modules/dashboard/pages/DashboardPage.vue'
import SoftwareListPage from '@/modules/software-manager/pages/SoftwareListPage.vue'
import RepositoryPage from '@/modules/software-manager/pages/RepositoryPage.vue'
import SettingsPage from '@/modules/settings/pages/SettingsPage.vue'
const SpringBootPage = defineComponent({ template: '<div>Spring Boot管理</div>' })

const routes = [
  {
    path: '/',
    redirect: '/dashboard'
  },
  {
    path: '/dashboard',
    name: 'Dashboard',
    component: SystemMonitorDashboard,
    meta: { title: 'systemMonitor' }
  },
  {
    path: '/software',
    name: 'SoftwareList',
    component: SoftwareListPage,
    meta: { title: 'softwareManagement' }
  },
  {
    path: '/repository',
    name: 'Repository',
    component: RepositoryPage,
    meta: { title: 'softwareRepository' }
  },
  {
    path: '/springboot',
    name: 'SpringBoot',
    component: SpringBootPage,
    meta: { title: 'springBoot' }
  },
  {
    path: '/settings',
    name: 'Settings',
    component: SettingsPage,
    meta: { title: 'settings' }
  }
]

const router = createRouter({
  history: createWebHashHistory(),
  routes
})

export default router