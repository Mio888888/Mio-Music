<template>
  <n-config-provider :theme="isDark ? darkTheme : undefined" :theme-overrides="themeOverrides">
    <n-message-provider><n-dialog-provider><nglobal-style />
      <GlobalAudio />
      <slot />
    </n-dialog-provider></n-message-provider>
  </n-config-provider>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { darkTheme } from 'naive-ui'
import { NConfigProvider, NMessageProvider, NDialogProvider, NGlobalStyle as NglobalStyle } from 'naive-ui'
import { useSettingsStore } from '@/store/Settings'
import GlobalAudio from '@/components/Play/GlobalAudio.vue'
import { storeToRefs } from 'pinia'
import '@/assets/theme/blue.css'
import '@/assets/theme/pink.css'
import '@/assets/theme/orange.css'
import '@/assets/theme/cyan.css'

const settingsStore = useSettingsStore()
const { settings } = storeToRefs(settingsStore)
const isDark = computed(() => settings.value.isDarkMode)

const themes = [
  { name: 'default', color: '#2ba55b' },
  { name: 'pink', color: '#fc5e7e' },
  { name: 'blue', color: '#57b4ff' },
  { name: 'cyan', color: '#3ac2b8' },
  { name: 'orange', color: '#fb9458' }
]

const themeVersion = ref(0)

const themeOverrides = computed(() => {
  void themeVersion.value
  const docEl = document.documentElement
  const computedStyle = getComputedStyle(docEl)
  const primary = (computedStyle.getPropertyValue('--td-brand-color') || '').trim()
  const currentTheme = settings.value.theme || 'default'
  const fallback = themes.find((t) => t.name === currentTheme)?.color || '#2ba55b'
  const mainColor = primary || fallback
  return {
    common: {
      primaryColor: mainColor,
      primaryColorHover: mainColor,
      primaryColorPressed: mainColor
    }
  }
})

watch(() => settings.value.isDarkMode, (dark) => {
  document.documentElement.setAttribute('data-theme', dark ? 'dark' : 'light')
}, { immediate: true })

watch(() => settings.value.theme, (theme) => {
  document.documentElement.setAttribute('theme-mode', theme ?? 'default')
  themeVersion.value++
}, { immediate: true })

onMounted(() => {
  const prefersDark = window.matchMedia('(prefers-color-scheme: dark)')
  if (settings.value.isDarkMode === undefined) {
    settingsStore.updateSettings({ isDarkMode: prefersDark.matches })
  }
  prefersDark.addEventListener('change', (e) => {
    settingsStore.updateSettings({ isDarkMode: e.matches })
  })
})
</script>
