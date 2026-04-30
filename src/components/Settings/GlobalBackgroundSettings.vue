<script setup lang="ts">
import { computed } from 'vue'
import { storeToRefs } from 'pinia'
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

const handleTypeChange = (val: any) => {
  const typeVal = String(val) as 'image' | 'video' | 'none'
  settingsStore.updateSettings({
    globalBackground: { ...bgSettings.value, type: typeVal, url: typeVal === 'none' ? '' : bgSettings.value.url }
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
      filters: [{
        name: 'Media',
        extensions: ['jpg', 'jpeg', 'png', 'gif', 'webp', 'mp4', 'webm', 'ogg']
      }]
    })
    if (selected) {
      const filePath = typeof selected === 'string' ? selected : (selected as any).path
      settingsStore.updateSettings({
        globalBackground: { ...bgSettings.value, url: filePath }
      })
    }
  } catch (error) {
    console.error('选择文件失败:', error)
  }
}

const clearBackground = () => {
  settingsStore.updateSettings({
    globalBackground: { ...bgSettings.value, url: '', type: 'none' }
  })
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
          <div class="setting-title">背景类型</div>
        </div>
        <t-radio-group :value="bgSettings.type" @change="handleTypeChange">
          <t-radio-button value="none">无</t-radio-button>
          <t-radio-button value="image">图片</t-radio-button>
          <t-radio-button value="video">视频</t-radio-button>
        </t-radio-group>
      </div>

      <div v-if="bgSettings.type !== 'none'" class="setting-item">
        <div class="setting-info">
          <div class="setting-title">{{ bgSettings.type === 'image' ? '背景图片' : '背景视频' }}</div>
          <div class="setting-desc">{{ bgSettings.url || '未选择文件' }}</div>
        </div>
        <div style="display: flex; gap: 8px;">
          <t-button size="small" @click="selectFile">选择文件</t-button>
          <t-button v-if="bgSettings.url" size="small" theme="danger" variant="outline" @click="clearBackground">清除</t-button>
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
</style>
