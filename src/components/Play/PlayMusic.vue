<script setup lang="ts">
import {
  ref,
  computed,
  onMounted,
  onUnmounted,
  watch,
  nextTick,
  onActivated,
  onDeactivated
} from 'vue'
import { ControlAudioStore } from '@/store/ControlAudio'
import { LocalUserDetailStore } from '@/store/LocalUserDetail'
import { useGlobalPlayStatusStore } from '@/store/GlobalPlayStatus'
import { useSettingsStore } from '@/store/Settings'
import { storeToRefs } from 'pinia'
import { PlayMode } from '@/types/audio'
import {
  playNext,
  playPrevious,
  updatePlayMode,
  togglePlayPause,
  isLoadingSong,
  setVolume,
  seekTo,
  playSong,
  playMode
} from '@/utils/audio/globaPlayList'
import { crossfadeState } from '@/utils/audio/crossfade'

const controlAudio = ControlAudioStore()
const localUserStore = LocalUserDetailStore()
const globalPlayStatus = useGlobalPlayStatusStore()
const settingsStore = useSettingsStore()
const { Audio } = storeToRefs(controlAudio)
const { list, userInfo } = storeToRefs(localUserStore)
const { player } = storeToRefs(globalPlayStatus)
const songInfo = computed(() => player.value.songInfo || ({} as any))

// 播放模式
const playModeTip = ref('')
const playModeIconClass = computed(() => {
  switch (playMode.value) {
    case PlayMode.SEQUENCE:
      playModeTip.value = '顺序播放'
      return 'iconfont icon-shunxubofangtubiao'
    case PlayMode.RANDOM:
      playModeTip.value = '随机播放'
      return 'iconfont icon-suijibofang'
    case PlayMode.SINGLE:
      playModeTip.value = '单曲循环'
      return 'iconfont icon-bofang-xunhuanbofang'
    default:
      return 'iconfont icon-shunxubofangtubiao'
  }
})

// 音量控制
const showVolumeSlider = ref(false)
const volumeBarRef = ref<HTMLDivElement | null>(null)
const isDraggingVolume = ref(false)

const volumeValue = computed({
  get: () => Audio.value.volume,
  set: (val) => setVolume(val)
})

const handleVolumeClick = (event: MouseEvent) => {
  if (!volumeBarRef.value) return
  const rect = volumeBarRef.value.getBoundingClientRect()
  const offsetY = rect.bottom - event.clientY
  const percentage = Math.max(0, Math.min(100, (offsetY / rect.height) * 100))
  volumeValue.value = Math.round(percentage)
}

const handleVolumeDragStart = (event: MouseEvent) => {
  event.preventDefault()
  isDraggingVolume.value = true
  window.addEventListener('mousemove', handleVolumeDragMove)
  window.addEventListener('mouseup', handleVolumeDragEnd)
}

const handleVolumeDragMove = (event: MouseEvent) => {
  if (!isDraggingVolume.value || !volumeBarRef.value) return
  const rect = volumeBarRef.value.getBoundingClientRect()
  const offsetY = rect.bottom - event.clientY
  const percentage = Math.max(0, Math.min(100, (offsetY / rect.height) * 100))
  volumeValue.value = Math.round(percentage)
}

const handleVolumeDragEnd = () => {
  isDraggingVolume.value = false
  window.removeEventListener('mousemove', handleVolumeDragMove)
  window.removeEventListener('mouseup', handleVolumeDragEnd)
}

const handleVolumeWheel = (event: WheelEvent) => {
  event.preventDefault()
  const volumeStep = event.deltaY > 0 ? -5 : 5
  const updatedVolume = Math.max(0, Math.min(100, volumeValue.value + volumeStep))
  if (updatedVolume === volumeValue.value) return
  volumeValue.value = updatedVolume
}

// 播放列表
const showPlaylist = ref(false)
const currentSongId = computed(() => userInfo.value.lastPlaySongId)

