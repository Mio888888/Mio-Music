import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { MessagePlugin } from 'tdesign-vue-next'

export interface AudioOutputDevice { deviceId: string; kind: MediaDeviceKind; label: string; groupId: string }
export interface DeviceStats { sampleRate: number; channelCount: number; latency: number }

export const useAudioOutputStore = defineStore('audioOutput', () => {
  const devices = ref<AudioOutputDevice[]>([])
  const currentDeviceId = ref<string>('default')
  const isLoading = ref(false)
  const error = ref<string | null>(null)
  const deviceStats = ref<DeviceStats>({ sampleRate: 0, channelCount: 0, latency: 0 })
  const primaryDeviceId = ref<string>('default')
  const secondaryDeviceId = ref<string>('')
  const activeABChannel = ref<'A' | 'B'>('A')

  const sortedDevices = computed(() => [...devices.value].sort((a, b) => {
    if (a.deviceId === 'default') return -1
    if (b.deviceId === 'default') return 1
    return a.label.localeCompare(b.label)
  }))

  const currentDeviceLabel = computed(() => {
    const device = devices.value.find((d) => d.deviceId === currentDeviceId.value)
    return device ? device.label : 'Default'
  })

  const scanDevices = async () => {
    if (!navigator.mediaDevices?.enumerateDevices) return
    isLoading.value = true
    error.value = null
    try {
      const allDevices = await navigator.mediaDevices.enumerateDevices()
      devices.value = allDevices.filter((d) => d.kind === 'audiooutput').map((d) => ({
        deviceId: d.deviceId, kind: d.kind, label: d.label || `Speaker (${d.deviceId.slice(0, 5)}...)`, groupId: d.groupId
      }))
      if (currentDeviceId.value !== 'default' && !devices.value.find((d) => d.deviceId === currentDeviceId.value)) {
        currentDeviceId.value = 'default'
        MessagePlugin.warning('上次使用的音频设备未找到，已切换回默认设备')
      }
    } catch (err: any) {
      error.value = err.message || '无法获取音频设备列表'
      MessagePlugin.error(error.value || '获取音频设备列表失败')
    } finally { isLoading.value = false }
  }

  const setDevice = async (deviceId: string) => {
    if (deviceId === currentDeviceId.value) return
    try {
      currentDeviceId.value = deviceId
      if (activeABChannel.value === 'A') primaryDeviceId.value = deviceId
      else secondaryDeviceId.value = deviceId
      MessagePlugin.success(`已切换音频输出至: ${currentDeviceLabel.value}`)
    } catch (err: any) {
      error.value = err.message
      MessagePlugin.error('切换音频设备失败')
    }
  }

  const toggleAB = () => {
    if (activeABChannel.value === 'A') {
      if (secondaryDeviceId.value && secondaryDeviceId.value !== primaryDeviceId.value) {
        activeABChannel.value = 'B'
        setDevice(secondaryDeviceId.value)
      } else MessagePlugin.info('请先设置对比设备 (设备 B)')
    } else {
      activeABChannel.value = 'A'
      setDevice(primaryDeviceId.value)
    }
  }

  const handleDeviceChange = () => { scanDevices() }
  const init = () => {
    if (!navigator.mediaDevices) return
    scanDevices()
    navigator.mediaDevices.removeEventListener('devicechange', handleDeviceChange)
    navigator.mediaDevices.addEventListener('devicechange', handleDeviceChange)
  }

  const playTestSound = (deviceId: string) => {
    try {
      const AudioContextClass = window.AudioContext || (window as any).webkitAudioContext
      const ctx = new AudioContextClass()
      if ((ctx as any).setSinkId && deviceId !== 'default') { try { (ctx as any).setSinkId(deviceId) } catch (e) {} }
      const osc = ctx.createOscillator()
      const gain = ctx.createGain()
      osc.connect(gain); gain.connect(ctx.destination)
      osc.type = 'sine'; osc.frequency.setValueAtTime(440, ctx.currentTime)
      osc.frequency.exponentialRampToValueAtTime(880, ctx.currentTime + 0.1)
      gain.gain.setValueAtTime(0.1, ctx.currentTime)
      gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.5)
      osc.start(); osc.stop(ctx.currentTime + 0.5)
      setTimeout(() => ctx.close(), 600)
    } catch { MessagePlugin.error('测试音播放失败') }
  }

  return { devices, sortedDevices, currentDeviceId, isLoading, error, deviceStats, currentDeviceLabel, primaryDeviceId, secondaryDeviceId, activeABChannel, scanDevices, setDevice, toggleAB, init, playTestSound }
}, { persist: { paths: ['currentDeviceId', 'primaryDeviceId', 'secondaryDeviceId'] } as any })
