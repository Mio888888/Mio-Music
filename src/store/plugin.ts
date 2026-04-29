import { defineStore } from 'pinia'
import { ref } from 'vue'

export interface PluginInfo {
  name: string
  version: string
  author: string
  description: string
}

export interface LoadedPlugin {
  plugin_id: string
  plugin_name: string
  plugin_info: PluginInfo
  supported_sources: Array<{ name: string; qualities: string[] }>
}

export const usePluginStore = defineStore('plugin', () => {
  const plugins = ref<LoadedPlugin[]>([])
  const loading = ref(false)

  async function initialize() {
    loading.value = true
    try {
      const res = await (window as any).api.plugins.initialize()
      if (res?.success) {
        plugins.value = res.data || []
      }
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

  async function addPlugin(pluginCode: string, pluginName: string, targetPluginId?: string) {
    const res = await (window as any).api.plugins.add(pluginCode, pluginName, targetPluginId)
    if (res?.success) {
      await refresh()
      return res.data as LoadedPlugin
    }
    throw new Error(res?.message || '添加插件失败')
  }

  async function uninstallPlugin(pluginId: string) {
    const res = await (window as any).api.plugins.uninstall(pluginId)
    if (res?.success) {
      plugins.value = plugins.value.filter(p => p.plugin_id !== pluginId)
    } else {
      throw new Error(res?.message || '卸载插件失败')
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
    throw new Error(res?.message || '下载插件失败')
  }

  return {
    plugins,
    loading,
    initialize,
    refresh,
    addPlugin,
    uninstallPlugin,
    getPluginInfo,
    downloadAndAdd
  }
})
