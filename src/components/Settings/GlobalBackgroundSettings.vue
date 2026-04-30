<script setup lang="ts">
import { computed } from 'vue'
import { storeToRefs } from 'pinia'
import { convertFileSrc } from '@tauri-apps/api/core'
import { useSettingsStore } from '@/store/Settings'

const settingsStore = useSettingsStore()
const { settings } = storeToRefs(settingsStore)

const bgSettings = computed({
  get: () => settings.value.globalBackground || { enable: false, type: 'none', url: '', opacity: 0.5, blur: 10, brightness: 0.8 },
  set: (val) => settingsStore.updateSettings({ globalBackground: val })
})

const handleEnableChange = (val: any) => {
  settingsStore.updateSettings({
    globalBackground: { ...bgSettings.value, enable: Boolean(val) }
  })
}

const handleOpacityChange = (val: any) => {
  settingsStore.updateSettings({
    globalBackground: { ...bgSettings.value, opacity: Number(val) }
  })
}

const handleBlurChange = (val: any) => {
  settingsStore.updateSettings({
    globalBackground: { ...bgSettings.value, blur: Number(val) }
  })
}

const handleBrightnessChange = (val: any) => {
  settingsStore.updateSettings({
    globalBackground: { ...bgSettings.value, brightness: Number(val) }
  })
}

const selectFile = async () => {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const selected = await open({
      multiple: false,
      filters: [
        { name: 'Media Files', extensions: ['jpg', 'jpeg', 'png', 'gif', 'webp', 'mp4', 'webm', 'ogg'] },
        { name: 'Images', extensions: ['jpg', 'jpeg', 'png', 'gif', 'webp'] },
        { name: 'Videos', extensions: ['mp4', 'webm', 'ogg'] },
        { name: 'All Files', extensions: ['*'] }
      ]
    })
    if (selected) {
      const filePath = typeof selected === 'string' ? selected : (selected as any).path
      const ext = filePath.split('.').pop()?.toLowerCase() || ''
      const type: 'image' | 'video' = ['mp4', 'webm', 'ogg'].includes(ext) ? 'video' : 'image'
      settingsStore.updateSettings({
        globalBackground: { ...bgSettings.value, type, url: convertFileSrc(filePath) }
      })
    }
  } catch (error) {
    console.error('选择文件失败:', error)
  }
}

</script>

<template>
  <div class="global-bg-settings">
    <div class="setting-item">
      <div class="setting-info">
        <div class="setting-title">启用全局背景</div>
        <div class="setting-desc">设置应用的全局背景图片或视频</div>
      </div>
      <t-switch :value="bgSettings.enable" @change="handleEnableChange" />
    </div>

    <template v-if="bgSettings.enable">
      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-title">背景文件</div>
          <div class="setting-desc">选择您想要作为背景的文件（支持图片、视频、GIF）</div>
        </div>
        <div style="display: flex; gap: 8px; align-items: center;">
          <t-button size="small" @click="selectFile">选择文件</t-button>
          <div v-if="bgSettings.url" class="file-path" :title="bgSettings.url">{{ bgSettings.url.split('/').pop() }}</div>
        </div>
      </div>

      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-title">透明度</div>
          <div class="setting-desc">{{ Math.round(bgSettings.opacity * 100) }}%</div>
        </div>
        <t-slider :value="bgSettings.opacity" :min="0" :max="1" :step="0.05" style="width: 200px;" @change="handleOpacityChange" />
      </div>

      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-title">模糊度</div>
          <div class="setting-desc">{{ bgSettings.blur }}px</div>
        </div>
        <t-slider :value="bgSettings.blur" :min="0" :max="50" :step="1" style="width: 200px;" @change="handleBlurChange" />
      </div>

      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-title">亮度</div>
          <div class="setting-desc">{{ Math.round(bgSettings.brightness * 100) }}%</div>
        </div>
        <t-slider :value="bgSettings.brightness" :min="0" :max="2" :step="0.05" style="width: 200px;" @change="handleBrightnessChange" />
      </div>
    </template>
  </div>
</template>

<style scoped>
.global-bg-settings {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.setting-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.875rem 1rem;
  border: 1px solid var(--td-border-level-1-color);
  background: var(--td-bg-color-page);
  border-radius: 0.5rem;
}

.setting-info {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.setting-title {
  font-weight: 600;
  color: var(--td-text-color-primary);
  font-size: 0.95rem;
}

.setting-desc {
  color: var(--td-text-color-secondary);
  font-size: 0.8rem;
}

.file-path {
  font-size: 0.75rem;
  color: var(--td-text-color-secondary);
  max-width: 160px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
