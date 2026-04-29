import { defineStore } from 'pinia'

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
    pollTimer: null as ReturnType<typeof setInterval> | null
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

      try {
        const res = await (window as any).api?.download?.getTasks?.()
        if (res?.success && Array.isArray(res.data)) {
          this.tasks = res.data
        }
      } catch (error) {
        console.error('Failed to load download tasks:', error)
      }

      // Poll for task updates since we don't have backend events yet
      this.pollTimer = setInterval(async () => {
        try {
          const res = await (window as any).api?.download?.getTasks?.()
          if (res?.success && Array.isArray(res.data)) {
            this.tasks = res.data
          }
        } catch {}
      }, 2000)
    },

    destroy() {
      if (this.pollTimer) {
        clearInterval(this.pollTimer)
        this.pollTimer = null
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
