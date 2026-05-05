<script lang="ts" setup>
import '@applemusic-like-lyrics/core/style.css'
import {
  BackgroundRender as CoreBackgroundRender,
  PixiRenderer
} from '@applemusic-like-lyrics/core'
import { LyricPlayer, type LyricPlayerRef } from '@applemusic-like-lyrics/vue'
import type { SongList } from '@/types/audio'
import { ref, computed, onMounted, watch, reactive, onBeforeUnmount, onUnmounted, nextTick } from 'vue'
import { useRouter } from 'vue-router'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { PerformanceDegrader } from '@/utils/performanceMonitor'
import { ControlAudioStore } from '@/store/ControlAudio'
import {
  Fullscreen1Icon,
  FullscreenExit1Icon,
  ChevronDownIcon,
  PenBallIcon
} from 'tdesign-icons-vue-next'
import debounce from 'lodash/debounce'
import { storeToRefs } from 'pinia'
import { useSettingsStore } from '@/store/Settings'
import { useGlobalPlayStatusStore } from '@/store/GlobalPlayStatus'
import { useDlnaStore } from '@/store/dlna'
import { invoke } from '@tauri-apps/api/core'
import { MessagePlugin } from 'tdesign-vue-next'
import { usePlaySettingStore } from '@/store'
import PlaySettings from './PlaySettings.vue'
import TitleBarControls from '@/components/TitleBarControls.vue'
import CommentsOverlay from './CommentsOverlay.vue'

const playSetting = usePlaySettingStore()
const settingsStore = useSettingsStore()
const dlnaStore = useDlnaStore()
const globalPlayStatus = useGlobalPlayStatusStore()
const { player } = storeToRefs(globalPlayStatus)
const showSettings = ref(false)
const router = useRouter()

function goToSinger() {
  const song = player.value.songInfo as any
  if (song?.singerId && song?.source && song.source !== 'local') {
    router.push({ name: 'singer', params: { id: song.singerId }, query: { source: song.source } })
  }
}

const lyricFontSize = computed(() => {
  const rate = settingsStore.settings.FullPlayLyricFontRate || 1.0
  return `calc(min(clamp(30px, 2.5vw, 50px), 5vh) * ${rate})`
})
const lyricFontWeight = computed(() => settingsStore.settings.lyricFontWeight || 600)
const lyricFontFamily = computed(
  () => settingsStore.settings.lyricFontFamily || 'lyricfont'
)

const safeLyricLines = computed(() => player.value.lyrics.lines || [])

const showLeftPanel = computed({
  get: () => playSetting.getShowLeftPanel,
  set: (val) => playSetting.setShowLeftPanel(val)
})

// 春节烟花
const festivalOverlay = ref<HTMLDivElement | null>(null)
let fwCanvas: HTMLCanvasElement | null = null
let fwCtx: CanvasRenderingContext2D | null = null
let rafId: number | null = null
let loopId: number | null = null
let bursts: any[] = []
let particles: any[] = []
let lastTime = 0
let running = false
const showFestivalEffects = computed(
  () =>
    settingsStore.shouldUseSpringFestivalTheme() && !settingsStore.settings.springFestivalDisabled
)

const rnd = (min: number, max: number) => Math.random() * (max - min) + min
const pick = (arr: any[]) => arr[Math.floor(Math.random() * arr.length)]
const colors = ['#ff3b3b', '#ffd65a', '#ff7a00', '#ff2d55', '#ffe08a', '#fa383e', '#ff9f0a']

const resizeCanvas = () => {
  if (!fwCanvas || !festivalOverlay.value) return
  const rect = festivalOverlay.value.getBoundingClientRect()
  fwCanvas.width = Math.floor(rect.width * window.devicePixelRatio)
  fwCanvas.height = Math.floor(rect.height * window.devicePixelRatio)
}

const addBurst = (x: number, y: number) => {
  const c = pick(colors)
  const count = Math.floor(rnd(40, 80))
  for (let i = 0; i < count; i++) {
    const angle = rnd(0, Math.PI * 2)
    const speed = rnd(2, 6)
    particles.push({ x, y, vx: Math.cos(angle) * speed, vy: Math.sin(angle) * speed, life: rnd(60, 120), color: c, alpha: 1, size: rnd(1, 2.8) })
  }
  if (particles.length > 500) particles.splice(0, particles.length - 500)
}

const scheduleBursts = (w: number, h: number) => {
  bursts.push({ t: 0, x: rnd(w * 0.15, w * 0.85), y: rnd(h * 0.15, h * 0.5) })
  bursts.push({ t: 400, x: rnd(w * 0.1, w * 0.9), y: rnd(h * 0.2, h * 0.6) })
  bursts.push({ t: 800, x: rnd(w * 0.2, w * 0.8), y: rnd(h * 0.15, h * 0.55) })
  bursts.push({ t: 1200, x: rnd(w * 0.25, w * 0.75), y: rnd(h * 0.1, h * 0.5) })
  bursts.push({ t: 1600, x: rnd(w * 0.2, w * 0.8), y: rnd(h * 0.15, h * 0.6) })
}

const step = (ts: number) => {
  if (!fwCtx || !fwCanvas) return
  const dt = ts - lastTime
  lastTime = ts
  fwCtx.globalCompositeOperation = 'source-over'
  fwCtx.fillStyle = 'rgba(0,0,0,0)'
  fwCtx.clearRect(0, 0, fwCanvas.width, fwCanvas.height)
  const g = 0.05
  const f = 0.985
  fwCtx.globalCompositeOperation = 'lighter'
  for (let i = particles.length - 1; i >= 0; i--) {
    const p = particles[i]
    p.vx *= f
    p.vy = p.vy * f + g
    p.x += p.vx
    p.y += p.vy
    p.life -= 1
    p.alpha = Math.max(0, p.life / 120)
    fwCtx.beginPath()
    fwCtx.fillStyle = p.color
    fwCtx.globalAlpha = p.alpha
    fwCtx.arc(p.x, p.y, p.size, 0, Math.PI * 2)
    fwCtx.fill()
    if (p.life <= 0) particles.splice(i, 1)
  }
  for (let i = bursts.length - 1; i >= 0; i--) {
    const b = bursts[i]
    b.t -= dt
    if (b.t <= 0) { addBurst(b.x, b.y); bursts.splice(i, 1) }
  }
  if (running) rafId = requestAnimationFrame(step)
}

