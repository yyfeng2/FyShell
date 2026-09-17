/**
 * FyShell 前端入口
 *
 * 职责：createApp、注册 pinia 与 vuetify 插件、加载全局样式、挂载 #app
 */
import { createApp } from 'vue'
import App from './App.vue'
import { pinia } from './plugins/pinia'
import { vuetify } from './plugins/vuetify'
import './styles/theme.css'

const app = createApp(App)

// 注册状态管理
app.use(pinia)

// 注册 Vuetify（深色主题默认基调，dark-light-auto 跟随系统切换在 App.vue 中完成）
app.use(vuetify)

// 全局错误兜底：组件事件处理抛错时打印详情（否则表现为"按钮没反应"且无任何报错）
app.config.errorHandler = (err, _instance, info) => {
  console.error('[FyShell] Vue 运行时错误:', err, info)
}

app.mount('#app')
