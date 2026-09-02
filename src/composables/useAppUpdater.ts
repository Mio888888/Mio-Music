import { computed, ref } from 'vue'
import { check, type Update } from '@tauri-apps/plugin-updater'
import { getVersion } from '@tauri-apps/api/app'
import { relaunch } from '@tauri-apps/plugin-process'
import { platform } from '@tauri-apps/plugin-os'
import { isDesktopUpdatePlatform } from '@/utils/updatePlatform'

export type AppUpdateStatus =
  | 'idle'
  | 'checking'
  | 'available'
  | 'downloading'
  | 'downloaded'
  | 'up-to-date'
  | 'error'

let update: Update | null = null
const status = ref<AppUpdateStatus>('idle')
const currentVersion = ref('')
const newVersion = ref('')
const releaseNotes = ref('')
const progress = ref(0)
const downloadedBytes = ref(0)
const totalBytes = ref(0)
const error = ref('')

const isUpdateSupported = computed(() => {
  try {
    return isDesktopUpdatePlatform(platform())
  } catch {
    return false
  }
})

const downloadedMb = computed(() => (downloadedBytes.value / 1024 / 1024).toFixed(1))
const totalMb = computed(() => (totalBytes.value / 1024 / 1024).toFixed(1))

function reset() {
  status.value = 'idle'
  update = null
  newVersion.value = ''
  releaseNotes.value = ''
  progress.value = 0
  downloadedBytes.value = 0
  totalBytes.value = 0
  error.value = ''
}

async function loadCurrentVersion() {
  if (!currentVersion.value) {
    try {
      currentVersion.value = await getVersion()
    } catch (cause) {
      console.warn('读取应用版本失败:', cause)
    }
  }
}

async function checkForUpdate() {
  if (!isUpdateSupported.value) {
    reset()
    return false
  }

  await loadCurrentVersion()
  reset()
  status.value = 'checking'

  try {
    update = await check()
    if (!update) {
      status.value = 'up-to-date'
      return false
    }

    newVersion.value = update.version
    releaseNotes.value = update.body || ''
    status.value = 'available'
    return true
  } catch (cause) {
    console.error('检查应用更新失败:', cause)
    error.value = cause instanceof Error ? cause.message : String(cause)
    status.value = 'error'
    return false
  }
}

async function downloadAndInstall() {
  if (!isUpdateSupported.value) {
    reset()
    return
  }

  await loadCurrentVersion()
  status.value = 'downloading'
  error.value = ''

  try {
    update ??= await check()
    if (!update) {
      status.value = 'up-to-date'
      return
    }

    let received = 0
    await update.downloadAndInstall((event) => {
      if (event.event === 'Started') {
        totalBytes.value = event.data.contentLength ?? 0
      } else if (event.event === 'Progress') {
        received += event.data.chunkLength
        downloadedBytes.value = received
        if (totalBytes.value > 0) {
          progress.value = Math.min(100, Math.round((received / totalBytes.value) * 100))
        }
      }
    })

    progress.value = 100
    status.value = 'downloaded'
  } catch (cause) {
    console.error('下载应用更新失败:', cause)
    error.value = cause instanceof Error ? cause.message : String(cause)
    status.value = 'error'
  }
}

async function restartToInstall() {
  if (!isUpdateSupported.value) return
  await relaunch()
}

export function useAppUpdater() {
  return {
    status,
    isUpdateSupported,
    currentVersion,
    newVersion,
    releaseNotes,
    progress,
    totalBytes,
    downloadedMb,
    totalMb,
    error,
    loadCurrentVersion,
    checkForUpdate,
    downloadAndInstall,
    restartToInstall,
    dismiss: reset
  }
}