const startFireworks = () => {
  if (running) return
  if (!festivalOverlay.value) return
  fwCanvas = document.createElement('canvas')
  fwCanvas.style.position = 'absolute'
  fwCanvas.style.top = '0'
  fwCanvas.style.left = '0'
  fwCanvas.style.width = '100%'
  fwCanvas.style.height = '100%'
  fwCanvas.style.zIndex = '0'
  fwCanvas.style.pointerEvents = 'none'
  festivalOverlay.value.appendChild(fwCanvas)
  fwCtx = fwCanvas.getContext('2d')
  resizeCanvas()
  particles = []
  bursts = []
  const w = fwCanvas.width
  const h = fwCanvas.height
  scheduleBursts(w, h)
  scheduleBursts(w, h)
  lastTime = performance.now()
  running = true
  rafId = requestAnimationFrame(step)
  loopId = window.setInterval(() => {
    if (!running || !fwCanvas) return
    scheduleBursts(fwCanvas.width, fwCanvas.height)
  }, 2200)
  window.setTimeout(() => stopFireworks(), 14000)
}

const stopFireworks = () => {
  running = false
  if (rafId) { cancelAnimationFrame(rafId); rafId = null }
  if (loopId) { clearInterval(loopId); loopId = null }
  particles = []
  bursts = []
  if (fwCanvas && festivalOverlay.value) festivalOverlay.value.removeChild(fwCanvas)
  fwCanvas = null
  fwCtx = null
}

onMounted(() => {
  window.addEventListener('resize', resizeCanvas)
  if (import.meta.env.DEV) console.debug('[DEBUG] FullPlay.vue mounted, lyrics lines:', player.value.lyrics.lines.length)
})
onBeforeUnmount(() => { window.removeEventListener('resize', resizeCanvas); stopFireworks() })

interface Props {
  show?: boolean
  showComments?: boolean
  coverImage?: string
  songId?: string | null
  songInfo: SongList | { songmid: number | null | string; lrc: string | null }
  mainColor: string
}

const props = withDefaults(defineProps<Props>(), {
  show: false,
  showComments: false,
  coverImage: '/src/assets/images/Default.jpg',
  songId: '',
  mainColor: '#rgb(0,0,0)'
})

const emit = defineEmits(['toggle-fullscreen', 'idle-change', 'update:showComments'])

// 全屏状态
const isFullscreen = ref(false)
const isAnimating = ref(false)
let animatingTimer: any = null

// 自动隐藏
const isIdle = ref(false)
const isHide = ref(false)
let idleTimer: any = null

const resetIdleTimer = () => {
  if (isHide.value) return
  if (!playSetting.getAutoHideBottom) {
    isIdle.value = false
    emit('idle-change', false)
    return
  }
  if (isIdle.value) {
    isIdle.value = false
    emit('idle-change', false)
  }
  if (idleTimer) clearTimeout(idleTimer)
  if (props.show) {
    idleTimer = setTimeout(() => {
      if (props.show && playSetting.getAutoHideBottom && !showSettings.value) {
        isIdle.value = true
        emit('idle-change', true)
      }
    }, 3000)
  }
}

watch(
  () => [props.show, showFestivalEffects.value],
  (vals) => {
    const visible = vals[0]
    const seasonal = vals[1]
    if (visible && seasonal && !running) startFireworks()
    if ((!visible || !seasonal) && running) stopFireworks()
  },
  { immediate: true }
)

const handleKeyDown = (e: KeyboardEvent) => {
  if (e.key === 'F1') {
    e.preventDefault()
    isHide.value = !isHide.value
    isIdle.value = isHide.value
    emit('idle-change', isHide.value)
  }
}

watch(
  () => props.show,
  (val) => {
    isAnimating.value = true
    if (animatingTimer) clearTimeout(animatingTimer)
    animatingTimer = setTimeout(() => { isAnimating.value = false }, 300)
    if (val) {
      resetIdleTimer()
      window.addEventListener('mousemove', resetIdleTimer)
    } else {
      window.removeEventListener('mousemove', resetIdleTimer)
      if (idleTimer) clearTimeout(idleTimer)
      isIdle.value = false
      emit('idle-change', false)
    }
  },
  { immediate: true }
)

watch(() => playSetting.getAutoHideBottom, (val) => {
  if (!val) {
    if (idleTimer) clearTimeout(idleTimer)
    isIdle.value = false
    emit('idle-change', false)
  } else { resetIdleTimer() }
})

watch(() => showSettings.value, (val) => {
  if (val) {
    if (idleTimer) clearTimeout(idleTimer)
    isIdle.value = false
    emit('idle-change', false)
  } else { resetIdleTimer() }
})

// 全屏切换
const toggleFullscreen = () => {
  isFullscreen.value = !isFullscreen.value
  window.api.toggleFullscreen()
}

onMounted(() => {
  document.addEventListener('fullscreenchange', handleFullscreenChange)
})

const handleFullscreenChange = () => {
  isFullscreen.value = !!document.fullscreenElement
}

// 音频控制
const controlAudio = ControlAudioStore()
const { Audio } = storeToRefs(controlAudio)
const isAudioPlaying = ref(false)

const updatePlayState = () => {
  isAudioPlaying.value = Audio.value.isPlay
}

watch(
  () => Audio.value.isPlay,
  (playing) => {
    isAudioPlaying.value = playing
  },
  { immediate: true }
)

// 内部状态
const state = reactive({
  audioUrl: Audio.value.url,
  albumUrl: props.coverImage,
  currentTime: 0,
  lowFreqVolume: 1.0
})

