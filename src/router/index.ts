import { createRouter, createWebHashHistory } from 'vue-router'

const routes = [
  {
    path: '/',
    redirect: '/dashboard',
  },
  {
    path: '/dashboard',
    name: 'dashboard',
    component: () => import('@/modules/system-monitor/pages/DashboardPage.vue'),
    meta: { title: 'systemMonitor' },
  },
  {
    path: '/repository',
    name: 'repository',
    component: () => import('@/modules/software-manager/pages/RepositoryPage.vue'),
    meta: { title: 'softwareRepository' },
  },
  {
    path: '/software',
    name: 'software',
    component: () => import('@/modules/software-manager/pages/SoftwareListPage.vue'),
    meta: { title: 'softwareManagement' },
  },
  {
    path: '/springboot',
    name: 'springboot',
    component: () => import('@/modules/springboot-manager/pages/SpringBootPage.vue'),
    meta: { title: 'springBoot' },
  },
  {
    path: '/settings',
    name: 'settings',
    component: () => import('@/modules/settings/pages/SettingsPage.vue'),
    meta: { title: 'settings' },
  },
]

const router = createRouter({
  history: createWebHashHistory(),
  routes,
})

export default router
