import { defineStore } from 'pinia'
import { ref } from 'vue'
import { LocalUserDetailStore } from './LocalUserDetail'
import { ControlAudioStore } from './ControlAudio'
import { useGlobalPlayStatusStore } from './GlobalPlayStatus'
import PluginRunner, { type PluginSources } from '@/utils/plugin/PluginRunner'
import i18n from '@/locales'

export interface PluginInfo {
  name: string
  version: string
  author: string
  description: string
}

export interface PluginSource {
  source_id: string
  name: string
  qualities: string[]
}

export interface LoadedPlugin {
  plugin_id: string
  plugin_name: string
  plugin_info: PluginInfo
  supported_sources: PluginSource[]
  plugin_type: 'music-source' | 'service'
}

export interface PluginConfigField {
  key: string
  label: string
  type: 'text' | 'password' | 'number' | 'select'
  required?: boolean
  default?: any
  placeholder?: string
  options?: { label: string; value: any }[]
}

export const usePluginStore = defineStore('plugin', () => {
  const plugins = ref<LoadedPlugin[]>([])
  const loading = ref(false)
  const currentPluginId = ref('')
  const currentPluginName = ref('')
  let sourceSyncGeneration = 0

  function applySources(plugin: LoadedPlugin, sources: PluginSources) {
    const userStore = LocalUserDetailStore()
    const info = userStore.userInfo
    const available = userStore.mergeBuiltInSources(info, sources)
    info.pluginId = plugin.plugin_id
    info.pluginName = plugin.plugin_info.name
    info.supportedSources = available
    if (!available[info.selectSources as string]) {
      info.selectSources = Object.keys(available)[0] || ''
    }
    const qualities = available[info.selectSources as string]?.qualitys || []
    if (!qualities.includes(info.selectQuality as string)) {
      info.selectQuality = qualities[qualities.length - 1] || ''
    }
    for (const [id, quality] of Object.entries(info.sourceQualityMap || {})) {
      if (!available[id]?.qualitys.includes(quality)) delete info.sourceQualityMap![id]
    }
  }

  function applyStaticSources(plugin: LoadedPlugin) {
    const sources: PluginSources = {}
    for (const source of plugin.supported_sources || []) {
      const id = source.source_id || source.name
      if (!id || ['__proto__', 'constructor', 'prototype'].includes(id)) continue
      sources[id] = { name: source.name, type: 'music', qualitys: source.qualities }
    }
    applySources(plugin, sources)
  }

  async function syncRuntimeSources() {
    const generation = ++sourceSyncGeneration
    const pluginId = currentPluginId.value
    const plugin = plugins.value.find(p => p.plugin_id === pluginId)
    if (!plugin) return
    if (plugin.plugin_type === 'service') {
      applyStaticSources(plugin)
      return
    }
    try {
      const sources = await PluginRunner.getSources(pluginId)
      if (generation !== sourceSyncGeneration || currentPluginId.value !== pluginId) return
      if (!Object.keys(sources).length) {
        applyStaticSources(plugin)
        return
      }
      plugin.supported_sources = Object.entries(sources).map(([id, source]) => ({
        source_id: id, name: source.name, qualities: source.qualitys,
      }))

      applySources(plugin, sources)
    } catch (e) {
      console.warn('[PluginStore] runtime sources unavailable; keeping static metadata:', e)
      if (generation === sourceSyncGeneration && currentPluginId.value === pluginId) {
        applyStaticSources(plugin)
      }
    }
  }

  function _loadPersistedSelection() {
    const saved = localStorage.getItem('pluginId')
    if (saved) {
      currentPluginId.value = saved
      currentPluginName.value = localStorage.getItem('pluginName') || ''
    }
  }

  function _persistSelection() {
    if (currentPluginId.value) {
      localStorage.setItem('pluginId', currentPluginId.value)
      localStorage.setItem('pluginName', currentPluginName.value)
    } else {
      localStorage.removeItem('pluginId')
      localStorage.removeItem('pluginName')
    }
  }

  async function initialize() {
    loading.value = true
    try {
      const res = await (window as any).api.plugins.initialize()
      if (res?.success) {
        plugins.value = res.data || []
      }
      _loadPersistedSelection()

      // Sync userInfo with latest plugin data
      const userStore = LocalUserDetailStore()
      if (currentPluginId.value) {
        const plugin = plugins.value.find(p => p.plugin_id === currentPluginId.value)
        if (plugin) {
          await syncRuntimeSources()
        } else {
          // Plugin was uninstalled — clear stale plugin data, keep built-in sources
          userStore.userInfo.supportedSources = userStore.mergeBuiltInSources(userStore.userInfo, {})
          const availableSources = userStore.userInfo.supportedSources || {}
          const currentSource = userStore.userInfo.selectSources as string
          if (currentSource && !availableSources[currentSource]) {
            userStore.userInfo.selectSources = Object.keys(availableSources)[0] || ''
          }
        }
      } else {
        // No plugin selected — ensure supportedSources only has built-in sources
        userStore.userInfo.supportedSources = userStore.mergeBuiltInSources(userStore.userInfo, {})
        const availableSources = userStore.userInfo.supportedSources || {}
        const currentSource = userStore.userInfo.selectSources as string
        if (currentSource && !availableSources[currentSource]) {
          userStore.userInfo.selectSources = Object.keys(availableSources)[0] || ''
        }
      }
    } catch (e) {
      console.error('[PluginStore] initialize failed:', e)
    } finally {
      loading.value = false
    }
  }

  async function refresh() {
    ++sourceSyncGeneration
    loading.value = true
    try {
      const res = await (window as any).api.plugins.getList()
      if (res?.success) {
        plugins.value = res.data || []
        await syncRuntimeSources()
      }
    } catch (e) {
      console.error('[PluginStore] refresh failed:', e)
    } finally {
      loading.value = false
    }
  }

  async function selectPlugin(plugin: LoadedPlugin) {
    currentPluginId.value = plugin.plugin_id
    currentPluginName.value = plugin.plugin_info.name
    _persistSelection()
    await syncRuntimeSources()
  }

  function clearSelection() {
    ++sourceSyncGeneration
    currentPluginId.value = ''
    currentPluginName.value = ''
    _persistSelection()
  }

  function isSelected(pluginId: string): boolean {
    return currentPluginId.value === pluginId
  }

  function isServicePlugin(plugin: LoadedPlugin): boolean {
    return plugin.plugin_type === 'service'
  }

  async function addPlugin(pluginCode: string, pluginName: string, targetPluginId?: string) {
    const res = await (window as any).api.plugins.add(pluginCode, pluginName, targetPluginId)
    if (res?.success) {
      // 清除该插件的执行缓存，确保下次使用新代码
      if (res.data?.plugin_id) PluginRunner.clearCache(res.data.plugin_id)
      await refresh()
      return res.data as LoadedPlugin
    }
    throw new Error(res?.error || i18n.global.t('plugin.addFailed'))
  }

  async function uninstallPlugin(pluginId: string) {
    const res = await (window as any).api.plugins.uninstall(pluginId)
    if (res?.success) {
      plugins.value = plugins.value.filter(p => p.plugin_id !== pluginId)
      PluginRunner.clearCache(pluginId)
      if (currentPluginId.value === pluginId) {
        clearSelection()
      }

      const userStore = LocalUserDetailStore()
      // Snapshot current sources before cleanup
      const oldKeys = new Set(Object.keys(userStore.userInfo.supportedSources || {}))

      // Rebuild sources: only built-in (Subsonic) remain
      userStore.userInfo.supportedSources = userStore.mergeBuiltInSources(userStore.userInfo, {})
      userStore.userInfo.pluginId = ''
      userStore.userInfo.pluginName = ''

      // Diff: which source keys were actually removed?
      const newKeys = new Set(Object.keys(userStore.userInfo.supportedSources || {}))
      const removedSourceKeys = new Set<string>()
      for (const key of oldKeys) {
        if (!newKeys.has(key)) removedSourceKeys.add(key)
      }

      if (removedSourceKeys.size > 0) {
        // Remove songs from removed sources in the play queue
        const newList = userStore.list.filter(song => {
          const src = (song as any).source
          return !src || src === 'local' || !removedSourceKeys.has(src)
        })
        userStore.replaceSongList(newList)

        // Pause playback if current song is from a removed source
        const globalPlayStatus = useGlobalPlayStatusStore()
        const playingSource = (globalPlayStatus.player.songInfo as any)?.source
        if (playingSource && removedSourceKeys.has(playingSource)) {
          try {
            const audio = ControlAudioStore()
            await audio.stop()
          } catch (e) {
            console.warn('[PluginStore] 暂停播放失败:', e)
          }
        }

        // Update lastPlaySongId if the song was removed
        if (newList.length > 0) {
          const lastId = userStore.userInfo.lastPlaySongId
          if (lastId && !newList.find(s => s.songmid === lastId)) {
            userStore.userInfo.lastPlaySongId = newList[0].songmid
          }
        } else {
          userStore.userInfo.lastPlaySongId = null
        }
      }

      // Validate selectSources against remaining sources
      const available = userStore.userInfo.supportedSources || {}
      const currentSource = userStore.userInfo.selectSources as string
      if (currentSource && !available[currentSource]) {
        userStore.userInfo.selectSources = Object.keys(available)[0] || ''
        userStore.userInfo.selectQuality = ''
      }
    } else {
      throw new Error(res?.error || i18n.global.t('plugin.uninstallFailed'))
    }
  }

  async function getPluginInfo(pluginId: string) {
    const res = await (window as any).api.plugins.getInfo(pluginId)
    if (res?.success) return res.data as LoadedPlugin
    return null
  }

  async function downloadAndAdd(url: string, pluginType: string, targetPluginId?: string) {
    const res = await (window as any).api.plugins.downloadAndAdd(url, pluginType, targetPluginId)
    if (res?.success) {
      if (res.data?.plugin_id) PluginRunner.clearCache(res.data.plugin_id)
      await refresh()
      return res.data as LoadedPlugin
    }
    throw new Error(res?.error || i18n.global.t('plugin.downloadFailed'))
  }

  async function selectAndAdd(pluginType: string) {
    const res = await (window as any).api.plugins.selectAndAdd(pluginType)
    if (res?.data?.canceled) return null
    if (res?.success) {
      if (res.data?.plugin_id) PluginRunner.clearCache(res.data.plugin_id)
      await refresh()
      return res.data as LoadedPlugin
    }
    throw new Error(res?.error || i18n.global.t('plugin.importFailed'))
  }

  async function getPluginLog(pluginId: string) {
    const res = await (window as any).api.plugins.getPluginLog(pluginId)
    if (res?.success) return res.data as string[]
    return []
  }

  async function getConfigSchema(pluginId: string) {
    const res = await (window as any).api.plugins.getConfigSchema(pluginId)
    if (res?.success) return res.data as PluginConfigField[]
    return []
  }

  async function getConfig(pluginId: string) {
    const res = await (window as any).api.plugins.getConfig(pluginId)
    if (res?.success) return res.data as Record<string, any>
    return {}
  }

  async function saveConfig(pluginId: string, config: Record<string, any>) {
    const res = await (window as any).api.plugins.saveConfig(pluginId, config)
    if (!res?.success) throw new Error(res?.error || i18n.global.t('plugin.saveConfigFailed'))
  }

  async function testConnection(pluginId: string) {
    const res = await (window as any).api.plugins.testConnection(pluginId)
    if (res?.success) return res.data as { success: boolean; message: string }
    return { success: false, message: res?.error || i18n.global.t('plugin.testConnectionFailed') }
  }

  return {
    plugins,
    loading,
    currentPluginId,
    currentPluginName,
    initialize,
    refresh,
    selectPlugin,
    clearSelection,
    isSelected,
    isServicePlugin,
    addPlugin,
    uninstallPlugin,
    getPluginInfo,
    downloadAndAdd,
    selectAndAdd,
    getPluginLog,
    getConfigSchema,
    getConfig,
    saveConfig,
    testConnection
  }
})