const bgRef = ref<CoreBackgroundRender<PixiRenderer> | undefined>(undefined)
const lyricPlayerRef = ref<LyricPlayerRef | undefined>(undefined)
const backgroundContainer = ref<HTMLDivElement | null>(null)

// 音频响应相关
let spectrumUnlisten: UnlistenFn | null = null
const backgroundConfig = computed(() => settingsStore.settings.backgroundRender?.fullPlay)
const audioResponseEnabled = computed(() => backgroundConfig.value?.audioResponse ?? true)

// 性能监控和自动降级
const performanceDegrader = new PerformanceDegrader()
const showPerformanceWarning = ref(false)
const hasAutoDegraded = ref(false)

const actualCoverImage = computed(() => {
  return player.value.cover || props.coverImage || '/src/assets/images/Default.jpg'
})

const jumpTime = (e: any) => {
  if (dlnaStore.currentDevice) {
    MessagePlugin.warning('投屏模式下不支持拖拽进度')
    return
  }
  const startTime = e?.line?.getLine?.()?.startTime ?? 0
  if (Audio.value.isPlay) invoke('player__seek', { position: startTime / 1000 })
}

// 封面变化 → 更新背景
watch(
  () => actualCoverImage.value,
  async (newImage) => {
    if (bgRef.value) {
      const renderer = bgRef.value as any
      const oldTexture = renderer.curContainer?.children?.[0]?.texture
      await bgRef.value.setAlbum(newImage, false)
      if (oldTexture) {
        setTimeout(() => {
          if (oldTexture.baseTexture && !oldTexture.baseTexture.destroyed) {
            try { oldTexture.destroy(true) } catch (e) { console.warn('Failed to clean up old album texture:', e) }
          }
        }, 2000)
      }
    }
  },
  { immediate: true }
)

// 电源阻止休眠
const blockerActive = ref(false)
watch(
  () => props.show,
  async (visible) => {
    try {
      if (visible && !blockerActive.value) {
        await (window as any)?.api?.powerSaveBlocker?.start?.()
        blockerActive.value = true
      } else if (!visible && blockerActive.value) {
        await (window as any)?.api?.powerSaveBlocker?.stop?.()
        blockerActive.value = false
      }
    } catch (e) {
      console.error('powerSaveBlocker 切换失败:', e)
    }
  },
  { immediate: true }
)

// 初始化背景渲染器
const initBackgroundRender = async () => {
  if (!backgroundContainer.value) {
    console.warn('[FullPlay] backgroundContainer 不存在，跳过背景初始化')
    return
  }

  if (!backgroundConfig.value?.enabled) {
    console.log('[FullPlay] 背景效果已禁用，跳过初始化')
    return
  }

  console.log('[FullPlay] 初始化背景渲染器，配置:', backgroundConfig.value)

  if (bgRef.value) {
    bgRef.value.dispose()
    const canvas = bgRef.value.getElement()
    canvas?.parentNode?.removeChild(canvas)
  }
  bgRef.value = CoreBackgroundRender.new(PixiRenderer)
  const canvas = bgRef.value.getElement()
  canvas.style.position = 'absolute'
  canvas.style.top = '0'
  canvas.style.left = '0'
  canvas.style.width = '100%'
  canvas.style.height = '100%'
  canvas.style.zIndex = '-1'
  backgroundContainer.value.appendChild(canvas)

  // 应用配置
  applyBackgroundConfig()
  console.log('[FullPlay] 背景配置已应用')

  bgRef.value.setHasLyric(player.value.lyrics.lines.length > 10)
  await bgRef.value.setAlbum(actualCoverImage.value, false)
  bgRef.value.resume()

  console.log('[FullPlay] 背景渲染器已启动')

  // 如果启用了音频响应，启动它
  if (audioResponseEnabled.value && Audio.value.isPlay) {
    console.log('[FullPlay] 启动音频响应')
    startAudioResponse()
  }
}

// 应用背景配置到渲染器
const applyBackgroundConfig = () => {
  if (!bgRef.value || !backgroundConfig.value) return

  console.log('[FullPlay] 应用背景配置:', {
    renderScale: backgroundConfig.value.renderScale,
    flowSpeed: backgroundConfig.value.flowSpeed,
    fps: backgroundConfig.value.fps,
    staticMode: backgroundConfig.value.staticMode,
    audioResponse: backgroundConfig.value.audioResponse
  })

  bgRef.value.setRenderScale(backgroundConfig.value.renderScale)
  bgRef.value.setFlowSpeed(backgroundConfig.value.flowSpeed)
  bgRef.value.setFPS(backgroundConfig.value.fps)
  bgRef.value.setStaticMode(backgroundConfig.value.staticMode)
}

// 启动音频响应效果
const startAudioResponse = async () => {
  if (!audioResponseEnabled.value || spectrumUnlisten) return

  try {
    spectrumUnlisten = await listen('player:spectrum', (event: any) => {
      if (!bgRef.value || !Audio.value.isPlay) return

      const { bands } = event.payload
      if (bands && Array.isArray(bands) && bands.length > 0) {
        // 提取低频能量（前 10 个频段）
        const lowFreqBands = bands.slice(0, 10)
        const avgLowFreq = lowFreqBands.reduce((sum: number, val: number) => sum + val, 0) / lowFreqBands.length

        // 转换为 0-1 范围（-80dB 到 0dB）
        const normalizedVolume = Math.max(0, Math.min(1, (avgLowFreq + 80) / 80))

        // 应用到背景渲染器
        bgRef.value.setLowFreqVolume(normalizedVolume)
      }
    })
  } catch (error) {
    console.error('[FullPlay] 启动音频响应失败:', error)
  }
}

// 停止音频响应效果
const stopAudioResponse = () => {
  if (spectrumUnlisten) {
    spectrumUnlisten()
    spectrumUnlisten = null
  }
  // 重置低频音量
  bgRef.value?.setLowFreqVolume(0)
}