const togglePlaylist = (e: MouseEvent) => {
  e.stopPropagation()
  showPlaylist.value = !showPlaylist.value
}

const closePlaylist = () => {
  showPlaylist.value = false
}

// 进度条
const progressRef = ref<HTMLDivElement | null>(null)
const isDraggingProgress = ref(false)
const tempProgressPercentage = ref(0)

const progressPercentage = computed(() => {
  if (isDraggingProgress.value) return tempProgressPercentage.value
  if (Audio.value.duration === 0) return 0
  return (Audio.value.currentTime / Audio.value.duration) * 100
})

// Crossfade marks
const crossfadeMarkVisible = computed(() => {
  return crossfadeState.markEnd > crossfadeState.markStart && Audio.value.duration > 0
})
const crossfadeMarkLeft = computed(() => {
  if (!crossfadeMarkVisible.value) return 0
  return (crossfadeState.markStart / Audio.value.duration) * 100
})
const crossfadeMarkWidth = computed(() => {
  if (!crossfadeMarkVisible.value) return 0
  return ((crossfadeState.markEnd - crossfadeState.markStart) / Audio.value.duration) * 100
})

const formatTime = (seconds: number) => {
  if (!seconds || !isFinite(seconds)) return '0:00'
  const mins = Math.floor(seconds / 60)
  const secs = Math.floor(seconds % 60)
  return `${mins}:${secs.toString().padStart(2, '0')}`
}

const currentTimeFormatted = computed(() => formatTime(Audio.value.currentTime))
const durationFormatted = computed(() => formatTime(Audio.value.duration))

const handleProgressClick = (event: MouseEvent) => {
  if (!progressRef.value) return
  const rect = progressRef.value.getBoundingClientRect()
  const offsetX = event.clientX - rect.left
  const percentage = (offsetX / rect.width) * 100
  tempProgressPercentage.value = percentage
  const newTime = (percentage / 100) * Audio.value.duration
  seekTo(newTime)
}

const handleProgressDragStart = (event: MouseEvent) => {
  event.preventDefault()
  document.querySelector('.progress-handle')?.classList.add('dragging')
  isDraggingProgress.value = true
  window.addEventListener('mousemove', handleProgressDragMove)
  window.addEventListener('mouseup', handleProgressDragEnd)
}

const handleProgressDragMove = (event: MouseEvent) => {
  if (!isDraggingProgress.value || !progressRef.value) return
  const rect = progressRef.value.getBoundingClientRect()
  const offsetX = Math.max(0, Math.min(event.clientX - rect.left, rect.width))
  const percentage = (offsetX / rect.width) * 100
  tempProgressPercentage.value = percentage
}

const handleProgressDragEnd = (event: MouseEvent) => {
  document.querySelector('.progress-handle')?.classList.remove('dragging')
  if (!isDraggingProgress.value || !progressRef.value) {
    isDraggingProgress.value = false
    window.removeEventListener('mousemove', handleProgressDragMove)
    window.removeEventListener('mouseup', handleProgressDragEnd)
    return
  }
  const rect = progressRef.value.getBoundingClientRect()
  const offsetX = Math.max(0, Math.min(event.clientX - rect.left, rect.width))
  const percentage = (offsetX / rect.width) * 100
  const newTime = (percentage / 100) * Audio.value.duration
  seekTo(newTime)
  isDraggingProgress.value = false
  window.removeEventListener('mousemove', handleProgressDragMove)
  window.removeEventListener('mouseup', handleProgressDragEnd)
}

// 春节烟花效果
const { settings } = storeToRefs(settingsStore)
const showFestivalEffects = computed(
  () => settingsStore.shouldUseSpringFestivalTheme() && !settings.value.springFestivalDisabled
)
const festivalOverlay = ref<HTMLDivElement | null>(null)
let fwCanvas: HTMLCanvasElement | null = null
let fwCtx: CanvasRenderingContext2D | null = null
let fwRafId: number | null = null
let fwLoopId: number | null = null
let fwBursts: any[] = []
let fwParticles: any[] = []
let fwLastTime = 0
let fwRunning = false

