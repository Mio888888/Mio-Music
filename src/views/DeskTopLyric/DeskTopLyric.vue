<script setup lang="ts">
import '@applemusic-like-lyrics/core/style.css'
import { ref, shallowRef, computed, onMounted, onBeforeUnmount, markRaw, watch, type Component } from 'vue'
import { listen, emit, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { LyricLine } from '@/types/lyric'
import { useBackgroundRender } from '@/composables/useBackgroundRender'
import { useSettingsStore } from '@/store/Settings'
import { storeToRefs } from 'pinia'

const LyricPlayerComp = shallowRef<Component | null>(null)
const lyricPlayerError = ref(false)

async function loadLyricPlayer() {
  try {
    const mod = await import('@applemusic-like-lyrics/vue')
    LyricPlayerComp.value = mod.LyricPlayer
  } catch (e) {
    console.error('[DeskTopLyric] Failed to load LyricPlayer:', e)
    lyricPlayerError.value = true
  }
}

const lyricLines = shallowRef<LyricLine[]>([])
const songInfo = ref({ name: '', singer: '' })
const isPlaying = ref(false)
const isLocked = ref(false)
const isHovering = ref(false)

// 背景渲染
const backgroundContainer = ref<HTMLDivElement | null>(null)
const settingsStore = useSettingsStore()
const { settings } = storeToRefs(settingsStore)

const backgroundConfig = computed(() => settings.value.backgroundRender?.desktopLyric)
const coverImage = ref('/src/assets/images/Default.jpg')
const isDragging = ref(false)

// 使用背景渲染 composable
const {
  isInitialized: bgInitialized,
  init: initBgRender,
  pause: pauseBgRender,
  resume: resumeBgRender,
  dispose: disposeBgRender
} = useBackgroundRender({
  container: backgroundContainer,
  enabled: computed(() => backgroundConfig.value?.enabled ?? false),
  config: computed(() => backgroundConfig.value ?? {
    preset: 'performance',
    enabled: false,
    audioResponse: false,
    renderScale: 0.3,
    flowSpeed: 0.5,
    staticMode: true,
    fps: 15
  }),
  coverImage,
  hasLyric: computed(() => lyricLines.value.length > 10),
  isPlaying
})

let baseMs = 0
let anchorTick = 0
const playSeekMs = ref(0)
let rafId: number | null = null

const unlisteners: UnlistenFn[] = []

interface StyleOption {
  fontSize?: number
  mainColor?: string
  shadowColor?: string
  fontWeight?: number
  fontFamily?: string
  position?: 'left' | 'center' | 'right' | 'both'
  showYrc?: boolean
  textBackgroundMask?: boolean
  backgroundMaskColor?: string
}

const styleOptions = ref<StyleOption>({})
const fontSize = computed(() => styleOptions.value.fontSize || 30)
const fontWeight = computed(() => styleOptions.value.fontWeight || 700)
const fontFamily = computed(() => styleOptions.value.fontFamily || '')
const resolvedFontFamily = computed(() => fontFamily.value || '-apple-system, BlinkMacSystemFont, sans-serif')
const lyricViewColor = computed(() => styleOptions.value.mainColor || 'rgba(255, 255, 255, 1)')
const lyricShadowColor = computed(() => styleOptions.value.shadowColor || 'rgba(0, 0, 0, 0.7)')
const lyricTextShadow = computed(() => `0 0 4px ${lyricShadowColor.value}, 0 0 10px ${lyricShadowColor.value}`)

const lyricPosition = computed(() => styleOptions.value.position || 'center')
const alignAnchor = computed(() => {
  if (lyricPosition.value === 'left') return 'left'
  if (lyricPosition.value === 'right') return 'right'
  return 'center'
})
const alignPosition = computed(() => {
  if (lyricPosition.value === 'left') return 0.2
  if (lyricPosition.value === 'right') return 0.8
  return 0.5
})
const lyricTextAlign = computed(() => {
  if (lyricPosition.value === 'left') return 'left'
  if (lyricPosition.value === 'right') return 'right'
  return 'center'
})

const showYrc = computed(() => styleOptions.value.showYrc !== false)
const textBackgroundMask = computed(() => styleOptions.value.textBackgroundMask === true)
const backgroundMaskColor = computed(() => styleOptions.value.backgroundMaskColor || 'rgba(0,0,0,0.5)')
const lyricLineBackground = computed(() => (textBackgroundMask.value ? backgroundMaskColor.value : 'transparent'))
const lyricLineShadow = computed(() => (textBackgroundMask.value ? `0 2px 12px ${backgroundMaskColor.value}` : 'none'))

const safeLyricLines = computed(() => {
  if (showYrc.value) return lyricLines.value

  return lyricLines.value.map((line: any) => {
    const words = Array.isArray(line?.words) ? line.words : []
    const text = words.map((w: any) => w?.word ?? '').join('')
    const startTime = Number(line?.startTime ?? words[0]?.startTime ?? 0)
    const endTime = Number(line?.endTime ?? words[words.length - 1]?.endTime ?? startTime + 1)
    return {
      ...line,
      words: [
        {
          word: text,
          startTime,
          endTime,
          obscene: false,
          romanWord: '',
        },
      ],
      startTime,
      endTime,
    }
  })
})
const currentTimeMs = computed(() => Math.floor(playSeekMs.value))

const placeholderText = computed(() => {
  if (lyricLines.value.length > 0) return ''
  if (songInfo.value.name) return `${songInfo.value.name} - ${songInfo.value.singer}`
  return 'Ceru Desktop Lyric'
})

// RAF Clock
function startRafLoop() {
  if (rafId !== null) return
  const tick = () => {
    if (isPlaying.value) {
      playSeekMs.value = baseMs + (performance.now() - anchorTick)
    } else {
      playSeekMs.value = baseMs
    }
    rafId = requestAnimationFrame(tick)
  }
  rafId = requestAnimationFrame(tick)
}

function stopRafLoop() {
  if (rafId !== null) {
    cancelAnimationFrame(rafId)
    rafId = null
  }
}

// IPC handlers
function handleLyricChange(lines: LyricLine[]) {
  console.log('[DeskTopLyric] Received lyric change, raw lines:', lines?.length, lines?.[0])
  const normalized = (lines || []).map((l: any) => ({
    ...l,
    translatedLyric: l.translatedLyric ?? '',
    romanLyric: l.romanLyric ?? '',
    isBG: l.isBG ?? false,
    isDuet: l.isDuet ?? false,
    words: (l.words || []).map((w: any) => ({
      ...w,
      word: w?.word ?? '',
      startTime: Number(w?.startTime ?? 0),
      endTime: Number(w?.endTime ?? 0),
      obscene: w?.obscene ?? false,
      romanWord: w?.romanWord ?? '',
    })),
    startTime: Number(l?.startTime ?? 0),
    endTime: Number(l?.endTime ?? 0),
  }))
  lyricLines.value = markRaw(normalized)
  console.log('[DeskTopLyric] Processed lyricLines:', lyricLines.value.length, lyricLines.value[0])
}

function handleSongChange(data: { name: string; singer: string }) {
  songInfo.value = {
    name: data?.name || '',
    singer: data?.singer || ''
  }
}

function handleProgress(data: { currentMs: number; timestamp: number }) {
  if (typeof data?.currentMs === 'number') {
    const newBase = Math.floor(data.currentMs)
    const drift = Math.abs(newBase - playSeekMs.value)
    if (drift > 300) {
      baseMs = newBase
      anchorTick = performance.now()
    }
  }
}

function handlePlayState(playing: boolean) {
  const wasPlaying = isPlaying.value
  isPlaying.value = !!playing
  if (isPlaying.value && !wasPlaying) {
    baseMs = playSeekMs.value
    anchorTick = performance.now()
  } else if (!isPlaying.value && wasPlaying) {
    baseMs = playSeekMs.value
    anchorTick = performance.now()
  }
}

// Controls
function emitControl(name: string, value?: boolean) {
  emit('desktop-lyric-control', { name, value }).catch(() => {})
}

function onPrev() { emitControl('playPrev') }
function onToggle() { emitControl('toggle') }
function onNext() { emitControl('playNext') }

function onToggleLock() {
  const next = !isLocked.value
  isLocked.value = next
  emitControl('lock', next)
}

function onClose() { emitControl('close') }

async function onDragStart(e: MouseEvent) {
  if (isLocked.value) return
  isDragging.value = true
  // 拖动时暂停背景渲染以节省资源
  if (bgInitialized.value) {
    pauseBgRender()
  }
  e.preventDefault()
  try {
    await getCurrentWindow().startDragging()
  } catch {}
}

async function onDragEnd() {
  isDragging.value = false
  // 拖动结束后恢复背景渲染
  if (bgInitialized.value && backgroundConfig.value?.enabled) {
    resumeBgRender()
  }
}

// Lifecycle
onMounted(async () => {
  // Load AMLL LyricPlayer component (may fail in some WebView contexts)
  loadLyricPlayer()

  try {
    const saved = await (window as any).electron?.ipcRenderer?.invoke('get-desktop-lyric-option')
    if (saved) styleOptions.value = saved
  } catch {}

  unlisteners.push(
    await listen<StyleOption>('desktop-lyric-style-change', (event) => {
      styleOptions.value = event.payload
    })
  )

  unlisteners.push(
    await listen<LyricLine[]>('desktop-lyric-change', (event) => {
      handleLyricChange(event.payload)
    })
  )

  unlisteners.push(
    await listen<{ name: string; singer: string }>('desktop-song-change', (event) => {
      handleSongChange(event.payload)
    })
  )

  // 监听封面变化事件
  unlisteners.push(
    await listen<string>('desktop-cover-change', (event) => {
      if (event.payload) {
        coverImage.value = event.payload
      }
    })
  )

  unlisteners.push(
    await listen<{ currentMs: number; timestamp: number }>('desktop-lyric-progress', (event) => {
      handleProgress(event.payload)
    })
  )

  unlisteners.push(
    await listen<boolean>('desktop-lyric-play-state', (event) => {
      handlePlayState(event.payload)
    })
  )

  startRafLoop()
  console.log('[DeskTopLyric] Component mounted, emitting ready event')
  emit('desktop-lyric-ready', {}).catch(() => {})
})

onBeforeUnmount(() => {
  stopRafLoop()
  // 清理背景渲染
  disposeBgRender()
  unlisteners.forEach((fn) => {
    try { fn() } catch {}
  })
  unlisteners.length = 0
})
</script>

<template>
  <div
    :class="['desktop-lyric', { locked: isLocked, hovered: isHovering }]"
    @mouseenter="isHovering = true"
    @mouseleave="isHovering = false"
    @pointerup="onDragEnd"
  >
    <!-- 背景渲染容器 -->
    <div
      v-if="backgroundConfig?.enabled"
      ref="backgroundContainer"
      class="desktop-lyric-background"
    ></div>
    <!-- AMLL LyricPlayer -->
    <component
      :is="LyricPlayerComp"
      v-if="LyricPlayerComp && lyricLines.length > 0"
      :lyric-lines="safeLyricLines"
      :current-time="currentTimeMs"
      :playing="isPlaying"
      :enable-blur="false"
      :enable-spring="false"
      :enable-scale="false"
      :align-anchor="alignAnchor"
      :align-position="alignPosition"
      :style="{ textAlign: lyricTextAlign }"
      class="desktop-lyric-player"
    />

    <!-- Drag handle (top area, visible on hover when not locked) -->
    <div
      v-if="isHovering && !isLocked"
      class="drag-handle"
      @pointerdown="onDragStart"
    >
      <div class="drag-dots">
        <span></span><span></span><span></span>
      </div>
    </div>

    <!-- Error state -->
    <div v-else-if="lyricPlayerError" class="lyric-placeholder">
      <span>歌词组件加载失败</span>
    </div>

    <!-- Placeholder when no lyrics -->
    <div v-else class="lyric-placeholder">
      <span>{{ placeholderText }}</span>
    </div>

    <!-- Control bar (shown on hover, not when locked) -->
    <Transition name="controls">
      <div v-if="isHovering && !isLocked" class="controls-bar" @pointerdown.stop>
        <div class="song-info" v-if="songInfo.name">
          {{ songInfo.name }} - {{ songInfo.singer }}
        </div>
        <div class="control-buttons">
          <button class="ctrl-btn" title="上一首" @click="onPrev">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
              <path d="M6 6h2v12H6zm3.5 6l8.5 6V6z"/>
            </svg>
          </button>
          <button class="ctrl-btn" :title="isPlaying ? '暂停' : '播放'" @click="onToggle">
            <svg v-if="isPlaying" width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
              <path d="M6 4h4v16H6zM14 4h4v16h-4z"/>
            </svg>
            <svg v-else width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
              <path d="M8 5v14l11-7z"/>
            </svg>
          </button>
          <button class="ctrl-btn" title="下一首" @click="onNext">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
              <path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z"/>
            </svg>
          </button>
          <button class="ctrl-btn" :title="isLocked ? '解锁' : '锁定'" @click="onToggleLock">
            <svg v-if="isLocked" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>
              <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
            </svg>
            <svg v-else width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>
              <path d="M7 11V7a5 5 0 0 1 9.9-1"/>
            </svg>
          </button>
          <button class="ctrl-btn" title="关闭" @click="onClose">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="18" y1="6" x2="6" y2="18"/>
              <line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped lang="scss">
.desktop-lyric {
  width: 100vw;
  height: 100vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  background: transparent;
  user-select: none;
  position: relative;
  color: #fff;
  font-family: -apple-system, BlinkMacSystemFont, 'PingFangSC-Semibold', 'Segoe UI', Roboto,
    sans-serif;
  font-weight: v-bind(fontWeight);
  opacity: 0.6;
  transition: opacity 0.3s ease;

  &.hovered {
    opacity: 1;
  }

  &.locked {
    pointer-events: none;
    opacity: 0.4;
  }
}

.desktop-lyric-background {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  z-index: -1;
  pointer-events: none;
}

.desktop-lyric-player {
  width: 100%;
  height: 100%;
  --amll-lyric-view-color: v-bind(lyricViewColor);
  --amll-lp-color: v-bind(lyricViewColor);
  --amll-lyric-player-font-size: v-bind(fontSize + 'px');
  --amll-lp-font-size: v-bind(fontSize + 'px');
  --amll-lyric-player-font-weight: v-bind(fontWeight);
  font-family: v-bind(resolvedFontFamily);
  text-shadow: v-bind(lyricTextShadow);

  :deep(.FmKaba_lyricMainLine),
  :deep(.FmKaba_lyricSubLine),
  :deep([class*='lyricMainLine']),
  :deep([class*='lyricSubLine']) {
    text-shadow: v-bind(lyricTextShadow) !important;
    font-weight: v-bind(fontWeight) !important;
  }

  :deep(.FmKaba_lyricLine),
  :deep(.amll-lyric-player),
  :deep([class*='lyricLine']) {
    background-color: v-bind(lyricLineBackground) !important;
    box-shadow: v-bind(lyricLineShadow) !important;
  }

  :deep(.FmKaba_lyricBgLine),
  :deep([class*='lyricBgLine']) {
    opacity: 0 !important;
  }

  :deep([class*='romanWord']) {
    font-size: calc(var(--amll-lp-font-size) * 0.5) !important;
    font-family: v-bind(resolvedFontFamily) !important;
    opacity: 0.8;
  }
  :deep([class*='lyricSubLine']) {
    font-weight: v-bind(fontWeight) !important;
  }
}

.lyric-placeholder {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  font-size: v-bind(fontSize + 'px');
  font-weight: v-bind(fontWeight);
  color: v-bind(lyricViewColor);
  text-shadow: v-bind(lyricTextShadow);
  font-family: v-bind(resolvedFontFamily);
}

/* Drag handle */
.drag-handle {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: grab;
  z-index: 10;
  background: linear-gradient(180deg, rgba(0, 0, 0, 0.3), transparent);

  &:active {
    cursor: grabbing;
  }
}

.drag-dots {
  display: flex;
  gap: 3px;
  padding: 4px 8px;

  span {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.5);
  }
}

/* Controls bar */
.controls-bar {
  position: absolute;
  bottom: 10px;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 12px;
  background: rgba(0, 0, 0, 0.6);
  backdrop-filter: blur(10px);
  padding: 8px 16px;
  border-radius: 20px;
}

.song-info {
  font-size: 12px;
  color: rgba(255, 255, 255, 0.8);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 300px;
}

.control-buttons {
  display: flex;
  gap: 8px;
  align-items: center;
}

.ctrl-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  background: none;
  border: none;
  color: rgba(255, 255, 255, 0.8);
  cursor: pointer;
  padding: 4px;
  border-radius: 50%;
  transition: all 0.2s;

  &:hover {
    color: #fff;
    background: rgba(255, 255, 255, 0.2);
  }

  &:active {
    transform: scale(0.95);
  }
}

/* Controls transition */
.controls-enter-active,
.controls-leave-active {
  transition: all 0.2s ease;
}

.controls-enter-from,
.controls-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(10px);
}
</style>

<style>
html,
body,
#app {
  background-color: transparent !important;
  background: transparent !important;
}

#app {
  border-radius: 0 !important;
  overflow: visible !important;
}
</style>