onMounted(async () => {
  await initBackgroundRender()

  // 启动性能监控（仅在启用背景效果时）
  if (settingsStore.settings.backgroundRender?.fullPlay?.enabled) {
    performanceDegrader.start({
      onTick: (fps) => {
        // 可选：实时 FPS 显示（调试用）
        // console.log('[Performance] FPS:', fps.toFixed(1))
      },
      onDegrade: (degradedConfig) => {
        console.warn('[FullPlay] 检测到性能问题，自动降低背景效果')
        hasAutoDegraded.value = true
        showPerformanceWarning.value = true

        // 应用降级配置
        settingsStore.updateSettings({
          backgroundRender: {
            ...settingsStore.settings.backgroundRender!,
            fullPlay: {
              ...settingsStore.settings.backgroundRender!.fullPlay,
              ...degradedConfig
            }
          }
        })

        // 3秒后自动隐藏提示
        setTimeout(() => {
          showPerformanceWarning.value = false
        }, 5000)
      },
      enabled: true
    })
  }
})

onBeforeUnmount(async () => {
  document.removeEventListener('fullscreenchange', handleFullscreenChange)
  window.removeEventListener('mousemove', resetIdleTimer)
  window.removeEventListener('resize', debouncedCheckOverflow)
  document.removeEventListener('click', handleClickOutside)
  if (idleTimer) clearTimeout(idleTimer)
  if (animatingTimer) clearTimeout(animatingTimer)
  if (blockerActive.value) {
    try { await (window as any)?.api?.powerSaveBlocker?.stop?.() } catch {}
    blockerActive.value = false
  }
  if (Audio.value.isPlay) {
    // Rust 后端管理播放状态
  }
  // 停止音频响应
  stopAudioResponse()
  // 停止性能监控
  performanceDegrader.stop()
  if (bgRef.value) {
    const canvas = bgRef.value.getElement()
    canvas?.parentNode?.removeChild(canvas)
    bgRef.value.dispose()
    bgRef.value = undefined
  }
  lyricPlayerRef.value?.lyricPlayer?.dispose()
})

watch(() => Audio.value.url, (newUrl) => { state.audioUrl = newUrl })
watch(() => Audio.value.currentTime, (newTime) => { state.currentTime = Math.round(newTime * 1000) })

// 监听播放状态，自动启停音频响应
watch(
  () => Audio.value.isPlay,
  (isPlaying) => {
    if (audioResponseEnabled.value) {
      if (isPlaying) {
        startAudioResponse()
      } else {
        stopAudioResponse()
      }
    }
  }
)

// 监听音频响应配置变化
watch(
  () => audioResponseEnabled.value,
  (enabled) => {
    if (enabled) {
      if (Audio.value.isPlay) {
        startAudioResponse()
      }
    } else {
      stopAudioResponse()
    }
  }
)

// 监听歌词变化，更新 hasLyric 状态
watch(
  () => player.value.lyrics.lines,
  (lines) => {
    if (bgRef.value) {
      bgRef.value.setHasLyric(lines.length > 10)
    }
  }
)

// 监听背景配置变化，动态更新渲染器
watch(
  () => backgroundConfig.value,
  (newConfig) => {
    if (!newConfig) return

    // 如果启用状态改变
    if (newConfig.enabled && !bgRef.value) {
      // 从禁用到启用，重新初始化
      initBackgroundRender()
    } else if (!newConfig.enabled && bgRef.value) {
      // 从启用到禁用，清理
      bgRef.value.dispose()
      const canvas = bgRef.value.getElement()
      canvas?.parentNode?.removeChild(canvas)
      bgRef.value = undefined
      stopAudioResponse()
    } else if (newConfig.enabled && bgRef.value) {
      // 已启用，更新参数
      applyBackgroundConfig()

      // 音频响应开关变化
      if (newConfig.audioResponse && Audio.value.isPlay && !spectrumUnlisten) {
        startAudioResponse()
      } else if (!newConfig.audioResponse && spectrumUnlisten) {
        stopAudioResponse()
      }
    }
  },
  { deep: true }
)

const handleLowFreqUpdate = (volume: number) => { state.lowFreqVolume = volume }

const lightMainColor = computed(() => player.value.coverDetail.lightMainColor || 'rgba(255, 255, 255, 0.9)')
const useBlackText = computed(() => player.value.coverDetail.useBlackText)
const lyricViewColor = computed(() => playSetting.getIsImmersiveLyricColor ? lightMainColor.value : 'rgba(255, 255, 255, 1)')

// 滚动标题
const titleRef = ref<HTMLElement | null>(null)
const shouldScrollTitle = ref(false)
const titleContentRef = ref<HTMLElement | null>(null)

const songName = computed(() => {
  const info = player.value.songInfo
  if (info && 'name' in info && typeof info.name === 'string') return info.name
  return '未知歌曲'
})

const checkOverflow = async () => {
  await nextTick()
  if (titleRef.value && titleContentRef.value) {
    const containerWidth = titleRef.value.clientWidth
    const contentWidth = titleContentRef.value.offsetWidth
    shouldScrollTitle.value = contentWidth > containerWidth
  }
}

watch(() => [props.songInfo, props.show], checkOverflow, { immediate: true })

// 点击外部关闭设置面板
const floatActionRef = ref<HTMLElement | null>(null)
const handleClickOutside = (event: MouseEvent) => {
  if (showSettings.value && floatActionRef.value && !floatActionRef.value.contains(event.target as Node)) {
    showSettings.value = false
  }
}

// 后台暂停动画
const handleVisibilityChange = () => {
  if (document.hidden) { bgRef.value?.pause() } else { bgRef.value?.resume() }
}

const handleWindowFocus = () => {
  if (!document.hidden) bgRef.value?.resume()
}

const debouncedCheckOverflow = debounce(checkOverflow, 200)

onMounted(() => {
  window.addEventListener('resize', debouncedCheckOverflow)
  document.addEventListener('click', handleClickOutside)
  window.addEventListener('keydown', handleKeyDown)
  document.addEventListener('visibilitychange', handleVisibilityChange)
  window.addEventListener('focus', handleWindowFocus)
  setTimeout(checkOverflow, 500)
})

