/**
 * IPC Adapter Layer — Maps CeruMusic's Electron IPC to Tauri's invoke/event system.
 *
 * This file creates window.api and window.electron.ipcRenderer compatible objects
 * so that CeruMusic's renderer code can work with minimal modifications.
 */
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'

// ============================================================
// IPC Core: wraps Tauri invoke / event into Electron-like API
// ============================================================

const listeners = new Map<string, Set<UnlistenFn>>()

/** Electron ipcRenderer.invoke equivalent */
async function ipcInvoke(channel: string, ...args: any[]): Promise<any> {
  // Flatten single-object args for Tauri command style
  const params: Record<string, any> = {}
  if (args.length === 1 && typeof args[0] === 'object' && args[0] !== null) {
    params.args = args[0]
  } else if (args.length > 0) {
    params.args = args
  }
  try {
    return await invoke(channel.replace(/:/g, '__'), params)
  } catch (e: any) {
    console.warn(`[IPC] invoke "${channel}" failed:`, e)
    throw e
  }
}

/** Electron ipcRenderer.send equivalent */
function ipcSend(channel: string, ...args: any[]): void {
  invoke(channel.replace(/:/g, '__'), { args }).catch((e: any) => {
    console.warn(`[IPC] send "${channel}" failed:`, e)
  })
}

/** Electron ipcRenderer.on equivalent */
async function ipcOn(
  channel: string,
  callback: (event: any, ...args: any[]) => void
): Promise<() => void> {
  const unlisten = await listen(channel, (event) => {
    callback({ ...event }, event.payload)
  })
  if (!listeners.has(channel)) {
    listeners.set(channel, new Set())
  }
  listeners.get(channel)!.add(unlisten)
  return () => {
    unlisten()
    listeners.get(channel)?.delete(unlisten)
  }
}

/** Electron ipcRenderer.removeAllListeners equivalent */
async function ipcRemoveAllListeners(channel: string): Promise<void> {
  const set = listeners.get(channel)
  if (set) {
    set.forEach((unlisten) => unlisten())
    set.clear()
    listeners.delete(channel)
  }
}

// ============================================================
// window.api — mirrors CeruMusic's preload API
// ============================================================