const fwRnd = (min: number, max: number) => Math.random() * (max - min) + min
const fwPick = (arr: string[]) => arr[Math.floor(Math.random() * arr.length)]
const fwColors = ['#ff3b3b', '#ffd65a', '#ff7a00', '#ff2d55', '#ffe08a', '#fa383e', '#ff9f0a']

const resizeFwCanvas = () => {
  if (!fwCanvas || !festivalOverlay.value) return
  const rect = festivalOverlay.value.getBoundingClientRect()
  fwCanvas.width = Math.floor(rect.width * window.devicePixelRatio)
  fwCanvas.height = Math.floor(rect.height * window.devicePixelRatio)
}

const addFwBurst = (x: number, y: number) => {
  const c = fwPick(fwColors)
  const count = Math.floor(fwRnd(40, 80))
  for (let i = 0; i < count; i++) {
    const angle = fwRnd(0, Math.PI * 2)
    const speed = fwRnd(2, 6)
    fwParticles.push({
      x, y,
      vx: Math.cos(angle) * speed,
      vy: Math.sin(angle) * speed,
      life: fwRnd(60, 120),
      color: c,
      alpha: 1,
      size: fwRnd(1, 2.8)
    })
  }
  if (fwParticles.length > 2000) fwParticles.splice(0, fwParticles.length - 2000)
}

const scheduleFwBursts = (w: number, h: number) => {
  fwBursts.push({ t: 0, x: fwRnd(w * 0.15, w * 0.85), y: fwRnd(h * 0.15, h * 0.5) })
  fwBursts.push({ t: 400, x: fwRnd(w * 0.1, w * 0.9), y: fwRnd(h * 0.2, h * 0.6) })
  fwBursts.push({ t: 800, x: fwRnd(w * 0.2, w * 0.8), y: fwRnd(h * 0.15, h * 0.55) })
  fwBursts.push({ t: 1200, x: fwRnd(w * 0.25, w * 0.75), y: fwRnd(h * 0.1, h * 0.5) })
  fwBursts.push({ t: 1600, x: fwRnd(w * 0.2, w * 0.8), y: fwRnd(h * 0.15, h * 0.6) })
}

const fwStep = (ts: number) => {
  if (!fwCtx || !fwCanvas) return
  const dt = ts - fwLastTime
  fwLastTime = ts
  fwCtx.globalCompositeOperation = 'source-over'
  fwCtx.fillStyle = 'rgba(0,0,0,0)'
  fwCtx.clearRect(0, 0, fwCanvas.width, fwCanvas.height)
  const g = 0.05
  const f = 0.985
  fwCtx.globalCompositeOperation = 'lighter'
  for (let i = fwParticles.length - 1; i >= 0; i--) {
    const p = fwParticles[i]
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
    if (p.life <= 0) fwParticles.splice(i, 1)
  }
  for (let i = fwBursts.length - 1; i >= 0; i--) {
    const b = fwBursts[i]
    b.t -= dt
    if (b.t <= 0) {
      addFwBurst(b.x, b.y)
      fwBursts.splice(i, 1)
    }
  }
  if (fwRunning) fwRafId = requestAnimationFrame(fwStep)
}

const startFireworks = () => {
  if (fwRunning || !festivalOverlay.value) return
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
  resizeFwCanvas()
  fwParticles = []
  fwBursts = []
  const w = fwCanvas.width
  const h = fwCanvas.height
  scheduleFwBursts(w, h)
  scheduleFwBursts(w, h)
  fwLastTime = performance.now()
  fwRunning = true
  fwRafId = requestAnimationFrame(fwStep)
  fwLoopId = window.setInterval(() => {
    if (!fwRunning || !fwCanvas) return
    scheduleFwBursts(fwCanvas.width, fwCanvas.height)
  }, 2200)
  window.setTimeout(() => stopFireworks(), 14000)
}

