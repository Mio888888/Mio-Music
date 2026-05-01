<script setup lang="ts">
import {
  onMounted,
  onUnmounted,
  provide,
  ref,
  onActivated,
  onDeactivated,
  watch
} from 'vue'
import { ControlAudioStore } from '@/store/ControlAudio'
import { useEqualizerStore } from '@/store/Equalizer'
import { useAudioEffectsStore } from '@/store/AudioEffects'
import { useAudioOutputStore } from '@/store/audioOutput'
import { storeToRefs } from 'pinia'
import AudioManager from '@/utils/audio/AudioManager'
import { crossfadeState } from '@/utils/audio/crossfade'
import createLogger from '@/utils/logger'

const log = createLogger('GlobalAudio')

type AudioSlot = 'A' | 'B'

const audioStore = ControlAudioStore()
const eqStore = useEqualizerStore()
const effectStore = useAudioEffectsStore()
const audioOutputStore = useAudioOutputStore()

// 双槽 audio 元素引用
const audioARef = ref<HTMLAudioElement>()
const audioBRef = ref<HTMLAudioElement>()

// 提供订阅方法给子组件使用
provide('audioSubscribe', audioStore.subscribe)

// 判断事件是否来自当前活跃槽
const isPrimarySlot = (slot: AudioSlot) => audioStore.Audio.primarySlot === slot

// 应用均衡器设置（仅在 EQ 启用时创建 AudioContext，避免影响基础播放）
const applyGlobalEQ = (el: HTMLAudioElement) => {
  if (!eqStore.enabled) return
  // 首次启用 EQ 时创建 AudioContext 路由音频
  AudioManager.getOrCreateAudioSource(el)
  eqStore.gains.forEach((gain, index) => {
    AudioManager.setEqualizerBand(el, index, gain)
  })
}

// Apply Audio Effects（仅在对应效果启用时创建 AudioContext）
const applyGlobalEffects = (el: HTMLAudioElement) => {
  const { bassBoost, surround, balance } = storeToRefs(effectStore)
  const needsContext = bassBoost.value.enabled || surround.value.enabled ||
    (balance.value.enabled && balance.value.value !== 0)

  if (!needsContext) return

  AudioManager.getOrCreateAudioSource(el)
  AudioManager.setBassBoost(el, bassBoost.value.enabled ? bassBoost.value.gain : 0)
  AudioManager.setSurroundMode(el, surround.value.enabled ? surround.value.mode : 'off')
  AudioManager.setBalance(el, balance.value.enabled ? balance.value.value : 0)
}

// 对两个元素都应用 EQ / Effects
const applyToBoth = () => {
  if (audioARef.value) {
    applyGlobalEQ(audioARef.value)
    applyGlobalEffects(audioARef.value)
  }
  if (audioBRef.value) {
    applyGlobalEQ(audioBRef.value)
    applyGlobalEffects(audioBRef.value)
  }
}

// 记录组件被停用前的播放状态
let wasPlaying = false
let playbackPosition = 0

onMounted(() => {
  audioStore.init(audioARef.value || null, audioBRef.value || null)
  audioOutputStore.init()

  // 不在 mount 时创建 AudioContext（避免 Web Audio API 路由阻断直接播放）
  // AudioContext 会在用户首次启用 EQ / 音效时按需创建

  const activeEl = audioStore.Audio.audio
  if (activeEl) {
    // Apply saved audio output device
    if (audioOutputStore.currentDeviceId !== 'default') {
      AudioManager.setAudioOutputDevice(activeEl, audioOutputStore.currentDeviceId)
    }

    // Initial stats update
    const stats = AudioManager.getAudioContextStats(activeEl)
    if (stats) {
      audioOutputStore.deviceStats = {
        sampleRate: stats.sampleRate,
        channelCount: stats.channels,
        latency: stats.latency
      }
    }
  }
  log.debug('音频组件初始化完成（双槽）')
})