onUnmounted(() => {
  window.removeEventListener('resize', debouncedCheckOverflow)
  document.removeEventListener('click', handleClickOutside)
  window.removeEventListener('keydown', handleKeyDown)
  document.removeEventListener('visibilitychange', handleVisibilityChange)
  window.removeEventListener('focus', handleWindowFocus)
})
</script>

<template>
  <div
    class="full-play"
    :class="{
      active: props.show,
      'use-black-text': useBlackText,
      idle: isIdle,
      animating: isAnimating
    }"
  >
    <div
      ref="backgroundContainer"
      style="position: absolute; top: 0; left: 0; width: 100%; height: 100%; z-index: -1"
    ></div>
    <div v-if="showFestivalEffects" ref="festivalOverlay" class="festival-overlay"></div>

    <!-- 性能警告提示 -->
    <Transition name="fade">
      <div v-if="showPerformanceWarning" class="performance-warning-toast">
        <t-icon name="info-circle" size="1.2rem" />
        <span>检测到性能问题，已自动降低背景效果以提升流畅度</span>
        <button class="close-btn" @click="showPerformanceWarning = false">
          <t-icon name="close" size="1rem" />
        </button>
      </div>
    </Transition>
    <!-- 全屏按钮 -->
    <button
      class="fullscreen-btn"
      :class="{ 'black-text': useBlackText }"
      @click="toggleFullscreen"
    >
      <Fullscreen1Icon v-if="!isFullscreen" class="icon" />
      <FullscreenExit1Icon v-else class="icon" />
    </button>
    <button
      class="putawayscreen-btn"
      :class="{ 'black-text': useBlackText }"
      @click="emit('toggle-fullscreen')"
    >
      <ChevronDownIcon class="icon" />
    </button>
    <Transition name="fade-nav">
      <TitleBarControls
        v-if="props.show"
        class="top"
        data-tauri-drag-region
        :color="useBlackText ? 'black' : 'white'"
        :show-account="false"
      />
    </Transition>
    <div
      class="playbox"
      :style="{
        padding:
          playSetting.getLayoutMode === 'cover' || !playSetting.getShowLeftPanel
            ? '0 min(4.5vw, 100px)'
            : '0 10vw'
      }"
      :class="{
        'mode-cover': playSetting.getLayoutMode === 'cover',
        'single-column': !showLeftPanel
      }"
    >
      <div
        class="left"
      >
        <!-- 黑胶模式 -->
        <template v-if="playSetting.getLayoutMode === 'cd'">
          <img
            class="pointer"
            :class="{ playing: isAudioPlaying }"
            src="@/assets/pointer.png"
            alt="pointer"
          />
          <div
            class="cd-container"
            :class="{ playing: isAudioPlaying }"
            :style="
              !isAudioPlaying
                ? 'animation-play-state: paused;'
                : ''
            "
          >
            <div class="vinyl-record"></div>
            <div class="vinyl-label">
              <img :src="coverImage" alt="cover" class="cover" />
              <div class="label-shine"></div>
            </div>
            <div class="center-hole"></div>
          </div>
        </template>

        <!-- 封面模式 -->
        <template v-else-if="playSetting.getLayoutMode === 'cover'">
          <div class="cover-layout-container">
            <div class="cover-wrapper-square" :class="{ playing: controlAudio.Audio.isPlay }">
              <img :src="actualCoverImage" class="cover-img-square" alt="cover" />
            </div>
            <div class="song-info-area">
              <div ref="titleRef" class="song-title-large text-scroll-container">
                <div class="text-scroll-wrapper" :class="{ 'animate-scroll': shouldScrollTitle }">
                  <div ref="titleContentRef" class="text-scroll-item">
                    {{ songName }}
                  </div>
                  <div v-if="shouldScrollTitle" class="text-scroll-item">
                    {{ songName }}
                  </div>
                </div>
              </div>
              <div class="song-meta-large">
                <span
                  class="artist"
                  :class="{ 'singer-link': (player.songInfo as any)?.singerId && (player.songInfo as any)?.source !== 'local' }"
                  @click="goToSinger()"
                >{{ (player.songInfo as any)?.singer }}</span>
                <span
                  v-if="(player.songInfo as any)?.singer && (player.songInfo as any)?.albumName"
                  class="divider"
                >
                  /
                </span>
                <span class="album">{{ (player.songInfo as any)?.albumName }}</span>
              </div>
            </div>
          </div>
        </template>
      </div>
      <div class="right">
        <div v-if="player.lyrics.lines.length <= 0 && !player.isLoading" class="lyric-empty">
          <span>暂无歌词</span>
        </div>
        <LyricPlayer
          v-if="player.lyrics.lines.length > 0"
          ref="lyricPlayerRef"
          :lyric-lines="safeLyricLines"
          :current-time="state.currentTime"
          :word-fade-width="0.5"
          :playing="isAudioPlaying"
          class="lyric-player"
          :align-position="
            playSetting.getLayoutMode === 'cd' && playSetting.getShowLeftPanel ? 0.5 : 0.34
          "
          :enable-blur="playSetting.getIsBlurLyric"
          :enable-spring="playSetting.getisJumpLyric"
          :enable-scale="playSetting.getisJumpLyric"
          :text-align="!playSetting.getShowLeftPanel ? 'center' : 'left'"
          :style="playSetting.getShowLeftPanel ? '' : 'text-align: center;'"
          @line-click="jumpTime"
        />
      </div>
    </div>
    <!-- 音频可视化 -->
    <div
      v-if="props.show && coverImage && playSetting.getIsAudioVisualizer"
      class="audio-visualizer-container"
      :class="{ idle: isIdle }"
    >
      <AudioVisualizer
        :show="Audio.isPlay"
        :height="70"
        :bar-count="80"
        :color="mainColor"
        @low-freq-update="handleLowFreqUpdate"
      />
    </div>
    <!-- 播放设置浮动按钮 -->
    <div ref="floatActionRef" class="float-action" :class="{ idle: isIdle }">
      <button class="skin-btn" data-tooltip="播放器主题" @click="showSettings = !showSettings">
        <pen-ball-icon
          :fill-color="'transparent'"
          :stroke-color="'currentColor'"
          :stroke-width="2"
          :style="{ fontSize: '20px' }"
        />
      </button>
      <Transition name="fade-up">
        <div v-if="showSettings" class="settings-panel">
          <PlaySettings />
        </div>
      </Transition>
    </div>
    <!-- 评论弹窗 -->
    <CommentsOverlay
      :show="props.showComments"
      :main-color="lightMainColor"
      @close="emit('update:showComments', false)"
    />
  </div>