const stopFireworks = () => {
  fwRunning = false
  if (fwRafId) { cancelAnimationFrame(fwRafId); fwRafId = null }
  if (fwLoopId) { clearInterval(fwLoopId); fwLoopId = null }
  fwParticles = []
  fwBursts = []
  if (fwCanvas && festivalOverlay.value) {
    festivalOverlay.value.removeChild(fwCanvas)
  }
  fwCanvas = null
  fwCtx = null
}

watch(showFestivalEffects, (val) => {
  if (val) startFireworks()
  else stopFireworks()
})

// 全屏播放
const showFullPlay = ref(false)
let isFull = false

const toggleFullPlay = () => {
  if (!songInfo.value.songmid) return
  showFullPlay.value = !showFullPlay.value
}

const isFullPlayIdle = ref(false)
const handleIdleChange = (idle: boolean) => {
  isFullPlayIdle.value = idle
}

// 颜色计算
const maincolor = computed(() => player.value.coverDetail.mainColor || 'var(--td-brand-color-5)')
const startmaincolor = computed(() => {
  const c = player.value.coverDetail.ColorObject
  if (c) return `rgba(${c.r},${c.g},${c.b},.2)`
  return 'rgba(0, 0, 0, 1)'
})
const contrastTextColor = computed(
  () => player.value.coverDetail.textColor || 'var(--player-text-idle)'
)
const hoverColor = computed(
  () => player.value.coverDetail.hoverColor || 'var(--player-text-hover-idle)'
)
const playbg = computed(() => player.value.coverDetail.playBg || 'var(--player-btn-bg-idle)')
const playbghover = computed(
  () => player.value.coverDetail.playBgHover || 'var(--player-btn-bg-hover-idle)'
)

const bg = ref('var(--player-bg-default)')

watch(
  songInfo,
  async (newVal) => {
    bg.value = bg.value === 'var(--player-bg-idle)' ? 'var(--player-bg-default)' : bg.value
    if (!newVal.songmid) bg.value = 'var(--player-bg-idle)'
  },
  { deep: true, immediate: true }
)

watch(showFullPlay, (val) => {
  if (val) bg.value = '#00000020'
  else bg.value = 'var(--player-bg-default)'
})

// 音量同步
watch(
  () => userInfo.value.volume,
  (newVolume) => {
    if (newVolume !== undefined) setVolume(newVolume)
  },
  { immediate: true }
)

// 全局控制事件
function globalControls(e: Event) {
  const detail = (e as CustomEvent).detail
  if (detail?.name === 'toggleFullPlay') toggleFullPlay()
}

onMounted(() => {
  window.addEventListener('global-music-control', globalControls)
  const openPlaylistHandler = () => { showPlaylist.value = true }
  const closePlaylistHandler = () => { showPlaylist.value = false }
  window.addEventListener('open-playlist', openPlaylistHandler)
  window.addEventListener('close-playlist', closePlaylistHandler)
  ;(window as any).__open_playlist_handler__ = openPlaylistHandler
  ;(window as any).__close_playlist_handler__ = closePlaylistHandler
})

onUnmounted(() => {
  stopFireworks()
  window.removeEventListener('global-music-control', globalControls)
  try {
    const h = (window as any).__open_playlist_handler__
    if (h) window.removeEventListener('open-playlist', h)
  } catch {}
  try {
    const h2 = (window as any).__close_playlist_handler__
    if (h2) window.removeEventListener('close-playlist', h2)
  } catch {}
  window.removeEventListener('mousemove', handleVolumeDragMove)
  window.removeEventListener('mouseup', handleVolumeDragEnd)
  window.removeEventListener('mousemove', handleProgressDragMove)
  window.removeEventListener('mouseup', handleProgressDragEnd)
})