watch(
  () => audioOutputStore.currentDeviceId,
  async (newId) => {
    // 两个槽都切换输出设备
    if (audioARef.value) {
      await AudioManager.setAudioOutputDevice(audioARef.value, newId)
    }
    if (audioBRef.value) {
      await AudioManager.setAudioOutputDevice(audioBRef.value, newId)
    }
    const activeEl = audioStore.Audio.audio
    if (activeEl) {
      const stats = AudioManager.getAudioContextStats(activeEl)
      if (stats) {
        audioOutputStore.deviceStats = {
          sampleRate: stats.sampleRate,
          channelCount: stats.channels,
          latency: stats.latency
        }
      }
    }
  }
)

watch(
  () => audioOutputStore.currentRustDeviceId,
  () => {
    // Rust backend changes system default output; WebView audio follows automatically.
    // Just refresh device stats.
    const activeEl = audioStore.Audio.audio
    if (activeEl) {
      const stats = AudioManager.getAudioContextStats(activeEl)
      if (stats) {
        audioOutputStore.deviceStats = {
          sampleRate: stats.sampleRate,
          channelCount: stats.channels,
          latency: stats.latency
        }
      }
    }
  }
)

watch(
  [() => eqStore.enabled, () => eqStore.gains],
  () => {
    applyToBoth()
  },
  { deep: true }
)

watch(
  [() => effectStore.bassBoost, () => effectStore.surround, () => effectStore.balance],
  () => {
    applyToBoth()
  },
  { deep: true }
)

/**
 * 监听 srcA 变化：当 A 槽 URL 清空或更换时，暂停当前播放。
 * Vue 的 :src 绑定会在 DOM 更新时自动设置 audio.src，浏览器自动触发加载，
 * 无需手动调用 load()（否则会中断进行中的 play() 导致 AbortError）。
 */
watch(
  () => audioStore.Audio.srcA,
  (newUrl) => {
    const a = audioARef.value
    if (!a) return
    if (!newUrl) {
      if (a.getAttribute('src')) {
        try { a.pause() } catch {}
        try {
          a.removeAttribute('src')
          a.load()
        } catch {}
      }
      return
    }
    try { a.pause() } catch {}
    // 不再手动调用 load() — 浏览器在 src 属性变更时自动加载
  }
)

watch(
  () => audioStore.Audio.srcB,
  (newUrl) => {
    const b = audioBRef.value
    if (!b) return
    if (!newUrl) {
      if (b.getAttribute('src')) {
        try { b.pause() } catch {}
        try {
          b.removeAttribute('src')
          b.load()
        } catch {}
      }
      return
    }
    try { b.pause() } catch {}
  }
)
// 组件被激活时（从缓存中恢复）
onActivated(() => {
  log.debug('音频组件被激活')
  audioStore.init(audioARef.value || null, audioBRef.value || null)

  const activeEl = audioStore.Audio.audio
  // Re-apply output device
  if (activeEl && audioOutputStore.currentDeviceId !== 'default') {
    AudioManager.setAudioOutputDevice(activeEl, audioOutputStore.currentDeviceId)
  }

  // 如果之前正在播放，恢复播放
  if (wasPlaying && audioStore.Audio.url && activeEl) {
    if (playbackPosition > 0) {
      activeEl.currentTime = playbackPosition
      audioStore.setCurrentTime(playbackPosition)
    }
    audioStore.start().catch((error) => {
      log.error('恢复播放失败:', error)
    })
  }
})

// 组件被停用时（缓存但不销毁）
onDeactivated(() => {
  log.debug('音频组件被停用')
  wasPlaying = audioStore.Audio.isPlay
  playbackPosition = audioStore.Audio.currentTime
})

// ---- 事件处理（带 slot 过滤） ----

const forward = (name: string, val?: any) => {
  log.debug('forward', name, val)
  window.dispatchEvent(new CustomEvent('global-music-control', { detail: { name, val } }))
}

const handleEnded = (slot: AudioSlot): void => {
  // 非活跃槽的 ended 事件忽略（过渡完成会由 crossfade 管理器推进）
  if (!isPrimarySlot(slot)) return
  // 若正在完成交叉淡化，ended 事件会由 crossfade 的 completeCrossfade 处理
  if (crossfadeState.active) return
  audioStore.Audio.isPlay = false
  audioStore.publish('ended')
  forward('autoNext')
}

