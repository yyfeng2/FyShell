/**
 * Pinia 状态管理初始化
 *
 * P0 阶段仅做基础初始化；后续状态模块（会话树、传输队列等）
 * 在各 store 文件中通过 defineStore 定义，此处统一 createPinia 注册。
 */
import { createPinia } from 'pinia'

export const pinia = createPinia()