onActivated(() => {
  if (isFull) showFullPlay.value = true
})

onDeactivated(() => {
  isFull = showFullPlay.value
})
</script>

<template>
  <div
    class="player-container"
    :style="!showFullPlay && 'box-shadow: none'"
    :class="{ 'full-play-idle': isFullPlayIdle && showFullPlay }"
    @click.stop="toggleFullPlay"
  >
    <!-- 春节烟花 -->
    <div v-if="showFestivalEffects" ref="festivalOverlay" class="festival-overlay"></div>

    <!-- 进度条 -->
    <div class="progress-bar-container">
      <div
        ref="progressRef"
        class="progress-bar"
        @mousedown="handleProgressDragStart($event)"
        @click.stop="handleProgressClick"
      >
        <div class="progress-background"></div>
        <div
          v-if="crossfadeMarkVisible"
          class="crossfade-mark"
          :style="{ left: crossfadeMarkLeft + '%', width: crossfadeMarkWidth + '%' }"
        ></div>
        <div class="progress-filled" :style="{ width: `${progressPercentage}%` }"></div>
        <div class="progress-handle" :style="{ left: `${progressPercentage}%` }"></div>
      </div>
    </div>

    <div class="player-content">
      <!-- 左侧：封面和歌曲信息 -->
      <div class="left-section">
        <div v-if="songInfo.songmid" class="album-cover">
          <img :src="player.cover || '/default-cover.png'" alt="专辑封面" />
        </div>

        <div class="song-info">
          <div class="song-name">{{ songInfo.name }}</div>
          <div class="artist-name">{{ songInfo.singer }}</div>
        </div>

        <div class="left-actions">
          <t-tooltip content="下载">
            <t-button
              class="control-btn"
              variant="text"
              shape="circle"
              :disabled="!songInfo.songmid"
              @click.stop
            >
              <i class="iconfont icon-xiazai" style="font-size: 18px"></i>
            </t-button>
          </t-tooltip>
        </div>
      </div>

      <!-- 中间：播放控制 -->
      <div class="center-controls">
        <t-button class="control-btn" variant="text" shape="circle" @click.stop="playPrevious">
          <span class="iconfont icon-shangyishou"></span>
        </t-button>
        <button
          class="control-btn play-btn"
          :disabled="isLoadingSong"
          @click.stop="() => !isLoadingSong && togglePlayPause()"
        >
          <transition name="fade" mode="out-in">
            <div v-if="isLoadingSong" key="loading" class="loading-spinner play-loading"></div>
            <span v-else-if="Audio.isPlay" key="play" class="iconfont icon-zanting"></span>
            <span v-else key="pause" class="iconfont icon-bofang"></span>
          </transition>
        </button>
        <t-button class="control-btn" shape="circle" variant="text" @click.stop="playNext">
          <span class="iconfont icon-xiayishou"></span>
        </t-button>
      </div>

      <!-- 右侧：时间和其他控制 -->
      <div class="right-section">
        <div class="time-display">{{ currentTimeFormatted }} / {{ durationFormatted }}</div>

        <div class="extra-controls">
          <!-- 播放模式按钮 -->
          <t-tooltip :content="playModeTip">
            <t-button
              class="control-btn"
              shape="circle"
              variant="text"
              @click.stop="updatePlayMode"
            >
              <i :class="playModeIconClass" style="width: 1.5em"></i>
            </t-button>
          </t-tooltip>

          <!-- 音量控制 -->
          <div
            class="volume-control"
            @mouseenter="showVolumeSlider = true"
            @mouseleave="showVolumeSlider = false"
            @wheel.prevent="handleVolumeWheel"
          >
            <button class="control-btn">
              <i class="iconfont icon-shengyin" style="font-size: 18px"></i>
            </button>

            <transition name="volume-popup">
              <div v-show="showVolumeSlider" class="volume-slider-container" @click.stop>
                <div class="volume-slider">
                  <div
                    ref="volumeBarRef"
                    class="volume-bar"
                    @click="handleVolumeClick"
                    @mousedown="handleVolumeDragStart"
                  >
                    <div class="volume-background"></div>
                    <div class="volume-filled" :style="{ height: `${volumeValue}%` }"></div>
                    <div class="volume-handle" :style="{ bottom: `${volumeValue}%` }"></div>
                  </div>
                  <div class="volume-value">{{ volumeValue }}%</div>
                </div>
              </div>
            </transition>
          </div>

          <!-- 播放列表按钮 -->
          <t-tooltip content="播放列表">
            <n-badge :value="list.length" :max="99" color="#bbb">
              <t-button
                class="control-btn"
                shape="circle"
                variant="text"
                @click.stop="togglePlaylist"
              >
                <i class="iconfont icon-liebiao" style="font-size: 18px"></i>
              </t-button>
            </n-badge>
          </t-tooltip>
        </div>
      </div>
    </div>
  </div>

  <!-- 播放列表面板 (临时简易版，后续移植 PlaylistDrawer) -->
  <Transition name="playlist">
    <div v-if="showPlaylist" class="playlist-panel" @click.stop>
      <div class="playlist-header">
        <span>播放列表 ({{ list.length }})</span>
        <t-button variant="text" size="small" @click="closePlaylist">
          <i class="iconfont icon-guanbi"></i>
        </t-button>
      </div>
      <div class="playlist-body">
        <div
          v-for="(song, idx) in list"
          :key="song.songmid"
          class="playlist-item"
          :class="{ active: song.songmid === currentSongId }"
          @click="playSong(song)"
        >
          <span class="song-index">{{ idx + 1 }}</span>
          <span class="song-title">{{ song.name }}</span>
          <span class="song-artist">{{ song.singer }}</span>
        </div>
        <div v-if="list.length === 0" class="playlist-empty">播放列表为空</div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.fade-leave-active { transition: all 0.2s ease-in-out; }