</template>

<style lang="scss" scoped>
.festival-overlay {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  z-index: 0;
  pointer-events: none;
}

.performance-warning-toast {
  position: absolute;
  top: 80px;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.875rem 1.25rem;
  background: rgba(255, 152, 0, 0.95);
  border-radius: 0.75rem;
  color: #fff;
  font-size: 0.9rem;
  font-weight: 500;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  z-index: 1000;
  pointer-events: auto;
  max-width: 600px;

  .close-btn {
    margin-left: auto;
    padding: 0.25rem;
    background: rgba(255, 255, 255, 0.2);
    border: none;
    border-radius: 0.25rem;
    color: #fff;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.2s;

    &:hover {
      background: rgba(255, 255, 255, 0.3);
    }
  }
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.fade-nav-enter-active,
.fade-nav-leave-active {
  transition: all 0.6s cubic-bezier(0.8, 0, 0.8, 0.43);
}
.fade-nav-enter-from,
.fade-nav-leave-to {
  opacity: 0;
}

.fullscreen-btn,
.putawayscreen-btn {
  position: absolute;
  top: 25px;
  left: 30px;
  padding: 10px;
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.1);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  border: none;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  z-index: 10;
  color: white;
  font-size: 18px;
  transition: background-color 0.3s ease, transform 0.3s ease;

  &:hover {
    background: rgba(255, 255, 255, 0.3);
    transform: scale(1.05);
  }
  &:active {
    transform: scale(0.95);
  }
  .icon {
    color: rgba(255, 255, 255, 0.6);
    width: 25px;
    height: 25px;
  }
  &.black-text {
    background: rgba(0, 0, 0, 0.1);
    .icon { color: rgba(0, 0, 0, 0.6); }
    &:hover { background: rgba(0, 0, 0, 0.2); }
  }
}

.putawayscreen-btn { left: 90px; }

