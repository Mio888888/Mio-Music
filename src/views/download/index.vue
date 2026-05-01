<script setup lang="ts">
import { onMounted, computed, ref, watch, onBeforeUnmount } from 'vue'
import { useDownloadStore, DownloadStatus } from '@/store/download'
import { formatMusicInfo } from '@/utils/format'
import { storeToRefs } from 'pinia'

const store = useDownloadStore()
const { tasks } = storeToRefs(store)
const maxConcurrent = ref(3)
const activeTab = ref('downloading')
const searchKeyword = ref('')

onMounted(async () => {
  store.init()
  maxConcurrent.value = await store.getMaxConcurrent()
})

onBeforeUnmount(() => {
  store.destroy()
})

watch(activeTab, (val) => {
  if (val === 'completed') {
    store.validateFiles()
  }
})

const downloadingTasks = computed(() =>
  tasks.value.filter((t) =>
    [DownloadStatus.Downloading, DownloadStatus.Queued, DownloadStatus.Paused].includes(t.status)
  )
)
const completedTasks = computed(() =>
  tasks.value.filter((t) => t.status === DownloadStatus.Completed)
)
const failedTasks = computed(() =>
  tasks.value.filter((t) => [DownloadStatus.Error, DownloadStatus.Cancelled].includes(t.status))
)

const filteredTasks = computed(() => {
  let list = activeTab.value === 'downloading' ? downloadingTasks.value
    : activeTab.value === 'completed' ? completedTasks.value
    : failedTasks.value

  if (searchKeyword.value.trim()) {
    const kw = searchKeyword.value.toLowerCase()
    list = list.filter(t =>
      t.song_info?.name?.toLowerCase().includes(kw) ||
      t.song_info?.singer?.toLowerCase().includes(kw)
    )
  }

  return [...list].sort((a, b) => {
    if (activeTab.value === 'completed' || activeTab.value === 'failed') {
      return b.created_at - a.created_at
    }
    const statusOrder: Record<string, number> = {
      [DownloadStatus.Downloading]: 0,
      [DownloadStatus.Queued]: 1,
      [DownloadStatus.Paused]: 2
    }
    const sa = statusOrder[a.status] ?? 99
    const sb = statusOrder[b.status] ?? 99
    if (sa !== sb) return sa - sb
    return a.created_at - b.created_at
  })
})

const formatSpeed = (speed: number) => {
  if (speed === 0) return '0 B/s'
  const units = ['B/s', 'KB/s', 'MB/s', 'GB/s']
  let s = speed, i = 0
  while (s >= 1024 && i < units.length - 1) { s /= 1024; i++ }
  return `${s.toFixed(1)} ${units[i]}`
}

const formatSize = (size: number) => {
  if (!size) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  let s = size, i = 0
  while (s >= 1024 && i < units.length - 1) { s /= 1024; i++ }
  return `${s.toFixed(1)} ${units[i]}`
}

const formatRemaining = (seconds: number | null) => {
  if (!seconds || !isFinite(seconds)) return ''
  if (seconds < 60) return `${Math.round(seconds)}秒`
  const m = Math.floor(seconds / 60)
  const s = Math.round(seconds % 60)
  return `${m}分${s}秒`
}

const getStatusText = (status: DownloadStatus) => {
  const map: Record<string, string> = {
    [DownloadStatus.Queued]: '等待中',
    [DownloadStatus.Downloading]: '下载中',
    [DownloadStatus.Paused]: '已暂停',
    [DownloadStatus.Completed]: '完成',
    [DownloadStatus.Error]: '错误',
    [DownloadStatus.Cancelled]: '已取消'
  }
  return map[status] || status
}

const getStatusTheme = (status: DownloadStatus): 'default' | 'primary' | 'danger' | 'warning' | 'success' => {
  const map: Record<string, 'default' | 'primary' | 'danger' | 'warning' | 'success'> = {
    [DownloadStatus.Downloading]: 'primary',
    [DownloadStatus.Completed]: 'success',
    [DownloadStatus.Error]: 'danger',
    [DownloadStatus.Paused]: 'warning'
  }
  return map[status] || 'default'
}
</script>

