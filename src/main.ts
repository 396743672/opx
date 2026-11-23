import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import router from './router'
import './styles/main.css'
import './utils/icons'
import { i18n } from './utils/i18n'
import { installRuntimeErrorHandlers, reportRuntimeError } from './utils/runtimeError'

installRuntimeErrorHandlers()

const pinia = createPinia()

const app = createApp(App)
// 设了 errorHandler 后 Vue 不再自己打日志：这里一并转交给提示条 + console
app.config.errorHandler = (err) => reportRuntimeError(err)
app.use(pinia)
app.use(router)
app.use(i18n)
app.mount('#app')
