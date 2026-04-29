<template>
  <div class="settings-page">
    <div class="settings-sidebar">
      <h3>设置</h3>
      <div
        v-for="cat in categories"
        :key="cat.key"
        class="sidebar-item"
        :class="{ active: activeKey === cat.key }"
        @click="activeKey = cat.key"
      >
        <span class="sidebar-icon">{{ cat.icon }}</span>
        <span>{{ cat.label }}</span>
      </div>
    </div>
    <div class="settings-content" ref="contentRef">
      <KeepAlive>
        <component :is="currentComponent" />
      </KeepAlive>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, markRaw } from 'vue'
import PluginSection from './sections/PluginSection.vue'
import HotkeySettings from './sections/HotkeySettings.vue'
import UpdateSettings from './sections/UpdateSettings.vue'

const categories = [
  { key: 'plugins', label: '插件管理', icon: '🔌', component: markRaw(PluginSection) },
  { key: 'hotkeys', label: '快捷键', icon: '⌨️', component: markRaw(HotkeySettings) },
  { key: 'update', label: '版本更新', icon: '🔄', component: markRaw(UpdateSettings) },
]

const activeKey = ref('plugins')
const contentRef = ref<HTMLElement>()

const currentComponent = computed(() => {
  return categories.find(c => c.key === activeKey.value)?.component
})
</script>

<style scoped>
.settings-page {
  display: flex;
  height: 100%;
  background: var(--td-bg-color-page);
  color: var(--td-text-color-primary);
}
.settings-sidebar {
  width: 240px;
  padding: 24px 16px;
  border-right: 1px solid var(--td-border-level-1-color);
  flex-shrink: 0;
}
.settings-sidebar h3 {
  font-size: 1.25rem;
  font-weight: 600;
  margin: 0 0 20px 8px;
}
.sidebar-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border-radius: 8px;
  cursor: pointer;
  font-size: 14px;
  transition: all 0.15s;
  margin-bottom: 2px;
}
.sidebar-item:hover { background: var(--td-bg-color-container-hover); }
.sidebar-item.active { background: var(--td-brand-color-1); color: var(--td-brand-color); font-weight: 500; }
.sidebar-icon { font-size: 18px; width: 24px; text-align: center; }
.settings-content {
  flex: 1;
  padding: 24px 32px;
  overflow-y: auto;
}
</style>
