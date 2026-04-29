import './assets/base.css'
import 'animate.css'
import './assets/icon_font/iconfont.css'
import './assets/icon_font/iconfont.js'
import './assets/main.css'

import App from './App.vue'
import { createApp } from 'vue'
import router from './router'
import { createPinia } from 'pinia'
import piniaPluginPersistedstate from 'pinia-plugin-persistedstate'

// IPC adapter layer
import './bridge'

const app = createApp(App)
const pinia = createPinia()
pinia.use(piniaPluginPersistedstate)
app.use(pinia)
app.use(router)
app.mount('#app')
