<template>
  <div class="directory-settings">
    <t-card title="存储目录配置" hover-shadow>
      <template #actions>
        <t-button theme="default" size="small" @click="resetDirectories"> 重置为默认 </t-button>
      </template>

      <div class="directory-section">
        <h4>缓存目录</h4>
        <p class="directory-description">用于存储歌曲缓存文件，提高播放速度</p>

        <div class="directory-item">
          <div class="directory-info">
            <div class="directory-path">
              <t-input
                v-model="directories.cacheDir"
                readonly
                placeholder="缓存目录路径"
                class="path-input"
              />
            </div>
            <div class="directory-size">
              <t-tag theme="primary" variant="light">
                {{ cacheDirSize.formatted }}
              </t-tag>
            </div>
          </div>

          <div class="directory-actions">
            <t-button theme="default" @click="selectCacheDir"> 选择目录 </t-button>
            <t-button theme="default" variant="outline" @click="openCacheDir"> 打开目录 </t-button>
          </div>
        </div>
      </div>

      <t-divider />

      <div class="directory-section">
        <h4>下载目录</h4>
        <p class="directory-description">用于存储下载的音乐文件</p>

        <div class="directory-item">
          <div class="directory-info">
            <div class="directory-path">
              <t-input
                v-model="directories.downloadDir"
                readonly
                placeholder="下载目录路径"
                class="path-input"
              />
            </div>
            <div class="directory-size">
              <t-tag theme="success" variant="light">
                {{ downloadDirSize.formatted }}
              </t-tag>
            </div>
          </div>

          <div class="directory-actions">
            <t-button theme="default" @click="selectDownloadDir"> 选择目录 </t-button>
            <t-button theme="default" variant="outline" @click="openDownloadDir">
              打开目录
            </t-button>
          </div>
        </div>
      </div>

      <div class="save-section">
        <t-button theme="primary" size="large" :loading="isSaving" @click="saveDirectories">
          保存设置
        </t-button>
      </div>
    </t-card>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, toRaw } from 'vue'
import { MessagePlugin, DialogPlugin } from 'tdesign-vue-next'

// 定义事件
const emit = defineEmits<{
  'directory-changed': []
  'cache-cleared': []
}>()

// 响应式数据
const directories = ref({
  cacheDir: '',
  downloadDir: ''
})

const cacheDirSize = ref({ size: 0, formatted: '0 B' })
const downloadDirSize = ref({ size: 0, formatted: '0 B' })
const isSaving = ref(false)

// 格式化文件大小
const formatSize = (bytes: number): string => {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return (bytes / Math.pow(1024, i)).toFixed(2) + ' ' + units[i]
}

// 加载目录配置
const loadDirectories = async () => {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const dirs = await invoke('get_directories') as { cacheDir: string; downloadDir: string }
    if (dirs) {
      directories.value = dirs
    }

    // 获取目录大小
    await Promise.all([updateCacheDirSize(), updateDownloadDirSize()])
  } catch (error) {
    console.error('加载目录配置失败:', error)
    MessagePlugin.error('加载目录配置失败')
  }
}

// 更新缓存目录大小
const updateCacheDirSize = async () => {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const size = await invoke('get_directory_size', { path: directories.value.cacheDir }) as number
    cacheDirSize.value = { size, formatted: formatSize(size) }
  } catch (error) {
    console.error('获取缓存目录大小失败:', error)
  }
}

// 更新下载目录大小
const updateDownloadDirSize = async () => {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const size = await invoke('get_directory_size', { path: directories.value.downloadDir }) as number
    downloadDirSize.value = { size, formatted: formatSize(size) }
  } catch (error) {
    console.error('获取下载目录大小失败:', error)
  }
}

