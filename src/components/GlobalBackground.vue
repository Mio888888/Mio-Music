<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useSettingsStore } from '@/store/Settings'
import { storeToRefs } from 'pinia'
import { convertFileSrc } from '@tauri-apps/api/core'

const settingsStore = useSettingsStore()
const { settings } = storeToRefs(settingsStore)

const bgSettings = computed(() => settings.value.globalBackground)
const isEnabled = computed(() => bgSettings.value?.enable && bgSettings.value?.url)
const bgType = computed(() => bgSettings.value?.type || 'none')
const bgUrl = computed(() => {
  const url = bgSettings.value?.url || ''
  if (!url) return ''
  // 对旧的本地路径做兼容转换（已转为 asset URL 的不受影响）
  if (!url.startsWith('http') && !url.startsWith('asset:')) {
    return convertFileSrc(url)
  }
  return url
})
const bgOpacity = computed(() => bgSettings.value?.opacity ?? 0.5)
const bgBlur = computed(() => bgSettings.value?.blur ?? 10)
const bgBrightness = computed(() => bgSettings.value?.brightness ?? 0.8)

const videoRef = ref<HTMLVideoElement | null>(null)

const appContainerStyle = computed(() => {
  if (isEnabled.value) {
    return {
      '--td-bg-color-container': 'transparent',
      '--td-bg-color-page': 'transparent',
      '--td-bg-color-secondarycontainer': 'transparent'
    }
  }
  return {}
})

watch([bgType, bgUrl, isEnabled], ([type, _url, enabled]) => {
  if (enabled && type === 'video' && videoRef.value) {
    videoRef.value.load()
    videoRef.value.play().catch((e) => console.error('Video auto-play failed:', e))
  }
}, { immediate: true })

const styleElement = document.createElement('style')
styleElement.id = 'global-bg-transparency'

watch([isEnabled, () => settings.value.isDarkMode], ([enabled, isDark]) => {
  const appEl = document.getElementById('app')
  if (enabled) {
    if (appEl) appEl.style.backgroundColor = 'transparent'
    document.body.style.backgroundColor = 'transparent'
    if (!document.head.contains(styleElement)) document.head.appendChild(styleElement)
    const containerColor = isDark ? '36, 36, 36' : '255, 255, 255'
    const pageColor = isDark ? '24, 24, 24' : '243, 243, 243'
    const hoverColor = isDark ? '255, 255, 255' : '0, 0, 0'
    styleElement.innerHTML = `
      :root, body[theme-mode="dark"], body[theme-mode="light"] {
        --td-bg-color-container: rgba(${containerColor}, 0.3) !important;
        --td-bg-color-page: transparent !important;
        --td-bg-color-secondarycontainer: rgba(${pageColor}, 0.2) !important;
        --td-bg-color-component: rgba(${containerColor}, 0.3) !important;
        --td-bg-color-component-hover: rgba(${hoverColor}, 0.05) !important;
        --td-bg-color-component-active: rgba(${hoverColor}, 0.1) !important;
        --list-content-bg: rgba(${containerColor}, 0.3) !important;
        --player-bg-default: rgba(${containerColor}, 0.25) !important;
        --player-bg-idle: rgba(${containerColor}, 0.35) !important;
        --settings-main-bg: transparent !important;
        --settings-header-bg: rgba(${containerColor}, 0.3) !important;
        --settings-sidebar-bg: rgba(${containerColor}, 0.25) !important;
        --settings-content-bg: transparent !important;
        --settings-group-bg: rgba(${containerColor}, 0.3) !important;
        --settings-preview-bg: rgba(${containerColor}, 0.2) !important;
        --settings-feature-bg: rgba(${containerColor}, 0.2) !important;
        --settings-api-tips-bg: rgba(${containerColor}, 0.2) !important;
        --settings-source-card-bg: rgba(${containerColor}, 0.3) !important;
      }
      .home-container .sidebar {
        background-image: none !important;
        background-color: rgba(${containerColor}, 0.2) !important;
        backdrop-filter: blur(10px);
      }
      .home-container .header {
        background-color: transparent !important;
      }
      .mainContent {
        background-color: transparent !important;
      }
      .player-container {
        background: rgba(${containerColor}, 0.25) !important;
        backdrop-filter: blur(20px) !important;
      }
      .scrollable-content {
        background: rgba(${containerColor}, 0.3) !important;
        backdrop-filter: blur(8px);
      }
    `
  } else {
    if (appEl) appEl.style.backgroundColor = ''
    document.body.style.backgroundColor = ''
    if (document.head.contains(styleElement)) document.head.removeChild(styleElement)
  }
}, { immediate: true })
</script>

<template>
  <div v-if="isEnabled" class="global-background-container" :style="appContainerStyle">
    <div class="global-background-media" :style="{ opacity: bgOpacity, filter: `blur(${bgBlur}px) brightness(${bgBrightness})` }">
      <video v-if="bgType === 'video'" ref="videoRef" :src="bgUrl" loop muted autoplay playsinline class="bg-video"></video>
      <div v-else class="bg-image" :style="{ backgroundImage: `url('${bgUrl}')` }"></div>
    </div>
  </div>
</template>

<style scoped>
.global-background-container {
  position: fixed; top: 0; left: 0; width: 100vw; height: 100vh;
  z-index: -999; pointer-events: none; overflow: hidden;
  background-color: var(--td-bg-color-page);
}
.global-background-media {
  position: absolute; top: -10%; left: -10%; width: 120%; height: 120%;
  transition: all 0.3s ease;
}
.bg-video { width: 100%; height: 100%; object-fit: cover; }
.bg-image { width: 100%; height: 100%; background-size: cover; background-position: center; background-repeat: no-repeat; }
</style>
