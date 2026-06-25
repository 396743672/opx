# 创建的前端基础结构文件

## 配置文件
- `tailwind.config.ts` - Tailwind CSS 配置文件
- `vite.config.ts` - Vite 构建配置文件
- `tsconfig.json` - TypeScript 配置文件
- `package.json` - 项目依赖配置文件
- `index.html` - 项目入口 HTML 文件

## 源代码目录
### 根文件
- `src/main.ts` - 应用程序入口文件
- `src/App.vue` - 根组件
- `src/vite-env.d.ts` - Vite 环境类型定义

### 样式文件
- `src/styles/main.css` - 全局样式和 Tailwind 入口

### 路由配置
- `src/router/index.ts` - Vue Router 路由配置

### 状态管理
- `src/stores/app.ts` - 应用状态存储
- `src/stores/settings.ts` - 设置状态存储

### 数据模型
- `src/models/settings.ts` - 设置数据类型定义

### 工具函数
- `src/utils/i18n.ts` - 国际化工具函数

### 组件
- `src/layouts/MainLayout.vue` - 主布局组件
- `src/components/Sidebar.vue` - 侧边栏组件
- `src/components/Header.vue` - 页头组件
- `src/components/ThemeToggle.vue` - 主题切换组件
- `src/components/LanguageToggle.vue` - 语言切换组件
- `src/components/UserMenu.vue` - 用户菜单组件

### 模块页面
#### 系统监控
- `src/modules/system-monitor/pages/DashboardPage.vue` - 系统监控仪表盘

#### 软件管理
- `src/modules/software-manager/pages/SoftwareListPage.vue` - 软件列表页面
- `src/modules/software-manager/pages/RepositoryPage.vue` - 软件仓库页面

#### Spring Boot管理
- `src/modules/springboot-manager/pages/SpringBootPage.vue` - Spring Boot项目管理页面

#### 设置
- `src/modules/settings/pages/SettingsPage.vue` - 设置页面

### 国际化
- `src/locales/zh-CN.json` - 中文翻译
- `src/locales/en.json` - 英文翻译
