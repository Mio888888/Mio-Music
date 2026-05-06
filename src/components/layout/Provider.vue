<template>
  <n-config-provider :theme-overrides="themeOverrides">
    <n-message-provider><n-dialog-provider><nglobal-style />
      <GlobalAudio />
      <AIChat />
      <slot />
    </n-dialog-provider></n-message-provider>
  </n-config-provider>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { NConfigProvider, NMessageProvider, NDialogProvider, NGlobalStyle as NglobalStyle } from 'naive-ui'
import { useDynamicSongTheme } from '@/composables/useDynamicSongTheme'
import GlobalAudio from '@/components/Play/GlobalAudio.vue'

const themeVersion = ref(0)

useDynamicSongTheme({
  onThemeChange: () => {
    themeVersion.value++
  }
})

const themeOverrides = computed(() => {
  void themeVersion.value
  const docEl = document.documentElement
  const computedStyle = getComputedStyle(docEl)
  const primary = (computedStyle.getPropertyValue('--td-brand-color') || '').trim()
  const mainColor = primary || '#2ba55b'
  return {
    common: {
      primaryColor: mainColor,
      primaryColorHover: mainColor,
      primaryColorPressed: mainColor
    }
  }
})
</script>