<template>
  <div class="download-manager">
    <div class="header">
      <h2>下载管理</h2>
      <div class="settings">
        <t-input
          v-model="searchKeyword"
          placeholder="搜索任务"
          clearable
          size="small"
          style="width: 180px"
        >
          <template #prefix-icon><i class="iconfont icon-sousuo"></i></template>
        </t-input>
        <div class="divider"></div>
        <div v-if="activeTab === 'downloading'" class="batch-actions">
          <t-button theme="primary" variant="text" size="small" @click="store.resumeAllTasks()">全部开始</t-button>
          <t-button theme="warning" variant="text" size="small" @click="store.pauseAllTasks()">全部暂停</t-button>
          <div class="divider"></div>
        </div>
        <t-button
          v-if="filteredTasks.length > 0"
          theme="default" variant="outline" size="small"
          @click="activeTab === 'downloading' ? store.clearTasks('queue') : activeTab === 'completed' ? store.clearTasks('completed') : store.clearTasks('failed')"
        >
          {{ activeTab === 'downloading' ? '清空队列' : '清空记录' }}
        </t-button>
        <div class="divider"></div>
        <span>同时下载数：</span>
        <t-input-number
          v-model="maxConcurrent" :min="1" :max="5" style="width: 100px"
          @change="(val: any) => store.setMaxConcurrent(Number(val))"
        />
      </div>
    </div>

    <t-tabs v-model="activeTab" class="tabs">
      <t-tab-panel value="downloading" :label="downloadingTasks.length ? `进行中(${downloadingTasks.length})` : '进行中'" />
      <t-tab-panel value="completed" :label="completedTasks.length ? `已完成(${completedTasks.length})` : '已完成'" />
      <t-tab-panel value="failed" :label="failedTasks.length ? `失败(${failedTasks.length})` : '失败'" />
    </t-tabs>

    <div class="task-list">
      <div v-if="filteredTasks.length === 0" class="empty-state">
        <p>{{ searchKeyword ? '没有匹配的任务' : '暂无任务' }}</p>
      </div>
      <div v-else class="tasks">
        <div v-for="task in filteredTasks" :key="task.id" class="task-item">
          <div class="task-info">
            <div class="task-name">{{ task.song_info?.name || '未知歌曲' }}</div>
            <div class="task-meta">
              <t-tag :theme="getStatusTheme(task.status)" variant="light" size="small">
                {{ getStatusText(task.status) }}
              </t-tag>
              <span v-if="task.quality" class="quality-tag">{{ task.quality.toUpperCase() }}</span>
              <span v-if="task.status === DownloadStatus.Downloading" class="speed">{{ formatSpeed(task.speed) }}</span>
              <span v-if="task.status === DownloadStatus.Downloading && task.remaining_time" class="remaining">
                剩余 {{ formatRemaining(task.remaining_time) }}
              </span>
              <span class="size">{{ formatSize(task.downloaded_size) }} / {{ formatSize(task.total_size) }}</span>
            </div>
          </div>

          <div class="task-progress">
            <t-progress
              :percentage="Math.round(task.progress)"
              :status="task.status === DownloadStatus.Error ? 'error' : task.status === DownloadStatus.Completed ? 'success' : 'active'"
            />
            <div v-if="task.error" class="error-msg">{{ task.error }}</div>
          </div>

          <div class="task-actions">
            <t-button v-if="task.status === DownloadStatus.Downloading || task.status === DownloadStatus.Queued"
              shape="circle" variant="text" size="small" @click="store.pauseTask(task.id)" title="暂停">
              <template #icon><i class="iconfont icon-zanting"></i></template>
            </t-button>
            <t-button v-if="task.status === DownloadStatus.Paused"
              shape="circle" variant="text" size="small" @click="store.resumeTask(task.id)" title="继续">
              <template #icon><i class="iconfont icon-bofang"></i></template>
            </t-button>
            <t-button v-if="task.status === DownloadStatus.Error"
              shape="circle" variant="text" size="small" @click="store.retryTask(task.id)" title="重试">
              <template #icon><i class="iconfont icon-shuaxin"></i></template>
            </t-button>
            <t-button v-if="task.status !== DownloadStatus.Completed && task.status !== DownloadStatus.Cancelled"
              shape="circle" variant="text" size="small" theme="danger" @click="store.cancelTask(task.id)" title="取消">
              <template #icon><i class="iconfont icon-guanbi"></i></template>
            </t-button>
            <t-button v-if="task.status === DownloadStatus.Completed"
              shape="circle" variant="text" size="small" @click="store.openFileLocation(task.file_path)" title="打开位置">
              <template #icon><i class="iconfont icon-wenjianjia"></i></template>
            </t-button>
            <t-button v-if="task.status === DownloadStatus.Completed || task.status === DownloadStatus.Cancelled || task.status === DownloadStatus.Error"
              shape="circle" variant="text" size="small" theme="danger" @click="store.deleteTask(task.id)" title="删除">
              <template #icon><i class="iconfont icon-shanchu"></i></template>
            </t-button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.download-manager { padding: 20px; height: 100%; display: flex; flex-direction: column; color: var(--td-text-color-primary); }
.header { margin-bottom: 20px; display: flex; justify-content: space-between; align-items: center; }
.header h2 { border-left: 8px solid var(--td-brand-color-3); padding-left: 12px; border-radius: 8px; line-height: 1.5em; font-size: 1.5rem; font-weight: 600; margin: 0; }
.settings { display: flex; align-items: center; gap: 8px; font-size: 14px; }
.batch-actions { display: flex; align-items: center; }
.divider { width: 1px; height: 16px; background: var(--td-border-level-1-color); margin: 0 4px; }
.tabs { margin-bottom: 16px; flex-shrink: 0; }
.task-list { flex: 1; overflow-y: auto; }
.empty-state { display: flex; align-items: center; justify-content: center; height: 300px; color: var(--td-text-color-secondary); }
.task-item { background: var(--td-bg-color-container); border-radius: 8px; padding: 16px; margin-bottom: 12px; display: flex; align-items: center; gap: 16px; box-shadow: 0 2px 8px rgba(0,0,0,0.05); }
.task-info { width: 350px; flex-shrink: 0; }
.task-name { font-weight: 500; margin-bottom: 8px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.task-meta { display: flex; gap: 8px; font-size: 12px; color: var(--td-text-color-secondary); align-items: center; }
.quality-tag { background: var(--td-bg-color-secondarycontainer); padding: 1px 6px; border-radius: 4px; font-size: 11px; }
.speed { color: var(--td-brand-color); font-weight: 500; }
.remaining { color: var(--td-text-color-placeholder); }
.task-progress { flex: 1; display: flex; flex-direction: column; justify-content: center; }
.error-msg { color: var(--td-error-color); font-size: 12px; margin-top: 4px; }
.task-actions { display: flex; gap: 4px; }
</style>
