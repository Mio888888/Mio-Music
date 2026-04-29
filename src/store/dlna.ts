import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useDlnaStore = defineStore('dlna', () => {
  const devices = ref<any[]>([])
  const currentDevice = ref<any>(null)
  const isSearching = ref(false)

  const api = (window as any).api

  const startSearch = async () => {
    isSearching.value = true
    try {
      await api.dlna.startSearch()
      setTimeout(async () => {
        try {
          const res = await api.dlna.getDevices()
          if (res?.success) devices.value = res.data || []
        } catch (e) {
          console.error('DLNA get devices error', e)
        }
        isSearching.value = false
      }, 3000)
    } catch (e) {
      console.error('DLNA start search error', e)
      isSearching.value = false
    }
  }

  const stopSearch = async () => {
    try {
      await api.dlna.stopSearch()
    } catch (e) {
      console.error('DLNA stop search error', e)
    }
    isSearching.value = false
  }

  const play = async (url: string, title: string) => {
    if (!currentDevice.value) return
    try {
      await api.dlna.play({ url, location: currentDevice.value.location, title })
    } catch (e) {
      console.error('DLNA play error', e)
    }
  }

  const pause = async () => {
    try { await api.dlna.pause() } catch (e) { console.error('DLNA pause error', e) }
  }

  const resume = async () => {
    try { await api.dlna.resume() } catch (e) { console.error('DLNA resume error', e) }
  }

  const stop = async () => {
    try { await api.dlna.stop() } catch (e) { console.error('DLNA stop error', e) }
  }

  const seek = async (seconds: number) => {
    try { await api.dlna.seek(seconds) } catch (e) { console.error('DLNA seek error', e) }
  }

  const setVolume = async (volume: number) => {
    try { await api.dlna.setVolume(volume) } catch (e) { console.error('DLNA setVolume error', e) }
  }

  const getPosition = async () => {
    try {
      const res = await api.dlna.getPosition()
      return res?.data || 0
    } catch (e) {
      console.error('DLNA getPosition error', e)
      return 0
    }
  }

  return {
    devices, currentDevice, isSearching,
    startSearch, stopSearch, play, pause, resume, stop, seek, setVolume, getPosition
  }
})
