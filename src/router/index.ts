import { createRouter, createWebHashHistory } from 'vue-router'
import SystemMonitorDashboard from '@/modules/system-monitor/pages/DashboardPage.vue'
import SoftwareListPage from '@/modules/software-manager/pages/SoftwareListPage.vue'
import RepositoryPage from '@/modules/software-manager/pages/RepositoryPage.vue'
import SpringBootPage from '@/modules/springboot-manager/pages/SpringBootPage.vue'
import SettingsPage from '@/modules/settings/pages/SettingsPage.vue'

const routes = [
  {
    path: '/',
    redirect: '/dashboard'
  },
  {
    path: '/dashboard',
    name: 'dashboard',
    component: SystemMonitorDashboard,
    meta: { title: 'systemMonitor' }
  },
  {
    path: '/software',
    name: 'software',
    component: SoftwareListPage,
    meta: { title: 'softwareManagement' }
  },
  {
    path: '/repository',
    name: 'repository',
    component: RepositoryPage,
    meta: { title: 'softwareRepository' }
  },
  {
    path: '/springboot',
    name: 'springboot',
    component: SpringBootPage,
    meta: { title: 'springBoot' }
  },
  {
    path: '/settings',
    name: 'settings',
    component: SettingsPage,
    meta: { title: 'settings' }
  }
]

const router = createRouter({
  history: createWebHashHistory(),
  routes
})

export default router