.full-play {
  --height: calc(100vh - var(--play-bottom-height));
  --text-color: rgba(255, 255, 255, 0.9);
  z-index: 120;
  position: fixed;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  color: var(--text-color);
  transform: translateY(var(--height));
  will-change: transform;

  &.animating {
    transition: transform 0.3s cubic-bezier(0.8, 0, 0.8, 0.43);
  }
  &.use-black-text {
    --text-color: rgba(255, 255, 255, 0.9);
  }
  &.active { transform: translateY(0); }

  &.idle {
    .playbox {
      cursor: none;
      .left, .right { margin-bottom: 0; }
      .right {
        :deep(.lyric-player) { height: 100vh; }
      }
    }
    .fullscreen-btn, .putawayscreen-btn, .top {
      opacity: 0;
      pointer-events: none;
      transform: translateY(-100%);
    }
  }

  .top {
    position: absolute;
    width: calc(100% - 200px);
    z-index: 1;
    right: 0;
    padding: 30px 30px;
    padding-bottom: 10px;
    transition: opacity 0.5s cubic-bezier(0.34, 1.56, 0.64, 1), transform 0.5s cubic-bezier(0.34, 1.56, 0.64, 1);
  }

  .playbox {
    width: 100%;
    height: 100%;
    background-color: rgba(0, 0, 0, 0.256);
    -webkit-drop-filter: blur(80px);
    padding: 0 10vw;
    overflow: hidden;
    display: flex;
    position: relative;
    --cd-width-auto: max(200px, min(30vw, 700px, calc(100vh - var(--play-bottom-height) - 250px)));

    .left {
      width: 40%;
      transition: width 0.5s cubic-bezier(0.34, 1.56, 0.64, 1), opacity 0.5s cubic-bezier(0.34, 1.56, 0.64, 1), transform 0.5s cubic-bezier(0.34, 1.56, 0.64, 1);
      opacity: 1;
      transform: translateX(0);
      display: flex;
      justify-content: center;
      align-items: center;
      margin: 0 0 var(--play-bottom-height) 0;
      perspective: 1000px;

      .pointer {
        user-select: none;
        -webkit-user-drag: none;
        position: absolute;
        width: calc(var(--cd-width-auto) / 3.5);
        left: calc(50% - 1.8vh);
        top: calc(50% - var(--cd-width-auto) / 2 - calc(var(--cd-width-auto) / 3.5) - 1vh);
        transform: rotate(-20deg);
        transform-origin: 1.8vh 1.8vh;
        z-index: 2;
        transition: transform 0.3s;
        &.playing { transform: rotate(0deg); }
      }

      .cd-container {
        width: var(--cd-width-auto);
        height: var(--cd-width-auto);
        position: relative;
        display: flex;
        align-items: center;
        justify-content: center;
        animation: rotateRecord 33s linear infinite;
        transition: filter 0.3s ease;
        filter: drop-shadow(0 15px 35px rgba(0, 0, 0, 0.6));
        &:hover { filter: drop-shadow(0 20px 45px rgba(0, 0, 0, 0.7)); }

        .vinyl-record {
          aspect-ratio: 1/1;
          width: 100%;
          height: 100%;
          position: relative;
          border-radius: 50%;
          background: radial-gradient(circle at 50% 50%, #1a1a1a 0%, #0d0d0d 100%);
          display: flex;
          align-items: center;
          justify-content: center;
          overflow: hidden;

          &::before {
            content: '';
            position: absolute;
            top: 0; left: 0;
            width: 100%; height: 100%;
            border-radius: 50%;
            background:
              repeating-conic-gradient(from 0deg, transparent 0deg, rgba(255,255,255,0.02) 0.5deg, transparent 1deg, rgba(255,255,255,0.01) 1.5deg, transparent 2deg),
              repeating-radial-gradient(circle at 50% 50%, transparent 0px, rgba(255,255,255,0.03) 1px, transparent 2px, transparent 8px);
            z-index: 1;
          }
          &::after {
            content: '';
            position: absolute;
            top: 0; left: 0;
            width: 100%; height: 100%;
            background: radial-gradient(ellipse at 30% 30%, rgba(255,255,255,0.08) 0%, rgba(255,255,255,0.04) 25%, rgba(255,255,255,0.02) 50%, rgba(255,255,255,0.01) 75%, transparent 100%);
            border-radius: 50%;
            z-index: 2;
            animation: vinylShine 6s ease-in-out infinite;
          }
        }

        .vinyl-label {
          position: absolute;
          width: 60%;
          height: 60%;
          background: radial-gradient(circle at 50% 50%, rgba(40,40,40,0.95) 0%, rgba(25,25,25,0.98) 70%, rgba(15,15,15,1) 100%);
          border-radius: 50%;
          display: flex;
          align-items: center;
          justify-content: center;
          z-index: 3;
          box-shadow: inset 0 0 20px rgba(0,0,0,0.8), inset 0 0 0 1px rgba(255,255,255,0.05), 0 0 10px rgba(0,0,0,0.5);

          .cover {
            position: relative;
            z-index: 4;
            border-radius: 50%;
            overflow: hidden;
            box-shadow: 0 0 20px rgba(0,0,0,0.4), inset 0 0 0 2px rgba(255,255,255,0.1);
            width: 95% !important;
            height: 95% !important;
            aspect-ratio: 1 / 1;
            object-fit: cover;
            filter: brightness(0.85) contrast(1.15) saturate(1.1);
          }

          .label-shine {
            position: absolute;
            top: 0; left: 0;
            width: 100%; height: 100%;
            background: radial-gradient(ellipse at 25% 25%, rgba(255,255,255,0.1) 0%, transparent 50%);
            border-radius: 50%;
            z-index: 5;
            pointer-events: none;
            animation: labelShine 8s ease-in-out infinite;
          }
        }

        .center-hole {
          position: absolute;
          width: 8%;
          height: 8%;
          background: radial-gradient(circle, #000 0%, #111 30%, #222 70%, #333 100%);
          border-radius: 50%;
          z-index: 10;
          box-shadow: inset 0 0 8px rgba(0,0,0,0.9), 0 0 3px rgba(0,0,0,0.8);
        }
      }
    }

    .right {
      width: 60%;
      transition: width 0.5s cubic-bezier(0.34, 1.56, 0.64, 1);
      mask: linear-gradient(rgba(255,255,255,0) 0px, rgba(255,255,255,0.6) 5%, rgb(255,255,255) 15%, rgb(255,255,255) 75%, rgba(255,255,255,0.6) 85%, rgba(255,255,255,0));

      .lyric-empty {
        height: calc(100vh - var(--play-bottom-height));
        display: flex;
        align-items: center;
        justify-content: center;
        transform: translateY(-80px);
        span {
          font-size: 16px;
          opacity: 0.4;
          font-weight: 400;
        }
      }

      :deep(.lyric-player) {
        height: calc(100vh - var(--play-bottom-height));
        transform: translateY(-80px);
        --amll-lyric-view-color: v-bind(lyricViewColor);
        --amll-lp-color: v-bind(lyricViewColor);
        --amll-lyric-player-font-size: v-bind(lyricFontSize);
        --amll-lp-font-size: v-bind(lyricFontSize);
        --amll-lyric-player-font-weight: v-bind(lyricFontWeight);
        --amll-lp-bg-line-scale: 0.7;
        font-family: v-bind(lyricFontFamily);

        [class*='romanWord'] {
          font-size: calc(var(--amll-lp-font-size) * 0.5);
          font-family: v-bind(lyricFontFamily) !important;
          opacity: 0.8;
        }
        [class*='lyricSubLine'] {
          font-weight: v-bind(lyricFontWeight) !important;
        }
        [class^='_interludeDots'] {
          padding: auto;
          height: calc(var(--amll-lyric-player-font-size) + 1em);
          justify-content: center;
          align-items: center;
        }
      }

      padding: 0 20px;
      margin: 80px 0 calc(var(--play-bottom-height)) 0;
      overflow: hidden;
    }

    &.mode-cover {
      .left {
        width: 35%;
        padding: 0 3vw;
        display: flex;
        flex-direction: column;
        justify-content: center;
        align-items: flex-start;
      }
      .right { padding-left: 3vw; width: 65%; }
    }

    &.single-column {
      .left {
        width: 0 !important;
        padding: 0 !important;
        margin: 0 !important;
        opacity: 0;
        transform: translateX(-100px);
        pointer-events: none;
      }
      .right {
        width: 100%;
        padding: 0 10vw;
        display: flex;
        justify-content: center;
        :deep(.lyric-player) {
          width: 100%;
          max-width: 1000px;
          margin: 0 auto;
        }
      }
    }

    .cover-layout-container {
      width: 100%;
      display: flex;
      flex-direction: column;
      gap: 40px;
      margin-top: calc(var(--play-bottom-height) / 2);
      max-height: calc(100vh - 200px);

      .cover-wrapper-square {
        width: 100%;
        max-width: min(480px, 45vh);
        aspect-ratio: 1/1;
        border-radius: 24px;
        overflow: hidden;
        box-shadow: 0 25px 50px -12px rgba(0,0,0,0.5), 0 0 0 1px rgba(255,255,255,0.1);
        transition: transform 0.44s cubic-bezier(0.44, 2, 0.64, 1);
        margin: 0 auto;
        transform: scale(0.8);

        &.playing {
          transform: scale(1);
          &:hover { transition: transform 0.2s; transform: scale(1.02); }
        }
        &:hover { transform: scale(0.82); }

        .cover-img-square {
          width: 100%;
          height: 100%;
          object-fit: cover;
          user-select: none;
        }
      }

      .song-info-area {
        width: 100%;
        display: flex;
        flex-direction: column;
        gap: 12px;

        .song-title-large {
          font-size: min(3vw, 42px);
          font-weight: 800;
          color: rgba(255,255,255,0.95);
          line-height: 1.2;
          letter-spacing: -0.5px;
          text-shadow: 0 2px 10px rgba(0,0,0,0.3);

          &.text-scroll-container {
            overflow: hidden;
            white-space: nowrap;
            position: relative;
            width: 100%;
          }
          .text-scroll-wrapper {
            display: inline-flex;
            &.animate-scroll { animation: scroll 15s linear infinite; }
          }
          .text-scroll-item {
            font-weight: 800;
            flex-shrink: 0;
            padding-right: 2rem;
            display: inline-block;
          }
        }

        .song-meta-large {
          font-size: min(1.5vw, 20px);
          color: rgba(255,255,255,0.6);
          font-weight: 600;
          display: flex;
          align-items: center;
          gap: 8px;
          flex-wrap: wrap;
          opacity: 0.55;
          .artist { color: v-bind(lightMainColor); filter: brightness(1.2); }
          .artist.singer-link { cursor: pointer; &:hover { filter: brightness(1.5); } }
          .divider { opacity: 0.4; }
        }
      }
    }
  }

  @keyframes scroll {
    0% { transform: translateX(0); }
    25% { transform: translateX(0); }
    100% { transform: translateX(-50%); }
  }

  .audio-visualizer-container {
    position: absolute;
    bottom: calc(var(--play-bottom-height) - 10px);
    z-index: 5;
    left: 0;
    right: 0;
    height: 60px;
    filter: blur(6px);
    display: flex;
    align-items: center;
    pointer-events: none;
    transition: bottom 0.5s cubic-bezier(0.34, 1.56, 0.64, 1);
    &.idle { bottom: 0 !important; }
  }
}

.float-action {
  position: absolute;
  z-index: 5;
  transition: opacity 0.5s ease, transform 0.5s ease;
  &.idle {
    opacity: 0;
    transform: translateY(20px);
    pointer-events: none;
  }
  --bottom-height: 60px;
  right: 20px;
  bottom: calc(var(--bottom-height) + var(--play-bottom-height));

  .skin-btn {
    position: relative;
    backdrop-filter: blur(20px);
    background: rgba(255,255,255,0.15);
    border: 1px solid rgba(255,255,255,0.628);
    height: 50px;
    width: 50px;
    border-radius: 50%;
    &[data-tooltip]::after {
      content: attr(data-tooltip);
      position: absolute;
      bottom: -30px;
      left: 50%;
      transform: translateX(-50%);
      background: rgba(0,0,0,0.75);
      color: #fff;
      font-size: 12px;
      padding: 4px 8px;
      border-radius: 4px;
      white-space: nowrap;
      pointer-events: none;
      opacity: 0;
      transition: opacity 0.2s;
    }
    &:hover[data-tooltip]::after {
      opacity: 1;
    }
    cursor: pointer;
    transition: all 0.4s cubic-bezier(0.34, 1.56, 0.64, 1);
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255,255,255,0.9);
    font-size: 14px;
    font-weight: 500;
    letter-spacing: 1px;

    &:hover {
      background-color: rgba(255,255,255,0.25);
      box-shadow: 0 12px 40px 0 rgba(0,0,0,0.15), 0 0 30px v-bind(lightMainColor), inset 0 0 0 1px rgba(255,255,255,0.4);
    }
    &:active {
      transform: translateY(1px) scale(0.92);
      box-shadow: 0 4px 10px 0 rgba(0,0,0,0.1), 0 0 10px v-bind(lightMainColor), inset 0 0 0 1px rgba(255,255,255,0.1);
      transition: all 0.1s ease;
    }
  }

  .settings-panel {
    max-height: calc(100vh - 40px - 2.25rem - 70px - calc(var(--bottom-height) + var(--play-bottom-height)));
    display: flex;
    flex-direction: column;
    position: absolute;
    bottom: 70px;
    right: 0;
    width: 340px;
    background: rgb(30 30 30 / 29%);
    backdrop-filter: blur(6px);
    -webkit-backdrop-filter: blur(6px);
    border-radius: 24px;
    padding: 20px;
    box-shadow: 0 20px 50px rgba(0,0,0,0.4), 0 0 0 1px rgba(255,255,255,0.1);
    transform-origin: bottom right;
    z-index: 100;
  }
}

.fade-up-enter-active, .fade-up-leave-active {
  transition: opacity 0.3s cubic-bezier(0.34, 1.56, 0.64, 1), transform 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
}
.fade-up-enter-from, .fade-up-leave-to {
  opacity: 0;
  transform: translateY(20px) scale(0.95);
}

@keyframes rotateRecord {
  0% { transform: rotate(0deg); }
  100% { transform: rotate(360deg); }
}

@keyframes vinylShine {
  0% { opacity: 0.1; transform: rotate(0deg) scale(1); }
  50% { opacity: 0.2; transform: rotate(180deg) scale(1.1); }
  100% { opacity: 0.1; transform: rotate(360deg) scale(1); }
}

@keyframes labelShine {
  0% { opacity: 0.05; transform: rotate(0deg); }
  25% { opacity: 0.15; }
  50% { opacity: 0.1; transform: rotate(180deg); }
  75% { opacity: 0.15; }
  100% { opacity: 0.05; transform: rotate(360deg); }
}
</style>

<style lang="scss">
.full-play {
  --user-lyric-fw: v-bind(lyricFontWeight) !important;
  .lyric-player {
    [class*='Line' i] {
      &, & *, span, div { font-weight: var(--user-lyric-fw) !important; }
    }
    [class*='emphasize' i] { font-weight: var(--user-lyric-fw) !important; }
    [class*='romanWord' i] {
      font-size: 0.5em !important;
      font-weight: var(--user-lyric-fw) !important;
      line-height: 1 !important;
    }
  }
}
</style>
