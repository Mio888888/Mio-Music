<script setup lang="ts">
import { onMounted, computed, ref, watch, onBeforeUnmount } from 'vue'
import { useDownloadStore, DownloadStatus } from '@/store/download'
import { formatMusicInfo } from '@/utils/format'
import { storeToRefs } from 'pinia'

const { t } = useI18n()
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
  if (seconds < 60) return t('common.unitSecond', { seconds: Math.round(seconds) })
  const m = Math.floor(seconds / 60)
  const s = Math.round(seconds % 60)
  return t('common.unitMinuteSecond', { m, s })
}

const getStatusText = (status: DownloadStatus) => {
  const map: Record<string, string> = {
    [DownloadStatus.Queued]: t('common.waiting'),
    [DownloadStatus.Downloading]: t('common.downloading'),
    [DownloadStatus.Paused]: t('common.paused'),
    [DownloadStatus.Completed]: t('common.completed'),
    [DownloadStatus.Error]: t('common.error'),
    [DownloadStatus.Cancelled]: t('common.cancelled')
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
      <h2>{{ t('download.title') }}</h2>
      <div class="settings">
        <t-input
          v-model="searchKeyword"
          :placeholder="t('download.searchTask')"
          clearable
          size="small"
          style="width: 180px"
        >
          <template #prefix-icon><i class="iconfont icon-sousuo"></i></template>
        </t-input>
        <div class="divider"></div>
        <div v-if="activeTab === 'downloading'" class="batch-actions">
          <t-button theme="primary" variant="text" size="small" @click="store.resumeAllTasks()">{{ t('download.startAll') }}</t-button>
          <t-button theme="warning" variant="text" size="small" @click="store.pauseAllTasks()">{{ t('download.pauseAll') }}</t-button>
          <div class="divider"></div>
        </div>
        <t-button
          v-if="filteredTasks.length > 0"
          theme="default" variant="outline" size="small"
          @click="activeTab === 'downloading' ? store.clearTasks('queue') : activeTab === 'completed' ? store.clearTasks('completed') : store.clearTasks('failed')"
        >
          {{ activeTab === 'downloading' ? t('download.clearQueue') : t('download.clearRecords') }}
        </t-button>
        <div class="divider"></div>
        <span>{{ t('download.concurrentDownloads') }}</span>
        <t-input-number
          v-model="maxConcurrent" :min="1" :max="5" style="width: 100px"
          @change="(val: any) => store.setMaxConcurrent(Number(val))"
        />
      </div>
    </div>

    <t-tabs v-model="activeTab" class="tabs">
      <t-tab-panel value="downloading" :label="downloadingTasks.length ? `${t('download.tabActive')}(${downloadingTasks.length})` : t('download.tabActive')" />
      <t-tab-panel value="completed" :label="completedTasks.length ? `${t('download.tabCompleted')}(${completedTasks.length})` : t('download.tabCompleted')" />
      <t-tab-panel value="failed" :label="failedTasks.length ? `${t('download.tabFailed')}(${failedTasks.length})` : t('download.tabFailed')" />
    </t-tabs>

    <div class="task-list">
      <div v-if="filteredTasks.length === 0" class="empty-state">
        <p>{{ searchKeyword ? t('download.noMatch') : t('download.empty') }}</p>
      </div>
      <div v-else class="tasks">
        <div v-for="task in filteredTasks" :key="task.id" class="task-item">
          <div class="task-info">
            <div class="task-name">{{ task.song_info?.name || t('download.unknownSong') }}</div>
            <div class="task-meta">
              <t-tag :theme="getStatusTheme(task.status)" variant="light" size="small">
                {{ getStatusText(task.status) }}
              </t-tag>
              <span v-if="task.quality" class="quality-tag">{{ task.quality.toUpperCase() }}</span>
              <span v-if="task.status === DownloadStatus.Downloading" class="speed">{{ formatSpeed(task.speed) }}</span>
              <span v-if="task.status === DownloadStatus.Downloading && task.remaining_time" class="remaining">
                {{ t('download.remaining', { time: formatRemaining(task.remaining_time) }) }}
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
              shape="circle" variant="text" size="small" @click="store.pauseTask(task.id)" :title="t('common.pause')">
              <template #icon><i class="iconfont icon-zanting"></i></template>
            </t-button>
            <t-button v-if="task.status === DownloadStatus.Paused"
              shape="circle" variant="text" size="small" @click="store.resumeTask(task.id)" :title="t('common.continue_')">
              <template #icon><i class="iconfont icon-bofang"></i></template>
            </t-button>
            <t-button v-if="task.status === DownloadStatus.Error"
              shape="circle" variant="text" size="small" @click="store.retryTask(task.id)" :title="t('common.retry')">
              <template #icon><i class="iconfont icon-shuaxin"></i></template>
            </t-button>
            <t-button v-if="task.status !== DownloadStatus.Completed && task.status !== DownloadStatus.Cancelled"
              shape="circle" variant="text" size="small" theme="danger" @click="store.cancelTask(task.id)" :title="t('common.cancel')">
              <template #icon><i class="iconfont icon-guanbi"></i></template>
            </t-button>
            <t-button v-if="task.status === DownloadStatus.Completed"
              shape="circle" variant="text" size="small" @click="store.openFileLocation(task.file_path)" :title="t('common.openLocation')">
              <template #icon><i class="iconfont icon-wenjianjia"></i></template>
            </t-button>
            <t-button v-if="task.status === DownloadStatus.Completed || task.status === DownloadStatus.Cancelled || task.status === DownloadStatus.Error"
              shape="circle" variant="text" size="small" theme="danger" @click="store.deleteTask(task.id)" :title="t('common.delete')">
              <template #icon><i class="iconfont icon-shanchu"></i></template>
            </t-button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* ==================== Base Layout ==================== */
.download-manager {
  padding: 20px;
  height: 100%;
  display: flex;
  flex-direction: column;
  color: var(--td-text-color-primary);
}

.header {
  margin-bottom: 20px;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.header h2 {
  border-left: 8px solid var(--td-brand-color-3);
  padding-left: 12px;
  border-radius: 8px;
  line-height: 1.5em;
  font-size: 1.5rem;
  font-weight: 600;
  margin: 0;
}

.settings {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
}

.batch-actions {
  display: flex;
  align-items: center;
}

.divider {
  width: 1px;
  height: 16px;
  background: var(--td-border-level-1-color);
  margin: 0 4px;
}

/* ==================== Tabs ==================== */
.tabs {
  margin-bottom: 16px;
  flex-shrink: 0;
}

/* ==================== Task List ==================== */
.task-list {
  flex: 1;
  overflow-y: auto;
}

.empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 300px;
  color: var(--td-text-color-secondary);
  font-size: 15px;
  background: linear-gradient(
    165deg,
    color-mix(in srgb, var(--td-bg-color-container) 60%, transparent) 0%,
    color-mix(in srgb, var(--td-bg-color-container) 45%, transparent) 100%
  );
  border-radius: var(--mobile-card-radius-small, 16px);
  border: 1px solid color-mix(in srgb, var(--td-text-color-primary) 8%, transparent);
  backdrop-filter: blur(var(--glass-blur-panel)) saturate(180%);
  -webkit-backdrop-filter: blur(var(--glass-blur-panel)) saturate(180%);
}

/* ==================== Task Item — Liquid Glass Card ==================== */
.task-item {
  background: linear-gradient(
    165deg,
    color-mix(in srgb, var(--td-bg-color-container) 78%, transparent) 0%,
    color-mix(in srgb, var(--td-bg-color-container) 62%, transparent) 35%,
    color-mix(in srgb, var(--td-bg-color-container) 70%, transparent) 100%
  );
  backdrop-filter: blur(var(--glass-blur-panel)) saturate(200%);
  -webkit-backdrop-filter: blur(var(--glass-blur-panel)) saturate(200%);
  border: 1px solid color-mix(in srgb, var(--td-text-color-primary) 10%, transparent);
  border-radius: 12px;
  padding: 16px;
  margin-bottom: 12px;
  display: flex;
  align-items: center;
  gap: 16px;
  box-shadow: var(--glass-shadow-control);
  transition:
    background var(--motion-duration-standard) var(--motion-ease-standard),
    box-shadow var(--motion-duration-standard) var(--motion-ease-standard),
    border-color var(--motion-duration-standard) var(--motion-ease-standard);
}

.task-info {
  min-width: 280px;
  flex: 0 0 auto;
}

.task-name {
  font-weight: 500;
  margin-bottom: 8px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.task-meta {
  display: flex;
  gap: 8px;
  font-size: 12px;
  color: var(--td-text-color-secondary);
  align-items: center;
}

.quality-tag {
  background: color-mix(in srgb, var(--td-bg-color-secondarycontainer) 85%, transparent);
  border: 1px solid color-mix(in srgb, var(--td-text-color-primary) 8%, transparent);
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 11px;
}

.speed {
  color: var(--td-brand-color);
  font-weight: 500;
}

.remaining {
  color: var(--td-text-color-placeholder);
}

/* ==================== Progress & Actions ==================== */
.task-progress {
  flex: 1;
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.error-msg {
  color: var(--td-error-color);
  font-size: 12px;
  margin-top: 4px;
}

.task-actions {
  display: flex;
  gap: 4px;
}

/* ==================== Mobile Layout (<=768px) ==================== */
@media (max-width: 768px) {
  .download-manager {
    min-width: 0;
    padding: var(--mobile-page-top-gutter) var(--mobile-page-gutter) 0;
    box-sizing: border-box;
    overflow: hidden;
  }

  .header {
    align-items: flex-start;
    flex-direction: column;
    gap: 14px;
    margin-bottom: 12px;
  }

  .header h2 {
    border-left: none;
    padding-left: 0;
    font-size: clamp(2rem, 9vw, 2.6rem);
    line-height: 1.1;
    letter-spacing: -0.04em;
  }

  /* Mobile settings: search full width row 1, actions row 2 */
  .settings {
    width: 100%;
    align-items: stretch;
    flex-wrap: wrap;
    gap: 8px;
  }

  .settings :deep(.t-input) {
    width: 100% !important;
    min-height: var(--mobile-touch-target);
    flex-basis: 100%;
    order: -1;
  }

  .settings :deep(.t-input-number) {
    width: 96px !important;
    min-height: var(--mobile-touch-target);
  }

  .settings :deep(.t-button) {
    min-height: var(--mobile-touch-target);
    border-radius: var(--mobile-control-radius);
    touch-action: manipulation;
  }

  .batch-actions {
    gap: 4px;
  }

  .divider {
    display: none;
  }

  .tabs {
    margin-bottom: 12px;
  }

  .tabs :deep(.t-tabs__nav-item) {
    min-height: var(--mobile-touch-target);
  }

  .task-list {
    min-height: 0;
    padding-bottom: calc(var(--mobile-safe-bottom, 0px) + 12px);
    -webkit-overflow-scrolling: touch;
  }

  /* Mobile task card — lighter glass treatment */
  .task-item {
    align-items: stretch;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    border-radius: var(--mobile-card-radius-small);
    background: color-mix(in srgb, var(--td-bg-color-container) 92%, transparent);
    border-color: color-mix(in srgb, var(--td-text-color-primary) 8%, transparent);
    box-shadow: var(--mobile-surface-shadow);
  }

  .task-info {
    width: 100%;
    min-width: 0;
  }

  .task-name {
    font-size: 15px;
    margin-bottom: 6px;
    white-space: normal;
    overflow: visible;
    text-overflow: unset;
    line-height: 1.4;
  }

  .task-meta {
    flex-wrap: wrap;
    row-gap: 6px;
    line-height: 1.4;
  }

  /* More prominent progress bar on mobile */
  .task-progress :deep(.t-progress__bar) {
    height: 8px;
  }

  .task-actions {
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: 6px;
    padding-top: 4px;
    border-top: 1px solid color-mix(in srgb, var(--td-text-color-primary) 6%, transparent);
  }

  .task-actions :deep(.t-button) {
    width: var(--mobile-touch-target);
    height: var(--mobile-touch-target);
    min-width: var(--mobile-touch-target);
    min-height: var(--mobile-touch-target);
    touch-action: manipulation;
  }

  .empty-state {
    height: 220px;
    border-radius: var(--mobile-card-radius-small);
  }
}
</style>
