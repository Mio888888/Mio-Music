import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

export interface S3Config {
  endpoint: string
  region: string
  accessKeyId: string
  secretAccessKey: string
  bucket: string
}

export interface BackupData {
  version: number
  timestamp: string
  playlists: any
  settings: any
}

type RestoreMode = 'overwrite' | 'merge'

const STORAGE_KEY = 's3BackupConfig'

export const useS3BackupStore = defineStore('s3Backup', () => {
  const config = ref<S3Config>({
    endpoint: '',
    region: 'auto',
    accessKeyId: '',
    secretAccessKey: '',
    bucket: ''
  })

  const isConnected = ref(false)
  const isConnecting = ref(false)
  const isBackingUp = ref(false)
  const isRestoring = ref(false)
  const lastBackupTime = ref<string | null>(null)
  const errorMessage = ref<string | null>(null)

  const statusText = computed(() => {
    if (isConnecting.value) return '连接中...'
    if (isBackingUp.value) return '备份中...'
    if (isRestoring.value) return '恢复中...'
    if (isConnected.value) return '已连接'
    return '未连接'
  })

  function loadConfig() {
    try {
      const saved = localStorage.getItem(STORAGE_KEY)
      if (saved) {
        const parsed = JSON.parse(saved)
        config.value = { ...config.value, ...parsed }
      }
      const time = localStorage.getItem('lastBackupTime')
      if (time) lastBackupTime.value = time
    } catch (e) {
      console.error('加载 S3 配置失败:', e)
    }
  }

  function saveConfig() {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(config.value))
  }

  function toApiConfig(): Record<string, string> {
    return {
      endpoint: config.value.endpoint,
      region: config.value.region || 'auto',
      accessKeyId: config.value.accessKeyId,
      secretAccessKey: config.value.secretAccessKey,
      bucket: config.value.bucket
    }
  }

  async function testConnection(): Promise<boolean> {
    isConnecting.value = true
    errorMessage.value = null
    try {
      await (window as any).api.s3.testConnection(toApiConfig())
      isConnected.value = true
      saveConfig()
      return true
    } catch (e: any) {
      isConnected.value = false
      errorMessage.value = e?.message || e?.toString() || '连接失败'
      return false
    } finally {
      isConnecting.value = false
    }
  }

  async function backup(): Promise<boolean> {
    if (!isConnected.value) {
      errorMessage.value = '请先连接 S3'
      return false
    }

    isBackingUp.value = true
    errorMessage.value = null
    try {
      const playlists = JSON.parse(localStorage.getItem('playlists') || '[]')
      const settings = JSON.parse(localStorage.getItem('appSettings') || '{}')

      const result = await (window as any).api.s3.backup(
        toApiConfig(),
        playlists,
        settings
      )

      lastBackupTime.value = result.timestamp
      localStorage.setItem('lastBackupTime', result.timestamp)
      return true
    } catch (e: any) {
      errorMessage.value = e?.message || e?.toString() || '备份失败'
      return false
    } finally {
      isBackingUp.value = false
    }
  }

  async function restore(mode: RestoreMode = 'overwrite'): Promise<boolean> {
    if (!isConnected.value) {
      errorMessage.value = '请先连接 S3'
      return false
    }

    isRestoring.value = true
    errorMessage.value = null
    try {
      const result = await (window as any).api.s3.restore(toApiConfig())
      const data: BackupData = result.data

      if (mode === 'overwrite') {
        localStorage.setItem('playlists', JSON.stringify(data.playlists))
        localStorage.setItem('appSettings', JSON.stringify(data.settings))
      } else {
        const existing = JSON.parse(localStorage.getItem('playlists') || '[]')
        const merged = [...existing]
        for (const pl of data.playlists) {
          if (!merged.some((e: any) => e.id === pl.id)) {
            merged.push(pl)
          }
        }
        localStorage.setItem('playlists', JSON.stringify(merged))

        const localSettings = JSON.parse(localStorage.getItem('appSettings') || '{}')
        const mergedSettings = { ...data.settings, ...localSettings }
        localStorage.setItem('appSettings', JSON.stringify(mergedSettings))
      }

      return true
    } catch (e: any) {
      errorMessage.value = e?.message || e?.toString() || '恢复失败'
      return false
    } finally {
      isRestoring.value = false
    }
  }

  // Initialize
  loadConfig()
  if (config.value.endpoint && config.value.accessKeyId) {
    testConnection()
  }

  return {
    config,
    isConnected,
    isConnecting,
    isBackingUp,
    isRestoring,
    lastBackupTime,
    errorMessage,
    statusText,
    testConnection,
    backup,
    restore,
    saveConfig
  }
})
