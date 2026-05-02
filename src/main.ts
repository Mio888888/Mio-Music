import './assets/base.css'
import './assets/icon_font/iconfont.css'
import './assets/icon_font/iconfont.js'
import './assets/main.css'

import App from './App.vue'
import { createApp } from 'vue'
import router from './router'
import { createPinia } from 'pinia'
import piniaPluginPersistedstate from 'pinia-plugin-persistedstate'
import LogtoClient from '@logto/browser'

// IPC adapter layer
import './bridge'

// 全局 fetch 拦截器：代理跨域图片请求到 Rust 后端，绕过 WebView CORS
import './utils/cors-proxy'

// Initialize Logto client
import config from './config'
config.instance = new LogtoClient({
  appId: config.appId,
  endpoint: config.endpoint,
})

const app = createApp(App)

// 过滤 TDesign TPopup 已知的 slot 警告（TPopupTrigger 在 render 外调用 slot）
app.config.warnHandler = (msg) => {
  if (typeof msg === 'string' && msg.includes('invoked outside of the render function')) return
  console.warn(msg)
}

const pinia = createPinia()
pinia.use(piniaPluginPersistedstate)
app.use(pinia)
app.use(router)
app.mount('#app')