// 选择缓存目录
const selectCacheDir = async () => {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const selected = await open({ directory: true, multiple: false })
    if (selected) {
      const path = typeof selected === 'string' ? selected : (selected as string[])[0]
      directories.value.cacheDir = path
      await updateCacheDirSize()
      MessagePlugin.success('缓存目录已选择,记得保存奥')
    }
  } catch (error) {
    console.error('选择缓存目录失败:', error)
    MessagePlugin.error('选择目录失败')
  }
}

// 选择下载目录
const selectDownloadDir = async () => {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const selected = await open({ directory: true, multiple: false })
    if (selected) {
      const path = typeof selected === 'string' ? selected : (selected as string[])[0]
      directories.value.downloadDir = path
      await updateDownloadDirSize()
      MessagePlugin.success('下载目录已选择,记得保存奥')
    }
  } catch (error) {
    console.error('选择下载目录失败:', error)
    MessagePlugin.error('选择目录失败')
  }
}

// 打开缓存目录
const openCacheDir = async () => {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('open_directory', { path: directories.value.cacheDir })
  } catch (error) {
    console.error('打开缓存目录失败:', error)
    MessagePlugin.error('打开目录失败')
  }
}

// 打开下载目录
const openDownloadDir = async () => {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('open_directory', { path: directories.value.downloadDir })
  } catch (error) {
    console.error('打开下载目录失败:', error)
    MessagePlugin.error('打开目录失败')
  }
}

// 保存目录设置
const saveDirectories = async () => {
  isSaving.value = true

  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('save_directories', { directories: toRaw(directories.value) })
    MessagePlugin.success('目录设置已保存')
    emit('directory-changed')
  } catch (error) {
    console.error('保存目录设置失败:', error)
    MessagePlugin.error('保存设置失败')
  } finally {
    isSaving.value = false
  }
}

// 重置为默认目录
const resetDirectories = async () => {
  const confirm = DialogPlugin.confirm({
    header: '重置目录设置',
    body: '确定要重置为默认目录吗？这将清除当前的自定义目录设置。',
    confirmBtn: '确定重置',
    cancelBtn: '取消',
    onConfirm: async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core')
        const result = await invoke('reset_directories') as { cacheDir: string; downloadDir: string }
        if (result) {
          directories.value = result
          await Promise.all([updateCacheDirSize(), updateDownloadDirSize()])
          MessagePlugin.success('已重置为默认目录')
          emit('directory-changed')
        }
      } catch (error) {
        console.error('重置目录设置失败:', error)
        MessagePlugin.error('重置失败')
      }
      confirm.hide()
    }
  })
}

// 刷新目录大小（供父组件调用）
const refreshDirectorySizes = async () => {
  await Promise.all([updateCacheDirSize(), updateDownloadDirSize()])
}

// 暴露方法给父组件
defineExpose({
  refreshDirectorySizes
})

// 组件挂载时加载配置
onMounted(() => {
  loadDirectories()
})
</script>

<style lang="scss" scoped>
.directory-settings {
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.directory-section {
  margin-bottom: 24px;

  h4 {
    margin: 0 0 8px 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--td-text-color-primary);
  }

  .directory-description {
    margin: 0 0 16px 0;
    font-size: 14px;
    color: var(--td-text-color-secondary);
  }
}

.directory-item {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.directory-info {
  display: flex;
  align-items: center;
  gap: 12px;

  .directory-path {
    flex: 1;

    .path-input {
      width: 100%;
    }
  }

  .directory-size {
    flex-shrink: 0;
  }
}

.directory-actions {
  display: flex;
  gap: 8px;
}

.save-section {
  margin-top: 24px;
  padding-top: 24px;
  border-top: 1px solid var(--td-border-level-1-color);
  text-align: center;
}

.cache-management {
  margin-top: 24px;
}

/* 响应式设计 */
@media (max-width: 768px) {
  .directory-info {
    flex-direction: column;
    align-items: stretch;
  }

  .directory-actions {
    justify-content: stretch;

    button {
      flex: 1;
    }
  }
}
</style>
