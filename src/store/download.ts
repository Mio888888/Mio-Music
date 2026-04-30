import { defineStore } from 'pinia'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export enum DownloadStatus {
  Queued = 'queued',
  Downloading = 'downloading',
  Paused = 'paused',
  Completed = 'completed',
  Error = 'error',
  Cancelled = 'cancelled'
}

export interface DownloadTask {
  id: string
  song_info: any
  url: string
  plugin_id?: string
  quality?: string
  file_path: string
  status: DownloadStatus
  progress: number
  speed: number
  total_size: number
  downloaded_size: number
  remaining_time: number | null
  error: string | null
  priority?: number
  created_at: number
}

export const useDownloadStore = defineStore('download', {
  state: () => ({
    tasks: [] as DownloadTask[],
    isInitialized: false,
    unlisteners: [] as UnlistenFn[]
  }),
  getters: {
    activeTasks: (state) => state.tasks.filter((t) =>
      [DownloadStatus.Downloading, DownloadStatus.Queued, DownloadStatus.Paused].includes(t.status)
    ),
    completedTasks: (state) => state.tasks.filter((t) => t.status === DownloadStatus.Completed),
    failedTasks: (state) => state.tasks.filter((t) =>
      [DownloadStatus.Error, DownloadStatus.Cancelled].includes(t.status)
    ),
    downloadingCount: (state) =>
      state.tasks.filter((t) => t.status === DownloadStatus.Downloading).length
  },
  actions: {
    async init() {
      if (this.isInitialized) return
      this.isInitialized = true

      // Load initial tasks
      try {
        const res = await (window as any).api?.download?.getTasks?.()
        if (res?.success && Array.isArray(res.data)) {
          this.tasks = res.data
        }
      } catch (error) {
        console.error('Failed to load download tasks:', error)
      }

      // Register event listeners for real-time updates
      try {
        const unlisteners = await Promise.all([
          listen<DownloadTask>('download:task-added', (e) => {
            const task = e.payload
            if (!this.tasks.find(t => t.id === task.id)) {
              this.tasks.push(task)
            }
          }),
          listen<DownloadTask>('download:task-progress', (e) => {
            this.updateTask(e.payload)
          }),
          listen<DownloadTask>('download:task-status-changed', (e) => {
            this.updateTask(e.payload)
          }),
          listen<DownloadTask>('download:task-completed', (e) => {
            this.updateTask(e.payload)
          }),
          listen<DownloadTask>('download:task-error', (e) => {
            this.updateTask(e.payload)
          }),
          listen<string>('download:task-deleted', (e) => {
            this.tasks = this.tasks.filter(t => t.id !== e.payload)
          }),
          listen<DownloadTask[]>('download:tasks-reset', (e) => {
            this.tasks = e.payload
          }),
        ])
        this.unlisteners = unlisteners
      } catch (e) {
        console.warn('[DownloadStore] Event listeners failed, falling back to polling:', e)
        // Fallback: polling is handled by periodic refresh in components
      }
    },

    destroy() {
      for (const un of this.unlisteners) { un() }
      this.unlisteners = []
    },

    updateTask(task: DownloadTask) {
      const idx = this.tasks.findIndex(t => t.id === task.id)
      if (idx !== -1) {
        this.tasks[idx] = task
      } else {
        this.tasks.push(task)
      }
    },

    async pauseTask(taskId: string) {
      await (window as any).api?.download?.pauseTask?.(taskId)
    },

    async resumeTask(taskId: string) {
      await (window as any).api?.download?.resumeTask?.(taskId)
    },

    async cancelTask(taskId: string) {
      await (window as any).api?.download?.cancelTask?.(taskId)
    },

    async deleteTask(taskId: string, deleteFile = false) {
      await (window as any).api?.download?.deleteTask?.(taskId, deleteFile)
    },

    async retryTask(taskId: string) {
      await (window as any).api?.download?.retryTask?.(taskId)
    },

    async pauseAllTasks() {
      await (window as any).api?.download?.pauseAllTasks?.()
    },

    async resumeAllTasks() {
      await (window as any).api?.download?.resumeAllTasks?.()
    },

    async clearTasks(type: 'queue' | 'completed' | 'failed' | 'all') {
      await (window as any).api?.download?.clearTasks?.(type)
    },

    async validateFiles() {
      try {
        const res = await (window as any).api?.download?.validateFiles?.()
        if (res?.success && Array.isArray(res.data)) {
          this.tasks = res.data
        }
      } catch {}
    },

    async getMaxConcurrent(): Promise<number> {
      try {
        const res = await (window as any).api?.download?.getMaxConcurrent?.()
        return res?.data ?? 3
      } catch {
        return 3
      }
    },

    async setMaxConcurrent(max: number) {
      await (window as any).api?.download?.setMaxConcurrent?.(max)
    },

    async openFileLocation(filePath: string) {
      await (window as any).api?.download?.openFileLocation?.(filePath)
    }
  },
  persist: false
})
