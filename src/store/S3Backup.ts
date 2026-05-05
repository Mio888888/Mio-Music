import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import {
  S3Client,
  PutObjectCommand,
  GetObjectCommand,
  HeadBucketCommand,
  ListObjectsV2Command
} from '@aws-sdk/client-s3'

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
const BACKUP_KEY_PREFIX = 'mio-backup-'

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

  function createClient(): S3Client {
    return new S3Client({
      endpoint: config.value.endpoint,
      region: config.value.region || 'auto',
      credentials: {
        accessKeyId: config.value.accessKeyId,
        secretAccessKey: config.value.secretAccessKey
      },
      forcePathStyle: true
    })
  }

  async function testConnection(): Promise<boolean> {
    isConnecting.value = true
    errorMessage.value = null
    try {
      const client = createClient()
      await client.send(new HeadBucketCommand({ Bucket: config.value.bucket }))
      isConnected.value = true
      saveConfig()
      return true
    } catch (e: any) {
      isConnected.value = false
      errorMessage.value = e?.message || '连接失败'
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
      const playlists = localStorage.getItem('playlists') || '[]'
      const settings = localStorage.getItem('appSettings') || '{}'

      const data: BackupData = {
        version: 1,
        timestamp: new Date().toISOString(),
        playlists: JSON.parse(playlists),
        settings: JSON.parse(settings)
      }

      const key = `${BACKUP_KEY_PREFIX}${new Date().toISOString().replace(/[:.]/g, '-')}.json`
      const client = createClient()
      await client.send(new PutObjectCommand({
        Bucket: config.value.bucket,
        Key: key,
        Body: JSON.stringify(data),
        ContentType: 'application/json'
      }))

      lastBackupTime.value = data.timestamp
      localStorage.setItem('lastBackupTime', data.timestamp)
      return true
    } catch (e: any) {
      errorMessage.value = e?.message || '备份失败'
      return false
    } finally {
      isBackingUp.value = false
    }
  }

  async function getLatestBackupKey(): Promise<string | null> {
    try {
      const client = createClient()
      const result = await client.send(new ListObjectsV2Command({
        Bucket: config.value.bucket,
        Prefix: BACKUP_KEY_PREFIX,
        MaxKeys: 1
      }))

      const contents = result.Contents
      if (!contents || contents.length === 0) return null

      contents.sort((a, b) => {
        const ta = a.LastModified?.getTime() ?? 0
        const tb = b.LastModified?.getTime() ?? 0
        return tb - ta
      })

      return contents[0].Key ?? null
    } catch {
      return null
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
      const key = await getLatestBackupKey()
      if (!key) {
        errorMessage.value = '未找到备份数据'
        return false
      }

      const client = createClient()
      const result = await client.send(new GetObjectCommand({
        Bucket: config.value.bucket,
        Key: key
      }))

      const body = await result.Body?.transformToString()
      if (!body) {
        errorMessage.value = '备份数据为空'
        return false
      }

      const data: BackupData = JSON.parse(body)

      if (mode === 'overwrite') {
        localStorage.setItem('playlists', JSON.stringify(data.playlists))
        localStorage.setItem('appSettings', JSON.stringify(data.settings))
      } else {
        // merge playlists
        const existing = JSON.parse(localStorage.getItem('playlists') || '[]')
        const merged = [...existing]
        for (const pl of data.playlists) {
          if (!merged.some((e: any) => e.id === pl.id)) {
            merged.push(pl)
          }
        }
        localStorage.setItem('playlists', JSON.stringify(merged))

        // merge settings (prefer local)
        const localSettings = JSON.parse(localStorage.getItem('appSettings') || '{}')
        const mergedSettings = { ...data.settings, ...localSettings }
        localStorage.setItem('appSettings', JSON.stringify(mergedSettings))
      }

      return true
    } catch (e: any) {
      errorMessage.value = e?.message || '恢复失败'
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
