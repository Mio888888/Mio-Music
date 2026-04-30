import { defineStore } from 'pinia'
import { ref } from 'vue'

export interface PluginInfo {
  name: string
  version: string
  author: string
  description: string
}

export interface PluginSource {
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
    } catch (e) {
      console.error('[PluginStore] initialize failed:', e)
    } finally {
      loading.value = false
    }
  }

  async function refresh() {
    loading.value = true
    try {
      const res = await (window as any).api.plugins.getList()
      if (res?.success) {
        plugins.value = res.data || []
      }
    } catch (e) {
      console.error('[PluginStore] refresh failed:', e)
    } finally {
      loading.value = false
    }
  }

  function selectPlugin(plugin: LoadedPlugin) {
    currentPluginId.value = plugin.plugin_id
    currentPluginName.value = plugin.plugin_info.name
    _persistSelection()
  }

  function clearSelection() {
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
      await refresh()
      return res.data as LoadedPlugin
    }
    throw new Error(res?.error || '添加插件失败')
  }

  async function uninstallPlugin(pluginId: string) {
    const res = await (window as any).api.plugins.uninstall(pluginId)
    if (res?.success) {
      plugins.value = plugins.value.filter(p => p.plugin_id !== pluginId)
      if (currentPluginId.value === pluginId) {
        clearSelection()
      }
    } else {
      throw new Error(res?.error || '卸载插件失败')
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
      await refresh()
      return res.data as LoadedPlugin
    }
    throw new Error(res?.error || '下载插件失败')
  }

  async function selectAndAdd(pluginType: string) {
    const res = await (window as any).api.plugins.selectAndAdd(pluginType)
    if (res?.canceled) return null
    if (res?.success) {
      await refresh()
      return res.data as LoadedPlugin
    }
    throw new Error(res?.error || '导入插件失败')
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
    if (!res?.success) throw new Error(res?.error || '保存配置失败')
  }

  async function testConnection(pluginId: string) {
    const res = await (window as any).api.plugins.testConnection(pluginId)
    if (res?.success) return res.data as { success: boolean; message: string }
    return { success: false, message: res?.error || '测试连接失败' }
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
