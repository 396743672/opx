# 设置页 Tab 分区设计

> 状态：已确认设计
> 日期：2026-09-16
> 范围：`SettingsPage.vue` 布局重构（纯前端，不改任何设置字段与保存逻辑）

## 背景

`src/modules/settings/pages/SettingsPage.vue` 已 659 行、10 个区块纵向堆叠（外观 / 关闭行为 / 开机自启 / 下载代理 / DNS 服务商 / 告警阈值 / 告警通知 / DDNS / 配置备份），单页过长、定位靠滚动。

## 已确认决策

1. **4 个 Tab 按功能域分组**（见下表）。
2. **选中状态记入 URL query**（`?tab=...`）——刷新保持、可从别处直达、非法值回落首 Tab。
3. **保存行为不变**：仍为「任何字段改动 400ms 后自动保存」，切 Tab 不影响。
4. 复用现有 `CategoryTabs` 组件（`src/modules/software-manager/components/CategoryTabs.vue`），**不新建组件**。
5. 用 `v-show` 切换而非 `v-if`——保留已加载的表单 ref，避免重复挂载引起的一次加载竞态。

## 设计

### 1. Tab 分组

| key | 标签（zh / en） | 图标 | 包含区块 |
|---|---|---|---|
| `general` | 通用 / General | `mdi:tune` | 外观、关闭行为、开机自启、下载代理 |
| `monitor` | 监控与告警 / Monitoring | `mdi:bell-outline` | 告警阈值、告警通知（webhook + SMTP） |
| `dns` | 域名与 DNS / Domains & DNS | `mdi:dns-outline` | DNS 服务商（证书自动化）、DDNS 动态域名 |
| `data` | 数据与维护 / Data | `mdi:database-cog-outline` | 配置备份（及其后所有尾部区块） |

分组依据：外部依赖（证书/DDNS 都要 DNS 凭证）放同一 Tab；监控类同域；其余按「与本机行为」vs「数据维护」划分。

### 2. 结构

```
PageHeader  设置
CategoryTabs（复用组件，v-model="activeTab"）
<div v-show="activeTab === 'general'">  原先的四个区块（保持各自 border-t 分隔）
<div v-show="activeTab === 'monitor'">  告警阈值 + 告警通知
<div v-show="activeTab === 'dns'">      DNS 服务商 + DDNS
<div v-show="activeTab === 'data'">     配置备份 + 尾部区块
```

每个 Tab 内沿用现有的 `px-5 pt-4 pb-1` 标题 + `divide-y divide-border` 区块写法，**外观样式零改动**——只是把现有 DOM 分到四个容器里。

外层 `max-w-2xl` 保留（当前就是窄栏，Tab 不改变宽度）。

### 3. 状态与路由

```ts
const route = useRoute()
const router = useRouter()
const TAB_KEYS = ['general', 'monitor', 'dns', 'data'] as const

// 非法/缺失值回落首 Tab
const activeTab = ref<string>(
  TAB_KEYS.includes(route.query.tab as never) ? (route.query.tab as string) : 'general'
)

watch(activeTab, (t) => {
  // replace 而非 push：切 Tab 不该污染浏览器历史
  router.replace({ query: { ...route.query, tab: t } })
})
```

- 从别处直达：`router.push('/settings?tab=dns')`
- 刷新：query 保留 → 停在原 Tab
- 不新增路由（仍是单条 `/settings`）

### 4. i18n

`zh-CN.ts` / `en-US.ts` 同位置新增 4 键：

| key | zh-CN | en-US |
|---|---|---|
| `settingsTabGeneral` | 通用 | General |
| `settingsTabMonitor` | 监控与告警 | Monitoring |
| `settingsTabDns` | 域名与 DNS | Domains & DNS |
| `settingsTabData` | 数据与维护 | Data |

### 5. 验证

- `npx vue-tsc --noEmit` 零错误；`npm run build` 成功。
- 实机清单：
  1. 四个 Tab 显示正常，点击切换内容对应
  2. 刷新页面停在原 Tab（看 URL 有 `?tab=...`）
  3. 手改 URL 为 `?tab=dns` 回车 → 直接进入该 Tab
  4. 手改 URL 为 `?tab=nonsense` → 回到「通用」（不报错、不空白）
  5. 切 Tab 后返回上一页再前进：浏览器历史里没有一堆 Tab 记录（`replace` 生效）
  6. 各 Tab 内的设置项功能不变：改值 → 400ms 后自动保存 → 重启后仍在
  7. 中英切换：4 个 Tab 标签都有文案

## 边界（不做）

- 不改任何 `AppSettings` 字段、`save()` 逻辑、加载逻辑。
- 不新建 Tab 组件（复用 `CategoryTabs`）。
- 不做 Tab 切换动画、不做 Tab 内二次分页、不做设置搜索。
- 不调整现有区块的视觉样式（配色/间距/组件）。

## 改动文件

- `src/modules/settings/pages/SettingsPage.vue`（模板分区 + `activeTab` 状态）
- `src/locales/zh-CN.ts`、`src/locales/en-US.ts`（4 键）