const handleSeeked = (slot: AudioSlot): void => {
  if (!isPrimarySlot(slot)) return
  audioStore.publish('seeked')
}

const handlePlay = (slot: AudioSlot): void => {
  const el = slot === 'A' ? audioARef.value : audioBRef.value
  // 确保 AudioContext 处于运行状态
  if (el) {
    AudioManager.resumeContext(el)
  }

  // 非活跃槽在过渡期间也会 play，但不应更新 isPlay 状态（它本来就是 true）
  if (!isPrimarySlot(slot)) {
    return
  }

  audioStore.Audio.isPlay = true
  startSetupInterval()
  const activeEl = audioStore.Audio.audio
  audioStore.Audio.duration = activeEl?.duration || 0
  audioStore.publish('play')
}

let rafId: number | null = null
const startSetupInterval = (): void => {
  if (rafId !== null) return
  const onFrame = () => {
    const activeEl = audioStore.Audio.audio
    if (activeEl && !activeEl.paused) {
      audioStore.publish('timeupdate')
      audioStore.setCurrentTime(activeEl.currentTime || 0)
    }
    rafId = requestAnimationFrame(onFrame)
  }
  rafId = requestAnimationFrame(onFrame)
}

const handlePause = (slot: AudioSlot): void => {
  if (!isPrimarySlot(slot)) return
  audioStore.Audio.isPlay = false
  audioStore.publish('pause')
  if (rafId !== null) {
    try {
      cancelAnimationFrame(rafId)
    } catch {}
    rafId = null
  }
}

const handleError = (slot: AudioSlot, event: Event): void => {
  const target = event.target as HTMLAudioElement
  // 非活跃槽的错误（如清空 src 时的 MEDIA_ELEMENT_ERROR / Empty src）不进入错误流程
  // 这是 crossfade 结束清理时的正常情况
  if (!isPrimarySlot(slot)) return
  log.error(`音频加载错误 slot=${slot}:`, target.error)
  audioStore.Audio.isPlay = false
  audioStore.publish('error')
}

const handleLoadedData = (slot: AudioSlot): void => {
  const el = slot === 'A' ? audioARef.value : audioBRef.value
  if (!el) return
  if (!isPrimarySlot(slot)) return
  audioStore.setDuration(el.duration || 0)
  log.debug('音频数据加载完成 slot=', slot, 'duration:', el.duration)
}

const handleCanPlay = (slot: AudioSlot): void => {
  log.debug('音频可以开始播放 slot=', slot)
  if (!isPrimarySlot(slot)) return
  audioStore.publish('canplay')
}

onUnmounted(() => {
  try {
    // Tauri IPC bridge: 停止后台 ping 服务
    // 原始 Electron 版本使用 window.api.pingService.stop()
    // Tauri 版本通过 IPC bridge 调用
    const api = (window as any).api
    if (api?.pingService?.stop) {
      api.pingService.stop()
    }
  } catch {}
  if (rafId !== null) {
    try {
      cancelAnimationFrame(rafId)
    } catch {}
    rafId = null
  }
  for (const el of [audioARef.value, audioBRef.value]) {
    if (!el) continue
    try {
      el.pause()
    } catch {}
    try {
      el.removeAttribute('src')
      el.load()
    } catch {}
  }
  audioStore.clearAllSubscribers()
})
</script>

<template>
  <div>
    <audio
      id="globaAudio"
      ref="audioARef"
      preload="auto"
      :src="audioStore.Audio.srcA"
      @seeked="handleSeeked('A')"
      @play="handlePlay('A')"
      @pause="handlePause('A')"
      @error="handleError('A', $event)"
      @loadeddata="handleLoadedData('A')"
      @ended="handleEnded('A')"
      @canplay="handleCanPlay('A')"
    ></audio>
    <audio
      id="globaAudioB"
      ref="audioBRef"
      preload="auto"
      :src="audioStore.Audio.srcB"
      @seeked="handleSeeked('B')"
      @play="handlePlay('B')"
      @pause="handlePause('B')"
      @error="handleError('B', $event)"
      @loadeddata="handleLoadedData('B')"
      @ended="handleEnded('B')"
      @canplay="handleCanPlay('B')"
    ></audio>
  </div>
</template>
