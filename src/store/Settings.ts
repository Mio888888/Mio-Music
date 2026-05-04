import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { BackgroundRenderSettings } from '@/types/background'
import { DEFAULT_BACKGROUND_RENDER_SETTINGS } from '@/types/background'

export interface TagWriteOptions {
  basicInfo: boolean
  cover: boolean
  lyrics: boolean
  downloadLyrics: boolean
  lyricFormat: 'lrc' | 'word-by-word'
}

export interface GlobalBackgroundSettings {
  enable: boolean
  type: 'image' | 'video' | 'none'
  url: string
  opacity: number
  blur: number
  brightness: number
}

export interface SettingsState {
  showFloatBall: boolean
  autoCacheMusic?: boolean
  cacheSizeLimit?: number
  directories?: { cacheDir: string; downloadDir: string }
  filenameTemplate?: string
  tagWriteOptions?: TagWriteOptions
  autoUpdate?: boolean
  autoImportPlaylistOnOpen?: boolean
  suppressImportPrompt?: boolean
  lyricFontFamily?: string
  lyricFontSize?: number
  FullPlayLyricFontRate?: number
  lyricFontWeight?: number
  closeToTray?: boolean
  hasConfiguredCloseBehavior?: boolean
  theme?: string
  isDarkMode?: boolean
  springFestivalDisabled?: boolean
  routePreloadEnabled?: boolean
  globalBackground?: GlobalBackgroundSettings
  backgroundRender?: BackgroundRenderSettings
}

export const useSettingsStore = defineStore('settings', () => {
  const defaultSettings: SettingsState = {
    showFloatBall: true,
    autoCacheMusic: true,
    cacheSizeLimit: 1073741824,
    filenameTemplate: '%t - %s',
    tagWriteOptions: { basicInfo: true, cover: true, lyrics: true, downloadLyrics: false, lyricFormat: 'word-by-word' },
    autoUpdate: true,
    autoImportPlaylistOnOpen: false,
    suppressImportPrompt: false,
    lyricFontFamily: 'lyricfont',
    lyricFontSize: 36,
    lyricFontWeight: 700,
    closeToTray: true,
    hasConfiguredCloseBehavior: false,
    theme: 'default',
    isDarkMode: false,
    springFestivalDisabled: false,
    routePreloadEnabled: true,
    globalBackground: { enable: false, type: 'none', url: '', opacity: 0.5, blur: 10, brightness: 0.8 },
    backgroundRender: DEFAULT_BACKGROUND_RENDER_SETTINGS
  }

  const loadSettings = (): SettingsState => {
    try {
      const saved = localStorage.getItem('appSettings')
      if (saved) {
        const parsed = JSON.parse(saved) as SettingsState
        return {
          ...defaultSettings,
          ...parsed,
          tagWriteOptions: {
            basicInfo: parsed.tagWriteOptions?.basicInfo ?? (defaultSettings.tagWriteOptions as TagWriteOptions).basicInfo,
            cover: parsed.tagWriteOptions?.cover ?? (defaultSettings.tagWriteOptions as TagWriteOptions).cover,
            lyrics: parsed.tagWriteOptions?.lyrics ?? (defaultSettings.tagWriteOptions as TagWriteOptions).lyrics,
            downloadLyrics: parsed.tagWriteOptions?.downloadLyrics ?? (defaultSettings.tagWriteOptions as TagWriteOptions).downloadLyrics,
            lyricFormat: parsed.tagWriteOptions?.lyricFormat ?? (defaultSettings.tagWriteOptions as TagWriteOptions).lyricFormat
          },
          backgroundRender: {
            fullPlay: {
              ...(DEFAULT_BACKGROUND_RENDER_SETTINGS.fullPlay),
              ...(parsed.backgroundRender?.fullPlay ?? {})
            }
          }
        }
      }
    } catch (error) {
      console.error('加载设置失败:', error)
    }
    return { ...defaultSettings }
  }

  const settings = ref<SettingsState>(loadSettings())

  const saveSettings = () => {
    if (typeof settings.value.autoCacheMusic === 'undefined') settings.value.autoCacheMusic = true
    if (!settings.value.lyricFontFamily) settings.value.lyricFontFamily = 'lyricfont'
    if (!settings.value.lyricFontSize) settings.value.lyricFontSize = 36
    if (!settings.value.FullPlayLyricFontRate) settings.value.FullPlayLyricFontRate = 1
    if (!settings.value.lyricFontWeight) settings.value.lyricFontWeight = 700
    if (typeof settings.value.closeToTray === 'undefined') settings.value.closeToTray = true
    if (typeof settings.value.hasConfiguredCloseBehavior === 'undefined') settings.value.hasConfiguredCloseBehavior = false
    if (!settings.value.theme) settings.value.theme = 'default'
    if (typeof settings.value.isDarkMode === 'undefined') settings.value.isDarkMode = false
    if (typeof settings.value.springFestivalDisabled === 'undefined') settings.value.springFestivalDisabled = false
    if (typeof settings.value.routePreloadEnabled === 'undefined') settings.value.routePreloadEnabled = true
    if (!settings.value.globalBackground) {
      settings.value.globalBackground = { enable: false, type: 'none', url: '', opacity: 0.5, blur: 10, brightness: 0.8 }
    }
    if (!settings.value.tagWriteOptions) {
      settings.value.tagWriteOptions = { basicInfo: true, cover: true, lyrics: true, downloadLyrics: false, lyricFormat: 'word-by-word' }
    }
    if (!settings.value.backgroundRender) {
      settings.value.backgroundRender = DEFAULT_BACKGROUND_RENDER_SETTINGS
    }
    localStorage.setItem('appSettings', JSON.stringify(settings.value))
  }

  const updateSettings = (newSettings: Partial<SettingsState>) => {
    settings.value = { ...settings.value, ...newSettings }
    if (
      settings.value.FullPlayLyricFontRate &&
      (settings.value.FullPlayLyricFontRate < 0.1 || settings.value.FullPlayLyricFontRate > 2)
    ) {
      settings.value.FullPlayLyricFontRate = 1
    }
    saveSettings()
  }

  const toggleFloatBall = () => {
    settings.value.showFloatBall = !settings.value.showFloatBall
    saveSettings()
  }

  const isSpringFestivalWindow = () => {
    const now = new Date()
    const today = now.getFullYear() * 10000 + (now.getMonth() + 1) * 100 + now.getDate()
    return today >= 20260217 && today <= 20260223
  }

  const shouldUseSpringFestivalTheme = () => {
    const preview = localStorage.getItem('ceru_welcome_newyear_preview')
    if (preview === '1') return true
    return isSpringFestivalWindow()
  }

  const disableSpringFestivalTheme = () => {
    settings.value.springFestivalDisabled = true
    saveSettings()
  }

  const enableSpringFestivalTheme = () => {
    settings.value.springFestivalDisabled = false
    saveSettings()
  }

  return {
    settings,
    updateSettings,
    toggleFloatBall,
    saveSettings,
    isSpringFestivalWindow,
    shouldUseSpringFestivalTheme,
    disableSpringFestivalTheme,
    enableSpringFestivalTheme
  }
}, {
  persist: false
})
