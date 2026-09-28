import { createRouter, createWebHashHistory } from 'vue-router'
import { useLockStore } from '@/stores/lock'

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
    path: '/monitor',
    name: 'monitor',
    component: () => import('@/modules/monitor/pages/MonitorCenterPage.vue'),
    meta: { title: 'monitorCenter' },
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
    path: '/dns-accounts',
    name: 'dnsAccounts',
    component: () => import('@/modules/dns-accounts/pages/DnsAccountsPage.vue'),
    meta: { title: 'dnsAccounts' },
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
    path: '/about',
    name: 'about',
    component: () => import('@/modules/settings/pages/AboutPage.vue'),
    meta: { title: 'about' },
  },
  {
    path: '/stacks',
    name: 'stacks',
    component: () => import('@/modules/stack/StackList.vue'),
    meta: { title: 'stacks' },
  },
  {
    path: '/lock',
    name: 'lock',
    component: () => import('@/modules/settings/pages/LockScreen.vue'),
    meta: { title: 'lockScreen' },
  },
]

const router = createRouter({
  history: createWebHashHistory(),
  routes,
})

// 锁屏守卫：仅「有锁」时才拦截。被拦截时记下目标路由，解锁后回跳；
// 无锁访问 /lock 视为异常，重定向走，避免死锁在锁屏页。
router.beforeEach((to) => {
  const lock = useLockStore()
  if (to.path === '/lock') {
    if (!lock.hasPassword) return { path: '/' }
    return true
  }
  if (lock.hasPassword && !lock.unlocked) {
    lock.pendingRoute = to.fullPath
    return { path: '/lock' }
  }
  return true
})

export default router
