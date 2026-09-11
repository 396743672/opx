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
    path: '/websites',
    name: 'websites',
    component: () => import('@/modules/website-manager/pages/WebsiteListPage.vue'),
    meta: { title: 'websiteManagement' },
  },
  {
    path: '/springboot',
    name: 'springboot',
    component: () => import('@/modules/springboot-manager/pages/SpringBootPage.vue'),
    meta: { title: 'springBoot' },
  },
  {
    path: '/node-apps',
    name: 'node-apps',
    component: () => import('@/modules/node-apps/NodeAppsPage.vue'),
    meta: { title: 'nodeApps' },
  },
  {
    path: '/audit',
    name: 'audit',
    component: () => import('@/modules/audit/pages/AuditLogPage.vue'),
    meta: { title: 'auditLog' },
  },
  {
    path: '/settings',
    name: 'settings',
    component: () => import('@/modules/settings/pages/SettingsPage.vue'),
    meta: { title: 'settings' },
  },
  {
    path: '/stacks',
    name: 'stacks',
    component: () => import('@/modules/stack/StackList.vue'),
    meta: { title: 'stacks' },
  },
]

const router = createRouter({
  history: createWebHashHistory(),
  routes,
})

export default router
