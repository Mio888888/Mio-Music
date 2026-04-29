<template>
  <n-config-provider :theme="isDark ? darkTheme : undefined" :theme-overrides="themeOverrides">
    <n-message-provider><n-dialog-provider><nglobal-style />
      <GlobalAudio />
      <slot />
    </n-dialog-provider></n-message-provider>
  </n-config-provider>
</template>

<script setup lang="ts">
import { computed, onMounted, watch } from 'vue'
import { darkTheme } from 'naive-ui'
import { NConfigProvider, NMessageProvider, NDialogProvider, NGlobalStyle as NglobalStyle } from 'naive-ui'
import { useSettingsStore } from '@/store/Settings'
import GlobalAudio from '@/components/Play/GlobalAudio.vue'
import { storeToRefs } from 'pinia'

const settingsStore = useSettingsStore()
const { settings } = storeToRefs(settingsStore)
const isDark = computed(() => settings.value.isDarkMode)

const themeOverrides = computed(() => ({
  common: { primaryColor: settings.value.theme === 'default' ? '#00DAC0' : undefined }
}))

watch(() => settings.value.isDarkMode, (dark) => {
  document.documentElement.setAttribute('data-theme', dark ? 'dark' : 'light')
}, { immediate: true })

watch(() => settings.value.theme, (theme) => {
  document.documentElement.setAttribute('theme-mode', theme ?? 'default')
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