const api = {
  // Window controls
  minimize: () => getCurrentWindow().minimize(),
  maximize: () => getCurrentWindow().toggleMaximize(),
  close: () => getCurrentWindow().close(),
  setMiniMode: (isMini: boolean) => ipcSend('window-mini-mode', isMini),
  toggleFullscreen: () => getCurrentWindow().toggleMaximize(),

  onMusicCtrl: (callback: (event: any, ...args: any[]) => void) => {
    const unlistenPromise = ipcOn('music-control', callback)
    return () => {
      unlistenPromise.then((unlisten) => unlisten())
    }
  },

  powerSaveBlocker: {
    start: () => ipcInvoke('power-save-blocker__start'),
    stop: () => ipcInvoke('power-save-blocker__stop')
  },

  // Music SDK
  music: {
    requestSdk: (method: string, args: any) =>
      ipcInvoke('service-music-sdk-request', { method, args }),
    invoke: (channel: string, ...args: any[]) => ipcInvoke(channel, ...args)
  },

  // Plugin system
  plugins: {
    initialize: () => ipcInvoke('plugin__initialize'),
    getList: () => ipcInvoke('plugin__get_list'),
    add: (pluginCode: string, pluginName: string, targetPluginId?: string) =>
      ipcInvoke('plugin__add', { pluginCode, pluginName, targetPluginId }),
    uninstall: (pluginId: string) => ipcInvoke('plugin__uninstall', pluginId),
    getInfo: (pluginId: string) => ipcInvoke('plugin__get_info', pluginId),
    callMethod: (pluginId: string, method: string, argsJson: string) =>
      ipcInvoke('plugin__call_method', { pluginId, method, argsJson }),
    downloadAndAdd: (url: string, pluginType: string, targetPluginId?: string) =>
      ipcInvoke('plugin__download_and_add', { url, pluginType, targetPluginId }),
    getPluginType: (pluginId: string) => ipcInvoke('plugin__get_type', { pluginId }),
    getPluginLog: (pluginId: string) => ipcInvoke('plugin__get_log', { pluginId }),
    getConfigSchema: (pluginId: string) => ipcInvoke('plugin__get_config_schema', { pluginId }),
    getConfig: (pluginId: string) => ipcInvoke('plugin__get_config', { pluginId }),
    saveConfig: (pluginId: string, config: Record<string, any>) =>
      ipcInvoke('plugin__save_config', { pluginId, config }),
    testConnection: (pluginId: string) => ipcInvoke('plugin__test_connection', { pluginId }),
    selectAndAdd: (pluginType: string) => ipcInvoke('plugin__select_and_add', { pluginType }),
    onDeepLinkAdd: (
      callback: (payload: { url: string; type: 'lx' | 'cr'; targetPluginId?: string }) => void
    ) => {
      const unlistenPromise = ipcOn('plugin-add-link', (_, payload: any) => callback(payload))
      return () => {
        unlistenPromise.then((unlisten) => unlisten())
      }
    }
  },

  // AI assistant
  ai: {
    ask: (prompt: string) => ipcInvoke('ai-ask', prompt),
    askStream: (prompt: string, streamId: string) =>
      ipcInvoke('ai-ask-stream', { prompt, streamId }),
    onStreamChunk: (callback: (data: { streamId: string; chunk: string }) => void) => {
      ipcOn('ai-stream-chunk', (_, data: any) => callback(data))
    },
    onStreamEnd: (callback: (data: { streamId: string }) => void) => {
      ipcOn('ai-stream-end', (_, data: any) => callback(data))
    },
    onStreamError: (callback: (data: { streamId: string; error: string }) => void) => {
      ipcOn('ai-stream-error', (_, data: any) => callback(data))
    },
    removeStreamListeners: () => {
      ipcRemoveAllListeners('ai-stream-chunk')
      ipcRemoveAllListeners('ai-stream-end')
      ipcRemoveAllListeners('ai-stream-error')
    }
  },

  // Audio cache
  musicCache: {
    getInfo: () => ipcInvoke('music-cache__get-info'),
    clear: () => ipcInvoke('music-cache__clear'),
    getSize: () => ipcInvoke('music-cache__get-size')
  },

  // File
  file: {
    readFile: (path: string) => ipcInvoke('fs__read-file', path)
  },

  // Download manager
  download: {
    addTask: (songInfo: any, url: string, filePath: string, pluginId?: string, quality?: string, priority?: number) =>
      ipcInvoke('download__add_task', { songInfo, url, filePath, pluginId, quality, priority }),
    getTasks: () => ipcInvoke('download__get_tasks'),
    pauseTask: (taskId: string) => ipcInvoke('download__pause_task', taskId),
    resumeTask: (taskId: string) => ipcInvoke('download__resume_task', taskId),
    cancelTask: (taskId: string) => ipcInvoke('download__cancel_task', taskId),
    deleteTask: (taskId: string, deleteFile: boolean = false) =>
      ipcInvoke('download__delete_task', { task_id: taskId, delete_file: deleteFile }),
    pauseAllTasks: () => ipcInvoke('download__pause_all_tasks'),
    resumeAllTasks: () => ipcInvoke('download__resume_all_tasks'),
    retryTask: (taskId: string) => ipcInvoke('download__retry_task', taskId),
    setMaxConcurrent: (max: number) => ipcInvoke('download__set_max_concurrent', { max }),
    getMaxConcurrent: () => ipcInvoke('download__get_max_concurrent'),
    clearTasks: (type: 'queue' | 'completed' | 'failed' | 'all') =>
      ipcInvoke('download__clear_tasks', { task_type: type }),
    validateFiles: () => ipcInvoke('download__validate_files'),
    openFileLocation: (filePath: string) => ipcInvoke('download__open_file_location', { file_path: filePath }),
    onTaskAdded: (callback: (event: any, task: any) => void) => {
      const p = ipcOn('download:task-added', callback)
      return () => {
        p.then((unlisten) => unlisten())
      }
    },
    onTaskProgress: (callback: (event: any, task: any) => void) => {
      const p = ipcOn('download:task-progress', callback)
      return () => {
        p.then((unlisten) => unlisten())
      }
    },
    onTaskStatusChanged: (callback: (event: any, task: any) => void) => {
      const p = ipcOn('download:task-status-changed', callback)
      return () => {
        p.then((unlisten) => unlisten())
      }
    },
    onTaskCompleted: (callback: (event: any, task: any) => void) => {
      const p = ipcOn('download:task-completed', callback)
      return () => {
        p.then((unlisten) => unlisten())
      }
    },
    onTaskError: (callback: (event: any, task: any) => void) => {
      const p = ipcOn('download:task-error', callback)
      return () => {
        p.then((unlisten) => unlisten())
      }
    },
    onTaskDeleted: (callback: (event: any, taskId: string) => void) => {
      const p = ipcOn('download:task-deleted', callback)
      return () => {
        p.then((unlisten) => unlisten())
      }
    },
    onTasksReset: (callback: (event: any, tasks: any[]) => void) => {
      const p = ipcOn('download:tasks-reset', callback)
      return () => {
        p.then((unlisten) => unlisten())
      }
    }
  },

  // Song list management
  songList: {
    create: (name: string, description?: string, source?: string, meta?: Record<string, any>) =>
      ipcInvoke('songlist__create', { name, description, source, meta }),
    getAll: () => ipcInvoke('songlist__get_all'),
    getById: (hashId: string) => ipcInvoke('songlist__get', hashId),
    delete: (hashId: string) => ipcInvoke('songlist__delete', hashId),
    batchDelete: (hashIds: string[]) => ipcInvoke('songlist__batch_delete', hashIds),
    edit: (hashId: string, updates: any) => ipcInvoke('songlist__update', { id: hashId, ...updates }),
    updateCover: (hashId: string, coverImgUrl: string) =>
      ipcInvoke('songlist__update_cover', { id: hashId, cover_url: coverImgUrl }),
    search: (keyword: string, source?: string) =>
      ipcInvoke('songlist__search', { keyword, source }),
    getStatistics: () => ipcInvoke('songlist__get_statistics'),
    exists: (hashId: string) => ipcInvoke('songlist__exists', hashId),

    addSongs: (hashId: string, songs: any[]) =>
      ipcInvoke('songlist__add_songs', { playlist_id: hashId, songs }),
    removeSong: (hashId: string, songmid: string | number) =>
      ipcInvoke('songlist__remove_song', { playlist_id: hashId, songmid }),
    removeSongs: (hashId: string, songmids: (string | number)[]) =>
      ipcInvoke('songlist__remove_batch', { playlist_id: hashId, songmids }),
    clearSongs: (hashId: string) => ipcInvoke('songlist__clear_songs', hashId),
    getSongs: (hashId: string) => ipcInvoke('songlist__list_songs', hashId),
    getSongCount: (hashId: string) => ipcInvoke('songlist__count_songs', hashId),
    hasSong: (hashId: string, songmid: string | number) =>
      ipcInvoke('songlist__has_song', { playlist_id: hashId, songmid }),
    getSong: (hashId: string, songmid: string | number) =>
      ipcInvoke('songlist__get_song', { hashId, songmid }),
    searchSongs: (hashId: string, keyword: string) =>
      ipcInvoke('songlist__search_songs', { playlist_id: hashId, keyword }),
    getSongStatistics: (hashId: string) => ipcInvoke('songlist__get_song_statistics', hashId),
    validateIntegrity: (hashId: string) => ipcInvoke('songlist__validate_integrity', hashId),
    repairData: (hashId: string) => ipcInvoke('songlist__repair_data', hashId),
    forceSave: (hashId: string) => ipcInvoke('songlist__force_save', hashId),
    reorderSongs: (hashId: string, songmids: (string | number)[]) =>
      ipcInvoke('songlist__reorder_songs', { hashId, songmids }),
    moveSong: (hashId: string, songmid: string | number, toIndex: number) =>
      ipcInvoke('songlist__move_song', { hashId, songmid, toIndex }),

    getFavoritesId: () => ipcInvoke('songlist__get_favorites_id'),
    setFavoritesId: (id: string) => ipcInvoke('songlist__set_favorites_id', id)
  },

  getUserConfig: () => ipcInvoke('get-user-config'),

  hotkeys: {
    get: () => ipcInvoke('hotkeys__get'),
    set: (payload: any) => ipcInvoke('hotkeys__set', payload)
  },

  // Auto updater
  autoUpdater: {
    checkForUpdates: () => ipcInvoke('auto-updater__check-for-updates'),
    downloadUpdate: () => ipcInvoke('auto-updater__download-update'),
    quitAndInstall: () => ipcInvoke('auto-updater__quit-and-install'),
    getDownloadedPath: (updateInfo?: any) =>
      ipcInvoke('auto-updater__get-downloaded-path', updateInfo),
    onCheckingForUpdate: (callback: () => void) => {
      ipcOn('auto-updater__checking-for-update', callback)
    },
    onUpdateAvailable: (callback: () => void) => {
      ipcOn('auto-updater__update-available', callback)
    },
    onUpdateNotAvailable: (callback: () => void) => {
      ipcOn('auto-updater__update-not-available', callback)
    },
    onDownloadProgress: (callback: (progress: any) => void) => {
      ipcOn('auto-updater__download-progress', (_, progress: any) => callback(progress))
    },
    onUpdateDownloaded: (callback: () => void) => {
      ipcOn('auto-updater__update-downloaded', callback)
    },
    onError: (callback: (error: string) => void) => {
      ipcOn('auto-updater__error', (_, error: any) => callback(error))
    },
    onDownloadStarted: (callback: (updateInfo: any) => void) => {
      ipcOn('auto-updater__download-started', (_, updateInfo: any) => callback(updateInfo))
    },
    removeAllListeners: () => {
      ipcRemoveAllListeners('auto-updater__checking-for-update')
      ipcRemoveAllListeners('auto-updater__update-available')
      ipcRemoveAllListeners('auto-updater__update-not-available')
      ipcRemoveAllListeners('auto-updater__download-started')
      ipcRemoveAllListeners('auto-updater__download-progress')
      ipcRemoveAllListeners('auto-updater__update-downloaded')
      ipcRemoveAllListeners('auto-updater__error')
    }
  },

  ping: (callback: Function) => ipcOn('song-ended', () => callback()),
  pingService: {
    start: () => ipcSend('startPing'),
    stop: () => ipcSend('stopPing')
  },

  // Directory settings
  directorySettings: {
    getDirectories: () => ipcInvoke('directory-settings__get-directories'),
    selectCacheDir: () => ipcInvoke('directory-settings__select-cache-dir'),
    selectDownloadDir: () => ipcInvoke('directory-settings__select-download-dir'),
    saveDirectories: (directories: any) =>
      ipcInvoke('directory-settings__save-directories', directories),
    resetDirectories: () => ipcInvoke('directory-settings__reset-directories'),
    openDirectory: (dirPath: string) =>
      ipcInvoke('directory-settings__open-directory', dirPath),
    getDirectorySize: (dirPath: string) =>
      ipcInvoke('directory-settings__get-directory-size', dirPath)
  },

  // Local music
  localMusic: {
    selectDirs: () => ipcInvoke('local_music__select_dirs'),
    scan: async (dirs: string[]) => {
      const res = await ipcInvoke('local_music__scan', dirs)
      if (typeof res === 'string') {
        try {
          return JSON.parse(res)
        } catch {
          return []
        }
      }
      return Array.isArray(res) ? res : []
    },
    writeTags: (filePath: string, songInfo: any, tagWriteOptions: any) =>
      ipcInvoke('local_music__write_tags', { filePath, songInfo, tagWriteOptions }),
    getDirs: () => ipcInvoke('dir__get_all'),
    setDirs: (dirs: string[]) => ipcInvoke('dir__set', dirs),
    getList: () => ipcInvoke('track__get_all'),
    getUrlById: (id: string | number) => ipcInvoke('track__get_by_id', id),
    clearIndex: () => ipcInvoke('track__clear'),
    getCoverBase64: async (trackId: string) => {
      try {
        return await ipcInvoke('local_music__get_cover', trackId)
      } catch {
        return ''
      }
    },
    getCoversBase64: async (trackIds: string[]) => {
      try {
        return await ipcInvoke('local_music__get_covers', trackIds)
      } catch {
        return {}
      }
    },
    getTags: async (songmid: string, includeLyrics: boolean = true) => {
      try {
        return await ipcInvoke('local_music__get_tags', { songmid, includeLyrics })
      } catch {
        return null
      }
    },
    getLyric: async (songmid: string) => {
      try {
        return await ipcInvoke('local_music__get_lyric', songmid)
      } catch {
        return ''
      }
    },
    onScanProgress: (callback: (processed: number, total: number) => void) => {
      const p = ipcOn('local-music__scan-progress', (_, data: any) =>
        callback(data.processed, data.total)
      )
      return () => {
        p.then((unlisten) => unlisten())
      }
    },
    onScanFinished: (callback: (resList: any[]) => void) => {
      const p = ipcOn('local-music__scan-finished', (_, resList: any) => callback(resList))
      return () => {
        p.then((unlisten) => unlisten())
      }
    },
    removeScanProgress: () => ipcRemoveAllListeners('local-music__scan-progress'),
    removeScanFinished: () => ipcRemoveAllListeners('local-music__scan-finished'),
    batchMatch: (songmids: string[]) => ipcInvoke('local-music__batch-match', songmids),
    onBatchMatchProgress: (
      callback: (processed: number, total: number, matched: number) => void
    ) => {
      const p = ipcOn('local-music__batch-match-progress', (_, data: any) =>
        callback(data.processed, data.total, data.matched)
      )
      return () => {
        p.then((unlisten) => unlisten())
      }
    },
    onBatchMatchFinished: (callback: (res: any) => void) => {
      const p = ipcOn('local-music__batch-match-finished', (_, res: any) => callback(res))
      return () => {
        p.then((unlisten) => unlisten())
      }
    },
    removeBatchMatchListeners: () => {
      ipcRemoveAllListeners('local-music__batch-match-progress')
      ipcRemoveAllListeners('local-music__batch-match-finished')
    }
  },

  // Plugin notifications
  pluginNotice: {
    onPluginNotice(callback: (data: string) => any) {
      const p = ipcOn('plugin-notice', (_, data: any) => callback(data))
      return () => {
        p.then((unlisten) => unlisten())
      }
    }
  },

  // System audio
  systemAudio: {
    getDefaultScreenSourceId: async () => {
      return ipcInvoke('system-audio__get-default-source-id')
    },
    getAllScreenSourceIds: async () => {
      return []
    }
  },

  // DLNA / Screen casting
  dlna: {
    startSearch: () => ipcInvoke('dlna__start_search'),
    stopSearch: () => ipcInvoke('dlna__stop_search'),
    getDevices: () => ipcInvoke('dlna__get_devices'),
    play: (payload: { url: string; location: string; title: string }) =>
      ipcInvoke('dlna__play', payload),
    pause: () => ipcInvoke('dlna__pause'),
    resume: () => ipcInvoke('dlna__resume'),
    stop: () => ipcInvoke('dlna__stop'),
    seek: (seconds: number) => ipcInvoke('dlna__seek', { seconds }),
    setVolume: (volume: number) => ipcInvoke('dlna__set_volume', { volume }),
    getPosition: () => ipcInvoke('dlna__get_position')
  }
}

// ============================================================
// window.electron.ipcRenderer — simplified compatibility layer
// ============================================================

const electronCompat = {
  ipcRenderer: {
    invoke: ipcInvoke,
    send: ipcSend,
    on: ipcOn,
    removeAllListeners: ipcRemoveAllListeners,
    removeListener: async (_channel: string, _handler: Function) => {
      // Best-effort removal — Tauri events use UnlistenFn pattern
    }
  }
}

// ============================================================
// Expose to window
// ============================================================

;(window as any).api = api
;(window as any).electron = electronCompat

export { api, electronCompat }