.fade-enter-active { transition: all 0.1s ease-in-out; }
.fade-leave-to { opacity: 0; transform: rotate(180deg); }
.fade-enter-from { opacity: 0; transform: rotate(-180deg); }

.loading-spinner {
  border: 2px solid rgba(255, 255, 255, 0.3);
  border-top: 2px solid v-bind(hoverColor);
  border-radius: 50%;
  animation: spin 1s linear infinite;
  display: inline-block;
  width: 1em;
  height: 1em;
}

.play-loading {
  width: 20px !important;
  height: 20px !important;
  margin: 4px;
  border-width: 3px;
  border-color: rgba(255, 255, 255, 0.3);
  border-top-color: v-bind(hoverColor);
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

.player-container {
  box-shadow: 0px -2px 20px 0px #00000039;
  position: fixed;
  bottom: 0;
  left: 0;
  right: 0;
  transition: transform 0.5s cubic-bezier(0.34, 1.56, 0.64, 1), background 0.3s;
  background: v-bind(bg);
  backdrop-filter: blur(1000px);
  z-index: 1000;
  height: var(--play-bottom-height, 70px);
  display: flex;
  flex-direction: column;
}

.player-container.full-play-idle {
  transform: translateY(100%);
}

/* 进度条 */
.progress-bar-container {
  width: 100%;
  --touch-range-height: 20px;
  --play-line-height: 4px;
  height: calc(var(--touch-range-height) + var(--play-line-height));
  position: absolute;
  top: calc(var(--touch-range-height) / 2 * -1);
  cursor: pointer;
  transition: all 0.2s ease-in-out;
}

.progress-bar {
  width: 100%;
  height: 100%;
  position: relative;
}

.progress-bar .progress-background,
.progress-bar .progress-filled {
  position: absolute;
  left: 0;
  right: 0;
  height: var(--play-line-height);
  top: 50%;
  transform: translateY(-50%);
  border-radius: 999px;
}

.progress-bar .progress-background { background: transparent; }

.progress-bar .progress-filled {
  background: linear-gradient(to right, v-bind(startmaincolor), v-bind(maincolor) 80%);
}

.progress-bar .crossfade-mark {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  height: var(--play-line-height);
  border-radius: 999px;
  pointer-events: none;
  background: repeating-linear-gradient(
    45deg,
    rgba(255, 255, 255, 0.35),
    rgba(255, 255, 255, 0.35) 3px,
    rgba(255, 255, 255, 0.1) 3px,
    rgba(255, 255, 255, 0.1) 6px
  );
  box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.18);
  opacity: 0.75;
}

