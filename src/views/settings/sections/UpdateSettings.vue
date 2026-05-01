<template>
  <div class="update-settings">
    <div class="update-section">
      <h3>自动更新</h3>
      <div class="setting-item" style="margin-top: 0">
        <div class="item-info">
          <div class="item-title">启用自动检查更新</div>
          <div class="item-desc">启动时自动检查是否有新版本</div>
        </div>
        <t-switch v-model="autoCheck" @change="saveAutoCheck" />
      </div>
    </div>

    <div class="update-section" style="margin-top: 24px;">
      <h3>版本信息</h3>
      <div class="update-info">
        <p>当前版本: {{ currentVersion }}</p>
        <t-button theme="primary" :loading="isChecking" @click="checkUpdate">
          {{ isChecking ? '检查中...' : '检查更新' }}
        </t-button>
      </div>

      <div v-if="updateStatus === 'available'" class="update-card">
        <p>发现新版本: {{ updateInfo?.version || '未知' }}</p>
        <t-button theme="primary" :loading="isDownloading" @click="downloadUpdate">
          {{ isDownloading ? '下载中...' : '下载更新' }}
        </t-button>
      </div>

      <div v-if="updateStatus === 'downloading'" class="update-card">
        <p>正在下载更新...</p>
        <t-progress :percentage="downloadProgress" theme="plump" />
        <p class="progress-detail">{{ downloadDetail }}</p>
      </div>

      <div v-if="updateStatus === 'downloaded'" class="update-card">
        <p>更新已下载完成，重启应用即可安装。</p>
        <t-button theme="primary" @click="quitAndInstall">立即重启安装</t-button>
      </div>

      <div v-if="updateStatus === 'not-available'" class="update-card success">
        <p>当前已是最新版本。</p>
      </div>

      <div v-if="updateStatus === 'error'" class="update-card error">
        <p>{{ updateError || '检查更新失败' }}</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue'
import { MessagePlugin } from 'tdesign-vue-next'

const api = (window as any).api

const currentVersion = ref('0.1.0')
const autoCheck = ref(true)
const isChecking = ref(false)
const isDownloading = ref(false)
const updateStatus = ref<'idle' | 'checking' | 'available' | 'downloading' | 'downloaded' | 'not-available' | 'error'>('idle')
const updateInfo = ref<any>(null)
const downloadProgress = ref(0)
const downloadDetail = ref('')
const updateError = ref('')

let cleanups: Array<() => void> = []

const saveAutoCheck = () => {
  localStorage.setItem('autoUpdate', String(autoCheck.value))
}

const checkUpdate = async () => {
  isChecking.value = true
  updateStatus.value = 'checking'
  try {
    await api.autoUpdater.checkForUpdates()
  } catch (e: any) {
    updateStatus.value = 'error'
    // "Load failed" = Rust 端未注册 auto-updater 命令（tauri-plugin-updater 未安装）
    if (e.message?.includes('Load failed') || e.message?.includes('not found')) {
      updateError.value = '自动更新功能暂未实现，请关注项目发布页获取新版本'
    } else {
      updateError.value = e.message || '检查失败'
    }
  } finally {
    setTimeout(() => { isChecking.value = false }, 1500)
  }
}

const downloadUpdate = async () => {
  isDownloading.value = true
  updateStatus.value = 'downloading'
  try {
    await api.autoUpdater.downloadUpdate()
  } catch (e: any) {
    updateStatus.value = 'error'
    updateError.value = e.message || '下载失败'
  } finally {
    isDownloading.value = false
  }
}

const quitAndInstall = async () => {
  try {
    await api.autoUpdater.quitAndInstall()
  } catch (e: any) {
    MessagePlugin.error(e.message || '安装失败')
  }
}

onMounted(() => {
  autoCheck.value = localStorage.getItem('autoUpdate') !== 'false'

  cleanups.push(api.autoUpdater.onCheckingForUpdate(() => {
    updateStatus.value = 'checking'
  }))

  cleanups.push(api.autoUpdater.onUpdateAvailable(() => {
    updateStatus.value = 'available'
  }))

  cleanups.push(api.autoUpdater.onUpdateNotAvailable(() => {
    updateStatus.value = 'not-available'
  }))

  cleanups.push(api.autoUpdater.onDownloadProgress((progress: any) => {
    updateStatus.value = 'downloading'
    if (progress) {
      downloadProgress.value = progress.percent || 0
      const downloaded = (progress.transferred || 0) / 1024 / 1024
      const total = (progress.total || 0) / 1024 / 1024
      downloadDetail.value = `${downloaded.toFixed(1)} MB / ${total.toFixed(1)} MB`
    }
  }))

  cleanups.push(api.autoUpdater.onUpdateDownloaded(() => {
    updateStatus.value = 'downloaded'
    isDownloading.value = false
  }))

  cleanups.push(api.autoUpdater.onError((error: string) => {
    updateStatus.value = 'error'
    updateError.value = error
    isChecking.value = false
    isDownloading.value = false
  }))
})

onBeforeUnmount(() => {
  cleanups.forEach(fn => fn())
  cleanups = []
})
</script>

<style scoped>
.update-settings { padding: 0; }
.update-section { margin-bottom: 24px; }
.update-section h3 {
  margin-bottom: 12px; font-size: 16px; font-weight: 600;
  color: var(--td-text-color-primary);
}
.setting-item {
  display: flex; align-items: center; justify-content: space-between;
  padding: 0.875rem 1rem; border: 1px solid var(--td-border-level-1-color);
  background: var(--td-bg-color-container); border-radius: 0.5rem;
}
.setting-item .item-info { display: flex; flex-direction: column; gap: 0.25rem; }
.setting-item .item-title { font-weight: 600; font-size: 0.95rem; }
.setting-item .item-desc { color: var(--td-text-color-secondary); font-size: 0.8rem; }

.update-info { display: flex; align-items: center; gap: 16px; margin-top: 12px; }
.update-info p { margin: 0; color: var(--td-text-color-secondary); }

.update-card {
  margin-top: 12px; padding: 16px;
  background: var(--td-bg-color-container); border: 1px solid var(--td-border-level-1-color);
  border-radius: 10px;
}
.update-card p { margin: 0 0 12px 0; }
.update-card p:last-child { margin-bottom: 0; }
.update-card.success { border-color: var(--td-success-color); }
.update-card.error { border-color: var(--td-error-color); }
.progress-detail { font-size: 13px; color: var(--td-text-color-secondary); margin-top: 8px; }
</style>
