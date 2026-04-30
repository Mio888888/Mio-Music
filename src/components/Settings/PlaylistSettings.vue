<script setup lang="ts">
import { computed } from 'vue'
import { storeToRefs } from 'pinia'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { DialogPlugin } from 'tdesign-vue-next'

const userStore = LocalUserDetailStore()
const { userInfo } = storeToRefs(userStore)

const stats = computed(() => {
  const list = userStore.list || []
  const totalSongs = list.length
  const artists = new Set(list.map((s: any) => s.singer).filter(Boolean))
  let totalDuration = 0
  list.forEach((s: any) => {
    if (typeof s.interval === 'number') totalDuration += s.interval
    else if (typeof s.interval === 'string') {
      const parts = s.interval.split(':')
      if (parts.length === 2) totalDuration += Number(parts[0]) * 60 + Number(parts[1])
    }
  })
  return { totalSongs, totalArtists: artists.size, totalDuration: Math.round(totalDuration / 60) }
})

const exportPlaylist = () => {
  try {
    const list = (userStore.list || []).filter((s: any) => s.source !== 'local')
    const data = JSON.stringify(list)
    const blob = new Blob([data], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `ceru-playlist-${Date.now()}.cmpl`
    a.click()
    URL.revokeObjectURL(url)
  } catch (e) {
    console.error('导出失败:', e)
  }
}

const copyToClipboard = async () => {
  try {
    const list = (userStore.list || []).filter((s: any) => s.source !== 'local')
    await navigator.clipboard.writeText(JSON.stringify(list))
  } catch (e) {
    console.error('复制失败:', e)
  }
}

const importFromFile = async () => {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const selected = await open({
      multiple: false,
      filters: [{ name: 'Playlist', extensions: ['cmpl', 'cpl', 'json'] }]
    })
    if (selected) {
      const filePath = typeof selected === 'string' ? selected : (selected as any).path
      const { readFile } = await import('@tauri-apps/plugin-fs')
      const content = await readFile(filePath)
      const text = new TextDecoder().decode(content)
      const imported = JSON.parse(text)
      if (Array.isArray(imported)) {
        const existing = userStore.list || []
        const existingIds = new Set(existing.map((s: any) => s.songmid))
        const newSongs = imported.filter((s: any) => !existingIds.has(s.songmid))
        userStore.replaceSongList([...existing, ...newSongs])
      }
    }
  } catch (e) {
    console.error('导入失败:', e)
  }
}

const clearPlaylist = () => {
  const dialog = DialogPlugin.confirm({
    header: '确认清空播放列表',
    body: '此操作将清空所有播放列表中的歌曲，且不可恢复。确定要继续吗？',
    confirmBtn: { content: '确认清空', theme: 'danger' },
    cancelBtn: { content: '取消' },
    onConfirm: () => {
      userStore.clearList()
      dialog.destroy()
    },
    onClose: () => dialog.destroy()
  })
}
</script>

<template>
  <div class="playlist-settings">
    <div class="stats-grid">
      <div class="stat-item">
        <div class="stat-value">{{ stats.totalSongs }}</div>
        <div class="stat-label">歌曲总数</div>
      </div>
      <div class="stat-item">
        <div class="stat-value">{{ stats.totalArtists }}</div>
        <div class="stat-label">艺术家</div>
      </div>
      <div class="stat-item">
        <div class="stat-value">{{ stats.totalDuration }} 分钟</div>
        <div class="stat-label">总时长</div>
      </div>
    </div>

    <div class="actions">
      <t-button theme="primary" variant="outline" @click="exportPlaylist">导出播放列表</t-button>
      <t-button theme="default" variant="outline" @click="copyToClipboard">复制到剪贴板</t-button>
      <t-button theme="default" variant="outline" @click="importFromFile">从文件导入</t-button>
      <t-button theme="danger" variant="outline" @click="clearPlaylist">清空播放列表</t-button>
    </div>
  </div>
</template>

<style scoped>
.playlist-settings { display: flex; flex-direction: column; gap: 1rem; }
.stats-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 1rem; }
.stat-item {
  padding: 1rem; background: var(--td-bg-color-page); border-radius: 0.5rem;
  border: 1px solid var(--td-border-level-1-color); text-align: center;
}
.stat-item .stat-value { font-size: 1.5rem; font-weight: 700; color: var(--td-brand-color); }
.stat-item .stat-label { font-size: 0.75rem; color: var(--td-text-color-secondary); margin-top: 0.25rem; }
.actions { display: flex; gap: 0.5rem; flex-wrap: wrap; }
</style>
