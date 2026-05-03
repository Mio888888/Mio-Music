<script setup lang="ts">
import '@applemusic-like-lyrics/core/style.css'
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { listen, emit, type UnlistenFn } from '@tauri-apps/api/event'
import { LyricPlayer } from '@applemusic-like-lyrics/vue'
import type { LyricLine } from '@/types/lyric'

const lyricLines = ref<LyricLine[]>([])
const songInfo = ref({ name: '', singer: '' })
const isPlaying = ref(false)
const isLocked = ref(false)
const isHovering = ref(false)

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
}

const styleOptions = ref<StyleOption>({})
const fontSize = computed(() => styleOptions.value.fontSize || 30)
const fontWeight = computed(() => styleOptions.value.fontWeight || 700)
const fontFamily = computed(() => styleOptions.value.fontFamily || '')
const resolvedFontFamily = computed(() => fontFamily.value || '-apple-system, BlinkMacSystemFont, sans-serif')
const lyricViewColor = computed(() => styleOptions.value.mainColor || 'rgba(255, 255, 255, 1)')

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
  lyricLines.value = lines || []
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

// Lifecycle
onMounted(async () => {
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
  emit('desktop-lyric-ready', {}).catch(() => {})
})

onBeforeUnmount(() => {
  stopRafLoop()
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
  >
    <!-- AMLL LyricPlayer -->
    <LyricPlayer
      v-if="lyricLines.length > 0"
      :lyric-lines="lyricLines"
      :current-time="currentTimeMs"
      :playing="isPlaying"
      :enable-blur="false"
      :enable-spring="false"
      :enable-scale="false"
      :align-position="0.35"
      class="desktop-lyric-player"
    />

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

  &.locked {
    pointer-events: none;
  }
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
  text-shadow: 0 0 4px rgba(255, 255, 255, 0.5);
  font-family: v-bind(resolvedFontFamily);
  opacity: 0.6;
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
body {
  background-color: transparent !important;
}
</style>