.progress-bar .progress-handle {
  position: absolute;
  top: 50%;
  width: 12px;
  height: 12px;
  background: v-bind(hoverColor);
  border-radius: 50%;
  transform: translate(-50%, -50%);
  opacity: 0;
}

.progress-bar:hover .progress-handle,
.progress-bar .progress-handle.dragging { opacity: 1; }

.progress-bar:hover .progress-background,
.progress-bar:hover .progress-filled,
.progress-bar:hover .crossfade-mark { height: 6px; }

/* 播放器内容 */
.player-content {
  user-select: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 40px;
  height: calc(100% - 4px);
}

/* 左侧 */
.left-section {
  display: flex;
  align-items: center;
  min-width: 0;
  flex: 1;
  padding-top: 2px;
}

.album-cover {
  width: 50px;
  height: 50px;
  border-radius: 4px;
  overflow: hidden;
  margin-right: 12px;
  flex-shrink: 0;
}

.album-cover img {
  user-select: none;
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.song-info { min-width: 0; }

.song-name {
  font-size: 14px;
  font-weight: 700;
  color: v-bind(hoverColor);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-bottom: 4px;
}

.artist-name {
  font-size: 12px;
  color: v-bind(contrastTextColor);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.left-actions {
  display: flex;
  align-items: center;
  gap: 3px;
  margin-left: 12px;
}

.left-actions .control-btn {
  background: transparent;
  border: none;
  color: v-bind(contrastTextColor);
  cursor: pointer;
  padding: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.left-actions .control-btn:hover { color: v-bind(hoverColor); }
.left-actions .control-btn:disabled { cursor: not-allowed; opacity: 0.6; }

/* 中间 */
.center-controls {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 16px;
  flex: 1;
}

.center-controls .control-btn {
  background: transparent;
  border: none;
  color: v-bind(contrastTextColor);
  cursor: pointer;
  padding: 5px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.center-controls .control-btn span { font-size: 28px; }
.center-controls .control-btn:hover { color: v-bind(hoverColor); }

.center-controls .control-btn.play-btn {
  background-color: v-bind(playbg);
  transition: background-color 0.2s ease;
  border-radius: 50%;
}

.center-controls .control-btn.play-btn span {
  font-size: 28px;
  font-weight: 800;
  color: v-bind(hoverColor);
}

.center-controls .control-btn.play-btn:hover {
  background-color: v-bind(playbghover);
  color: v-bind(contrastTextColor);
}

/* 右侧 */
.right-section {
  display: flex;
  align-items: center;
  gap: 16px;
  flex: 1;
  justify-content: flex-end;
}

.time-display {
  font-size: 12px;
  line-height: 12px;
  color: v-bind(contrastTextColor);
  white-space: nowrap;
}

.extra-controls {
  display: flex;
  align-items: center;
  gap: 12px;
}

.extra-controls .control-btn {
  background: transparent;
  border: none;
  color: v-bind(contrastTextColor);
  cursor: pointer;
  padding: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
}

.extra-controls .control-btn .iconfont { font-size: 18px; }
.extra-controls .control-btn:hover { color: v-bind(hoverColor); }

/* 音量控制 */
.volume-control { position: relative; }

.volume-slider-container {
  position: absolute;
  bottom: calc(100% + 10px);
  right: -10px;
  background: v-bind(contrastTextColor);
  backdrop-filter: blur(60px);
  border-radius: 8px;
  padding: 15px 10px;
  width: 40px;
  height: 150px;
  z-index: 1000;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
  display: flex;
  flex-direction: column;
  align-items: center;
  transform-origin: bottom center;
}

.volume-slider {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: space-between;
  height: 100%;
  width: 100%;
  gap: 8px;
}

.volume-value {
  font-size: 12px;
  color: v-bind(maincolor);
  margin-top: 8px;
}

.volume-bar {
  width: 4px;
  height: 100px;
  position: relative;
  cursor: pointer;
}

.volume-background {
  position: absolute;
  top: 0; left: 0; right: 0; bottom: 0;
  background: #ffffff71;
  border-radius: 2px;
}

.volume-filled {
  position: absolute;
  bottom: 0; left: 0; right: 0;
  background: v-bind(maincolor);
  border-radius: 2px;
}

.volume-handle {
  position: absolute;
  left: 50%;
  width: 12px;
  height: 12px;
  background: v-bind(maincolor);
  border-radius: 50%;
  transform: translate(-50%, 50%);
  opacity: 1;
}

.volume-popup-enter-active,
.volume-popup-leave-active {
  transition: opacity 0.2s cubic-bezier(0.8, 0, 0.8, 0.43),
              transform 0.2s cubic-bezier(0.8, 0, 0.8, 0.43);
}

.volume-popup-enter-from,
.volume-popup-leave-to {
  opacity: 0;
  transform: translateY(10px) scale(0.95);
}

/* 播放列表面板 */
.playlist-panel {
  position: fixed;
  right: 0;
  bottom: var(--play-bottom-height, 70px);
  width: 360px;
  max-height: 480px;
  background: var(--td-bg-color-container);
  border: 1px solid var(--td-border-level-1-color);
  border-radius: 8px 8px 0 0;
  box-shadow: 0 -4px 20px rgba(0, 0, 0, 0.15);
  z-index: 999;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.playlist-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 16px;
  border-bottom: 1px solid var(--td-border-level-1-color);
  font-weight: 500;
}

.playlist-body {
  flex: 1;
  overflow-y: auto;
}

.playlist-item {
  display: flex;
  align-items: center;
  padding: 8px 16px;
  cursor: pointer;
  transition: background 0.15s;
}

.playlist-item:hover { background: var(--td-bg-color-component-hover); }

.playlist-item.active {
  color: var(--td-brand-color);
  background: var(--td-brand-color-light);
}

.playlist-item .song-index {
  width: 28px;
  font-size: 12px;
  color: var(--td-text-color-secondary);
  flex-shrink: 0;
}

.playlist-item .song-title {
  flex: 1;
  font-size: 13px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.playlist-item .song-artist {
  width: 80px;
  font-size: 12px;
  color: var(--td-text-color-secondary);
  text-align: right;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.playlist-empty {
  padding: 40px;
  text-align: center;
  color: var(--td-text-color-placeholder);
}

.playlist-enter-active,
.playlist-leave-active {
  transition: all 0.25s ease;
}

.playlist-enter-from,
.playlist-leave-to {
  opacity: 0;
  transform: translateX(20px);
}

/* 春节烟花覆盖层 */
.festival-overlay {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  z-index: 0;
  pointer-events: none;
  overflow: hidden;
}

/* 响应式 */
@media (max-width: 768px) {
  .right-section .time-display { display: none; }
  .center-controls { gap: 8px; }
  .right-section .extra-controls { gap: 8px; }
}

@media (max-width: 576px) {
  .left-section .song-info { max-width: 120px; }
}
</style>
