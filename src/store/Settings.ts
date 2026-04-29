import { defineStore } from 'pinia'
import { ref } from 'vue'

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
}

export const useSettingsStore = defineStore('settings', () => {
  const defaultSettings: SettingsState = {
    showFloatBall: true,
    autoCacheMusic: true,
    filenameTemplate: '%t - %s',
    tagWriteOptions: { basicInfo: true, cover: true, lyrics: true, downloadLyrics: false, lyricFormat: 'word-by-word' },
    autoUpdate: true,
    autoImportPlaylistOnOpen: false,
    suppressImportPrompt: false,
    lyricFontFamily: 'PingFangSC-Semibold',
    lyricFontSize: 36,
    lyricFontWeight: 700,
    closeToTray: true,
    hasConfiguredCloseBehavior: false,
    theme: 'default',
    isDarkMode: false,
    springFestivalDisabled: false,
    routePreloadEnabled: true,
    globalBackground: { enable: false, type: 'none', url: '', opacity: 0.5, blur: 10, brightness: 0.8 }
  }

  const loadSettings = (): SettingsState => {
    try {
      const saved = localStorage.getItem('appSettings')
      if (saved) {
        const parsed = JSON.parse(saved) as SettingsState
        return { ...defaultSettings, ...parsed }
      }
    } catch (error) {
      console.error('加载设置失败:', error)
    }
    return { ...defaultSettings }
  }

  const settings = ref<SettingsState>(loadSettings())

  const saveSettings = () => {
    localStorage.setItem('appSettings', JSON.stringify(settings.value))
  }

  const updateSettings = (newSettings: Partial<SettingsState>) => {
    settings.value = { ...settings.value, ...newSettings }
    saveSettings()
  }

  const toggleFloatBall = () => {
    settings.value.showFloatBall = !settings.value.showFloatBall
    saveSettings()
  }

  return { settings, updateSettings, toggleFloatBall, saveSettings }
}, {
  persist: true
})
